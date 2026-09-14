//! Report and check an `anim-trace` NDJSON file.
//!
//! Usage:
//!   cargo run -p winia --example anim_trace_report -- <file.ndjson> [--subject hero] [--stall-ms 120]
//!
//! Prints, per animated subject, the trajectory (painted rect + opacity + progress) and then runs the
//! checks that caught real defects in this project: stalls (the painted rect stops changing while the
//! animation is unfinished), jumps, an end whose opacity reaches zero before its geometry settles,
//! a flight whose two ends settle at different times, and a subject that does not end at the target
//! rect its own `resolve` event announced.
//!
//! Exits non-zero when any check fails, so a scripted run can gate on it.

use serde_json::Value;
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.get(1) {
        Some(p) => p.clone(),
        None => {
            eprintln!("usage: anim_trace_report <file.ndjson> [--subject <substr>] [--stall-ms <n>]");
            std::process::exit(2);
        }
    };
    let mut subject_filter: Option<String> = None;
    let mut stall_ms: f64 = 120.0;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--subject" => {
                subject_filter = args.get(i + 1).cloned();
                i += 2;
            }
            "--stall-ms" => {
                stall_ms = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(stall_ms);
                i += 2;
            }
            _ => i += 1,
        }
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        }
    };

    // subject -> records, and flight-id -> ends, plus the target rect each resolve announced.
    let mut rows: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut events: Vec<Value> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let subject = v.get("subject").and_then(|s| s.as_str()).unwrap_or("?").to_string();
        let kind = v.get("kind").and_then(|s| s.as_str()).unwrap_or("");
        if kind == "event" {
            events.push(v);
        } else if kind == "flight" {
            // Key by subject AND flight id: one subject is reused across navigations (the same hero
            // flies out and back), and treating those as one continuous trajectory invents anomalies
            // (measured: an "opacity reached 0 early" that was really the push flight's start opacity
            // compared against the return flight's settle time).
            let id = v.get("flight").and_then(|f| f.as_i64()).unwrap_or(-1);
            rows.entry(format!("{subject}|{id}")).or_default().push(v);
        }
    }
    if rows.is_empty() {
        eprintln!("no flight records in {path}");
        std::process::exit(2);
    }

    let t0 = rows
        .values()
        .filter_map(|v| v.first())
        .filter_map(|r| r.get("t_ms").and_then(|t| t.as_f64()))
        .fold(f64::INFINITY, f64::min);

    let num = |v: &Value, path: &[&str]| -> Option<f64> {
        let mut cur = v;
        for p in path {
            cur = cur.get(p)?;
        }
        cur.as_f64()
    };
    let rect = |v: &Value| -> Option<(f64, f64, f64, f64)> {
        let r = v.get("painted")?;
        if r.is_null() {
            return None;
        }
        Some((
            r.get("x")?.as_f64()?,
            r.get("y")?.as_f64()?,
            r.get("w")?.as_f64()?,
            r.get("h")?.as_f64()?,
        ))
    };

    let mut failures: Vec<String> = Vec::new();

    for (key, recs) in &rows {
        // `key` is "<subject>|<flight id>"; everything user-facing wants just the subject part.
        let subject = key.split('|').next().unwrap_or(key.as_str()).to_string();
        if let Some(f) = &subject_filter {
            if !subject.contains(f.as_str()) {
                continue;
            }
        }
        println!("=== {subject} ({} records) ===", recs.len());
        let step = (recs.len() / 8).max(1);
        for r in recs.iter().step_by(step) {
            let t = num(r, &["t_ms"]).unwrap_or(0.0) - t0;
            match rect(r) {
                Some((_, _, w, h)) => println!(
                    "   t{:8.0}ms  painted={:6.1}x{:<6.1} alpha={:5.3} p={:5.3}",
                    t,
                    w,
                    h,
                    num(r, &["alpha"]).unwrap_or(f64::NAN),
                    num(r, &["progress"]).unwrap_or(f64::NAN)
                ),
                None => println!("   t{:8.0}ms  (no painted rect)", t),
            }
        }

        // 1. stall: the painted rect frozen while the animation is unfinished.
        let mut stall_start: Option<(f64, (f64, f64, f64, f64))> = None;
        for r in recs {
            let (Some(t), Some(rc), Some(p)) = (
                num(r, &["t_ms"]),
                rect(r),
                num(r, &["progress"]),
            ) else {
                continue;
            };
            if p >= 0.99 {
                stall_start = None;
                continue;
            }
            match stall_start {
                Some((t_start, rc_start)) if rc_start == rc => {
                    if t - t_start > stall_ms {
                        let msg = format!(
                            "STALL: {subject} painted rect frozen at {:.0}x{:.0} for {:.0}ms (p={:.2})",
                            rc.2,
                            rc.3,
                            t - t_start,
                            p
                        );
                        if !failures.contains(&msg) {
                            println!("   !! {msg}");
                            failures.push(msg);
                        }
                    }
                }
                _ => stall_start = Some((t, rc)),
            }
        }

        // 2. jumps between consecutive records.
        for w in recs.windows(2) {
            let (Some(a), Some(b)) = (rect(&w[0]), rect(&w[1])) else { continue };
            let d = (a.0 - b.0).abs().max((a.1 - b.1).abs()).max((a.2 - b.2).abs()).max((a.3 - b.3).abs());
            if d > 40.0 {
                let msg = format!(
                    "JUMP: {subject} rect moved {d:.0}px in one record ({:.0}x{:.0} -> {:.0}x{:.0})",
                    a.2, a.3, b.2, b.3
                );
                if !failures.contains(&msg) {
                    println!("   !! {msg}");
                    failures.push(msg);
                }
            }
        }

        // 3. opacity must not be spent before the geometry settles. The two ends mean different things:
        //    a LEAVING end fades to zero while shrinking, so hitting zero long before the geometry
        //    settles is the defect (measured earlier: the ghost finished fading at p=0.20); an ENTERING
        //    end correctly STARTS at zero, so for it the defect is never fading in at all (measured
        //    earlier: a same-screen morph held alpha at 1 while its geometry moved).
        let last_p = recs
            .iter()
            .filter_map(|r| num(r, &["progress"]))
            .fold(0.0, f64::max);
        let is_target = subject.ends_with("#Target") || subject.ends_with("#Morph");
        if last_p > 0.0 && is_target {
            let max_alpha = recs
                .iter()
                .filter_map(|r| num(r, &["alpha"]))
                .fold(0.0, f64::max);
            if max_alpha <= 0.01 {
                let msg = format!(
                    "OPACITY MISSING: {subject} never fades in (max alpha {max_alpha:.3}) while its \
                     geometry moves to p={last_p:.2}"
                );
                println!("   !! {msg}");
                failures.push(msg);
            }
        } else if last_p > 0.0 && !is_target {
            // Skip the first record: a source is legitimately opaque at p=0.
            let zero_at = recs
                .iter()
                .skip(1)
                .find(|r| num(r, &["alpha"]).unwrap_or(1.0) <= 0.01)
                .and_then(|r| num(r, &["t_ms"]));
            let done_at = recs
                .iter()
                .find(|r| num(r, &["progress"]).unwrap_or(0.0) >= 0.99)
                .and_then(|r| num(r, &["t_ms"]));
            if let (Some(z), Some(d)) = (zero_at, done_at) {
                if z < d - 50.0 {
                    let msg = format!(
                        "OPACITY EARLY: {subject} alpha reached 0 {:.0}ms before the geometry settled",
                        d - z
                    );
                    println!("   !! {msg}");
                    failures.push(msg);
                }
            }
        }

        // 4. the end rect the flight announced must be where it finishes.
        if let Some(rr) = events.iter().find(|e| {
            let d = e.get("detail").and_then(|d| d.as_str()).unwrap_or("");
            d.contains(subject.as_str()) && d.contains("end=(")
        }) {
            let d = rr.get("detail").and_then(|d| d.as_str()).unwrap_or("");
            if let Some(rest) = d.split("end=(").nth(1) {
                let nums: Vec<f64> = rest
                    .trim_end_matches(')')
                    .split(',')
                    .filter_map(|v| v.trim().parse().ok())
                    .collect();
                if nums.len() == 2 {
                    let last = recs.iter().rev().find_map(rect);
                    if let Some((_, _, w, h)) = last {
                        if (w - nums[0]).abs() > 2.0 || (h - nums[1]).abs() > 2.0 {
                            let msg = format!(
                                "TARGET MISMATCH: {subject} announced end {:.0}x{:.0} but settled at {:.0}x{:.0}",
                                nums[0], nums[1], w, h
                            );
                            println!("   !! {msg}");
                            failures.push(msg);
                        }
                    }
                }
            }
        }
    }

    // 5. the two ends of one flight should settle together. Ends of one flight share the part of
    // their subject before '#', e.g. "flight:hero#Source" / "flight:hero#Target".
    let mut by_flight: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for subject in rows.keys() {
        if let Some((base, role)) = subject.split_once('#') {
            if role == "Source" || role == "Target" {
                by_flight.entry(base.to_string()).or_default().push(subject.as_str());
            }
        }
    }
    for (base, subjects) in by_flight {
        let mut settles: Vec<(String, f64)> = Vec::new();
        for s in subjects {
            if let Some(t) = rows.get(s).and_then(|recs| {
                recs.iter()
                    .find(|r| num(r, &["progress"]).unwrap_or(0.0) >= 0.99)
                    .and_then(|r| num(r, &["t_ms"]))
            }) {
                settles.push((s.to_string(), t));
            }
        }
        if settles.len() == 2 {
            let d = (settles[0].1 - settles[1].1).abs();
            if d > 200.0 {
                let msg = format!(
                    "SETTLE MISMATCH: {base} ends settle {d:.0}ms apart ({} vs {})",
                    settles[0].0, settles[1].0
                );
                println!("!! {msg}");
                failures.push(msg);
            }
        }
    }

    println!();
    if failures.is_empty() {
        println!("OK: no anomalies (stall>{stall_ms:.0}ms, jump>40px, opacity/geometry, target, settle)");
    } else {
        println!("{} anomalie(s):", failures.len());
        for f in &failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
