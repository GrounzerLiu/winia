//! Report and check an `anim-trace` NDJSON file.
//!
//! Usage:
//!   cargo run -p winia --example anim_trace_report -- <file.ndjson> [--subject hero] [--stall-ms 120]
//!
//! Prints, per flight end, the trajectory (painted rect + opacity + progress) and then runs the checks
//! that caught real defects in this project:
//!
//! - STALL: the painted rect stops changing while the animation is unfinished.
//! - JUMP: the rect moves faster than a threshold, measured per second so one slow frame is not a jump.
//! - OPACITY EARLY / MISSING: the leaving end fades out before its geometry settles, or the entering end
//!   never fades in at all (the same-screen-morph case).
//! - TARGET MISMATCH: the `end` rect the flight's own `resolve` event announced is not where it settles.
//! - SETTLE MISMATCH: the two ends of one flight settle at different times.
//! - INCOMPLETE: a flight stops before progress 1 without a cancel event — a truncated trace used to be
//!   reported as clean, which made this useless as a gate.
//!
//! Exits 1 when any check fails and 2 when the input cannot be checked at all, so a scripted run can gate
//! on it. Records of kind `node`/`scene` are counted and reported but not checked: they are not flights.
//!
//! Rows are keyed by subject AND flight id — one subject is reused across navigations (the same hero flies
//! out and back), and treating those as one trajectory invents anomalies.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
struct RowKey {
    subject: String,
    flight: i64,
}

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
    let mut jump_px_per_s: f64 = 4000.0;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--subject" => {
                let Some(v) = args.get(i + 1) else {
                    eprintln!("--subject needs a value");
                    std::process::exit(2);
                };
                subject_filter = Some(v.clone());
                i += 2;
            }
            "--stall-ms" => {
                let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) else {
                    eprintln!("--stall-ms needs a number");
                    std::process::exit(2);
                };
                stall_ms = v;
                i += 2;
            }
            "--jump-px-per-s" => {
                let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) else {
                    eprintln!("--jump-px-per-s needs a number");
                    std::process::exit(2);
                };
                jump_px_per_s = v;
                i += 2;
            }
            other => {
                // A typo silently keeping the default was measured as a usability trap.
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        }
    };

    let mut rows: BTreeMap<RowKey, Vec<Value>> = BTreeMap::new();
    let mut events: Vec<Value> = Vec::new();
    let mut skipped = 0usize;
    let mut lines = 0usize;
    let mut other_kinds: BTreeMap<String, usize> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        lines += 1;
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                // Dropping these silently let a 75 %-unparseable file report "OK".
                skipped += 1;
                continue;
            }
        };
        let kind = v.get("kind").and_then(|s| s.as_str()).unwrap_or("");
        match kind {
            "event" => events.push(v),
            "flight" => {
                let subject = v.get("subject").and_then(|s| s.as_str()).unwrap_or("?").to_string();
                let flight = v.get("flight").and_then(|f| f.as_i64()).unwrap_or(-1);
                rows.entry(RowKey { subject, flight }).or_default().push(v);
            }
            other => {
                *other_kinds.entry(other.to_string()).or_default() += 1;
            }
        }
    }
    if skipped > 0 {
        eprintln!(
            "WARNING: {skipped} of {lines} lines did not parse and were skipped{}",
            if skipped * 4 > lines { " (more than a quarter: results are unreliable)" } else { "" }
        );
    }
    if skipped * 4 > lines {
        std::process::exit(2);
    }
    if !other_kinds.is_empty() {
        let summary: Vec<String> = other_kinds.iter().map(|(k, v)| format!("{k}={v}")).collect();
        println!("(non-flight records present, not checked: {})", summary.join(", "));
    }
    if rows.is_empty() {
        eprintln!("no flight records in {path}: nothing to check (node/scene-only trace?)");
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
    let role_of = |subject: &str| subject.rsplit_once('#').map(|(_, r)| r.to_string());
    let key_of = |subject: &str| {
        subject.split_once('#').map(|(b, _)| b).unwrap_or(subject).to_string()
    };

    let mut failures: Vec<String> = Vec::new();
    let mut matched_any = false;
    let mut settled_groups = 0usize;
    let mut total_groups = 0usize;

    for (row, recs) in &rows {
        let subject = row.subject.clone();
        if let Some(f) = &subject_filter {
            if !subject.contains(f.as_str()) {
                continue;
            }
        }
        matched_any = true;
        total_groups += 1;
        println!("=== {subject} (flight {}, {} records) ===", row.flight, recs.len());
        let step = (recs.len() / 8).max(1);
        for r in recs.iter().step_by(step) {
            let t = num(r, &["t_ms"]).unwrap_or(0.0) - t0;
            match rect(r) {
                Some((x, y, w, h)) => println!(
                    "   t{:8.0}ms  painted=({x:6.1},{y:6.1}) {:6.1}x{:<6.1} alpha={:5.3} p={:5.3}",
                    t,
                    w,
                    h,
                    num(r, &["alpha"]).unwrap_or(f64::NAN),
                    num(r, &["progress"]).unwrap_or(f64::NAN)
                ),
                None => println!("   t{:8.0}ms  (no painted rect)", t),
            }
        }

        let last_p = recs.iter().filter_map(|r| num(r, &["progress"])).fold(0.0, f64::max);
        let settled = last_p >= 0.99;
        if settled {
            settled_groups += 1;
        }
        let base_key = key_of(&subject);
        let cancelled = events.iter().any(|e| {
            e.get("phase").and_then(|p| p.as_str()) == Some("cancel")
                && e.get("subject").and_then(|s| s.as_str()) == Some(base_key.as_str())
        });

        // 1. stall: the painted rect frozen while the animation is unfinished. A flight whose start and
        //    end rects are EQUAL moves nothing on purpose (an opacity-only morph), and a rect that only
        //    moves in the last hundredth of a pixel is not a stall either, so both are excluded.
        let first_rect = recs.iter().find_map(rect);
        let last_rect = recs.iter().rev().find_map(rect);
        let constant_rect = match (first_rect, last_rect) {
            (Some(a), Some(b)) => {
                (a.0 - b.0).abs() < 0.5
                    && (a.1 - b.1).abs() < 0.5
                    && (a.2 - b.2).abs() < 0.5
                    && (a.3 - b.3).abs() < 0.5
            }
            _ => false,
        };
        if !constant_rect {
            let mut stall_start: Option<(f64, (f64, f64, f64, f64))> = None;
            for r in recs {
                let (Some(t), Some(rc), Some(p)) =
                    (num(r, &["t_ms"]), rect(r), num(r, &["progress"]))
                else {
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
        }

        // 2. jumps between consecutive records, per SECOND: the same displacement is a fast flight in a
        //    16 ms record and a smooth glide in a 100 ms one, and one frame hitch made the per-record form
        //    fire on healthy animations.
        for w in recs.windows(2) {
            let (Some(a), Some(b)) = (rect(&w[0]), rect(&w[1])) else {
                continue;
            };
            let (Some(ta), Some(tb)) = (num(&w[0], &["t_ms"]), num(&w[1], &["t_ms"])) else {
                continue;
            };
            let dt = (tb - ta) / 1000.0;
            if dt <= 0.0 || dt > 0.5 {
                continue; // an unknown or absurd interval is not evidence of a jump
            }
            let d = (a.0 - b.0)
                .abs()
                .max((a.1 - b.1).abs())
                .max((a.2 - b.2).abs())
                .max((a.3 - b.3).abs());
            let speed = d / dt;
            if speed > jump_px_per_s {
                let msg = format!(
                    "JUMP: {subject} moved {d:.0}px in {:.0}ms ({speed:.0}px/s) ({:.0},{:.0}) -> ({:.0},{:.0})",
                    dt * 1000.0,
                    a.0,
                    a.1,
                    b.0,
                    b.1
                );
                if !failures.contains(&msg) {
                    println!("   !! {msg}");
                    failures.push(msg);
                }
            }
        }

        // 3. opacity must not be spent before the geometry settles. The two ends mean different things: a
        //    LEAVING end fades to zero while shrinking, so hitting zero long before the geometry settles is
        //    the defect; an ENTERING end correctly STARTS at zero, so for it the defect is never fading in.
        let is_target =
            matches!(role_of(&subject).as_deref(), Some("Target") | Some("Morph"));
        if last_p > 0.0 && is_target {
            let max_alpha = recs.iter().filter_map(|r| num(r, &["alpha"])).fold(0.0, f64::max);
            if max_alpha <= 0.01 {
                let msg = format!(
                    "OPACITY MISSING: {subject} never fades in (max alpha {max_alpha:.3}) while its \
                     geometry moves to p={last_p:.2}"
                );
                println!("   !! {msg}");
                failures.push(msg);
            }
        } else if last_p > 0.0 {
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

        // 4. the end rect the flight announced must be where it finishes. The `resolve` event carries the
        //    key in its `subject` (`flight:<key>`) and the flight id inside its detail; the old predicate
        //    looked for the full `flight:<key>#<role>` string in the detail, which the emitter never
        //    writes, so this check could never fire.
        if role_of(&subject).as_deref() == Some("Target") {
            let announced = events.iter().find_map(|e| {
                let subj_matches =
                    e.get("subject").and_then(|s| s.as_str()) == Some(base_key.as_str());
                let detail = e.get("detail").and_then(|d| d.as_str()).unwrap_or("");
                let id_matches = detail.contains(&format!("id={}", row.flight));
                if subj_matches && id_matches && detail.contains("end=(") {
                    detail
                        .split("end=(")
                        .nth(1)
                        .and_then(|rest| rest.split(')').next())
                        .and_then(|pair| {
                            let nums: Vec<f64> =
                                pair.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                            (nums.len() == 2).then_some((nums[0], nums[1]))
                        })
                } else {
                    None
                }
            });
            if let Some((ew, eh)) = announced {
                if let Some((_, _, w, h)) = last_rect {
                    if (w - ew).abs() > 2.0 || (h - eh).abs() > 2.0 {
                        let msg = format!(
                            "TARGET MISMATCH: {subject} announced end {ew:.0}x{eh:.0} but settled at {w:.0}x{h:.0}"
                        );
                        println!("   !! {msg}");
                        failures.push(msg);
                    }
                }
            }
        }

        // 5. a flight that never reached progress 1 and was never cancelled means the trace stops mid
        //    animation (truncated run, crash, or the process was killed) — previously that reported "OK".
        if !settled && !cancelled && recs.len() > 2 {
            let msg = format!(
                "INCOMPLETE: {subject} stops at p={last_p:.3} with no cancel event (truncated trace?)"
            );
            println!("   !! {msg}");
            failures.push(msg);
        }
    }

    if let Some(f) = &subject_filter {
        if !matched_any {
            eprintln!("--subject {f} matched no subject in {path}");
            std::process::exit(2);
        }
    }

    // 6. the two ends of one flight should settle together.
    let mut by_flight: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
    for (row, recs) in &rows {
        let base = key_of(&row.subject);
        let Some(role) = role_of(&row.subject) else { continue };
        if role != "Source" && role != "Target" {
            continue;
        }
        if let Some(t) = recs
            .iter()
            .find(|r| num(r, &["progress"]).unwrap_or(0.0) >= 0.99)
            .and_then(|r| num(r, &["t_ms"]))
        {
            by_flight
                .entry(format!("{base}#{}", row.flight))
                .or_default()
                .push((row.subject.clone(), t));
        }
    }
    for (base, settles) in by_flight {
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
    if settled_groups == 0 && total_groups > 0 {
        println!(
            "WARNING: none of the {total_groups} flight group(s) reached progress 1 — the trace covers \
             only part of an animation"
        );
    }
    if failures.is_empty() {
        println!(
            "OK: no anomalies ({settled_groups}/{total_groups} groups settled; stall>{stall_ms:.0}ms, \
             jump>{jump_px_per_s:.0}px/s, opacity/geometry, target, settle)"
        );
    } else {
        println!("{} anomalies:", failures.len());
        for f in &failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
