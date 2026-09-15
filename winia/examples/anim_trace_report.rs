//! Report and check an `anim-trace` NDJSON file.
//!
//! Usage:
//!   cargo run -p winia --example anim_trace_report -- <file.ndjson> [--subject <substr>]
//!       [--stall-ms 120] [--jump-px-per-s 4000]
//!
//! Prints, per flight end, the trajectory (painted rect, alpha, progress) and then runs the checks that
//! caught real defects in this project:
//!
//! - STALL: the painted rect stops changing while the animation is unfinished. A flight whose rect never
//!   changes at all is exempt (an opacity-only morph moves nothing on purpose); a flight that merely
//!   RETURNS to its starting rect is not exempt, because a hold in the middle of an out-and-back is a stall.
//! - JUMP: the rect moves faster than `--jump-px-per-s`, measured from the record timestamps, so one slow
//!   frame is not a jump.
//! - OPACITY EARLY (leaving end) / OPACITY MISSING (entering end): the leaving end fades out before its
//!   geometry settles, or the entering end never fades in — checked against BOTH its own alpha and the
//!   composited `effective_alpha`, since a scene host can fade an end whose own alpha stays 1.
//! - TARGET MISMATCH: the `end` rect the flight's `resolve` event announced is not where the target settles.
//!   The event is matched by key, flight id AND time (the last announcement before the records), because a
//!   retarget emits a second one.
//! - ENDS DISAGREE: the two ends of one flight settle at different rects (the historical "target resolved to
//!   the source's own size, so the cross-fade never grew" defect).
//! - SETTLE MISMATCH: the two ends settle at different times.
//! - INCOMPLETE: a flight stops before progress 1 without a cancel event for THAT flight (matched by id, not
//!   only by key: one subject is reused across navigations and a retarget cancels the previous flight).
//!
//! Exits 1 when any check fails and 2 when the input cannot be checked at all. Records of kind `node`/`scene`
//! are counted and reported but not checked: they are not flights.
//!
//! Row identity is (subject, flight id, scope): a subject is reused across navigations and flight ids are
//! per-composer, so neither alone identifies a flight.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
struct RowKey {
    subject: String,
    flight: Option<i64>,
    scope: Option<u64>,
    /// Flight ids are per composer, so two composers can produce the same (subject, flight): without this a
    /// multi-composer trace merges them and reports jumps that neither flight made.
    composer: Option<u64>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: anim_trace_report <file.ndjson> [--subject <substr>] [--stall-ms <n>] \
                 [--jump-px-per-s <n>]";
    let path = match args.get(1) {
        Some(p) => p.clone(),
        None => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let mut subject_filter: Option<String> = None;
    let mut stall_ms: f64 = 120.0;
    let mut jump_px_per_s: f64 = 4000.0;
    fn parse_num(args: &[String], i: usize, name: &str) -> f64 {
        let Some(v) = args.get(i + 1).and_then(|v| v.parse::<f64>().ok()) else {
            eprintln!("{name} needs a number");
            std::process::exit(2);
        };
        // NaN / inf / non-positive values used to disable or storm a check silently.
        if !v.is_finite() || v <= 0.0 {
            eprintln!("{name} needs a finite positive number, got {v}");
            std::process::exit(2);
        }
        v
    }
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
                stall_ms = parse_num(&args, i, "--stall-ms");
                i += 2;
            }
            "--jump-px-per-s" => {
                jump_px_per_s = parse_num(&args, i, "--jump-px-per-s");
                i += 2;
            }
            other => {
                eprintln!("unknown option {other}\n{usage}");
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
    // A UTF-8 BOM made the first line unparseable, which silently dropped a `resolve` announcement.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);

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
                skipped += 1;
                continue;
            }
        };
        let kind = v.get("kind").and_then(|s| s.as_str()).unwrap_or("");
        match kind {
            "event" => events.push(v),
            "flight" => {
                let subject = v.get("subject").and_then(|s| s.as_str()).unwrap_or("?").to_string();
                let flight = v.get("flight").and_then(|f| f.as_i64());
                let scope = v.get("scope").and_then(|f| f.as_u64());
                let composer = v.get("composer").and_then(|f| f.as_u64());
                rows.entry(RowKey { subject, flight, scope, composer }).or_default().push(v);
            }
            other => {
                *other_kinds.entry(other.to_string()).or_default() += 1;
            }
        }
    }
    if lines == 0 {
        eprintln!("{path} has no records (empty file?)");
        std::process::exit(2);
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
    let key_of =
        |subject: &str| subject.split_once('#').map(|(b, _)| b).unwrap_or(subject).to_string();
    let t_first = |recs: &[Value]| recs.iter().find_map(|r| num(r, &["t_ms"]));
    let t_last = |recs: &[Value]| recs.iter().rev().find_map(|r| num(r, &["t_ms"]));

    // Events indexed once by subject: the per-row scan used to be O(rows x events) and dominated the run
    // (measured: 104 s for a 52 MB trace, against 2.8 s to parse it).
    let mut events_by_subject: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in events.iter().enumerate() {
        if let Some(s) = e.get("subject").and_then(|s| s.as_str()) {
            events_by_subject.entry(s.to_string()).or_default().push(i);
        }
    }

    let t0 = rows
        .values()
        .filter_map(|v| v.first())
        .filter_map(|r| r.get("t_ms").and_then(|t| t.as_f64()))
        .fold(f64::INFINITY, f64::min);

    let mut failures: Vec<String> = Vec::new();
    let mut matched_any = false;
    let mut settled_groups = 0usize;
    let mut total_groups = 0usize;
    // (subject, flight, scope) -> last painted rect, for the cross-end checks.
    let mut last_rects: BTreeMap<RowKey, (f64, f64, f64, f64)> = BTreeMap::new();

    for (row, recs) in &rows {
        let subject = row.subject.clone();
        if let Some(f) = &subject_filter {
            if !subject.contains(f.as_str()) {
                continue;
            }
        }
        matched_any = true;
        total_groups += 1;
        println!(
            "=== {subject} (flight {:?}, scope {:?}, {} records) ===",
            row.flight,
            row.scope,
            recs.len()
        );
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
        // A cancelled flight is expected to stop mid-way: a retarget cancels the previous flight of the same
        // key, so "did not reach the announced end" and "the two ends did not converge" are normal for it
        // (measured false positive on a real retarget trace: the push flight ended at 199x153 and was
        // reported as a TARGET MISMATCH). Matched by flight id as well as key.
        let cancelled = events_by_subject.get(&base_key).is_some_and(|idx| {
            idx.iter().filter_map(|i| events.get(*i)).any(|e| {
                if e.get("phase").and_then(|p| p.as_str()) != Some("cancel") {
                    return false;
                }
                let detail = e.get("detail").and_then(|d| d.as_str()).unwrap_or("");
                match row.flight {
                    Some(f) => {
                        detail.contains(&format!("id={f} ")) || detail.ends_with(&format!("id={f}"))
                    }
                    None => true,
                }
            })
        });
        let first_rect = recs.iter().find_map(rect);
        let last_rect = recs.iter().rev().find_map(rect);
        if let Some(rc) = last_rect {
            last_rects.insert(row.clone(), rc);
        }

        // 1. stall.
        let steady_rect = match (first_rect, last_rect) {
            (Some(a), Some(b)) => {
                (a.0 - b.0).abs() < 0.5
                    && (a.1 - b.1).abs() < 0.5
                    && (a.2 - b.2).abs() < 0.5
                    && (a.3 - b.3).abs() < 0.5
            }
            _ => false,
        };
        // Exempt only a flight whose EVERY rect equals the first: a flight that returns to its start can
        // still hold in the middle (measured: an out-and-back frozen for 2.1 s was exempted before).
        let all_rects_equal = first_rect.is_some_and(|first| {
            recs.iter().filter_map(rect).all(|r| {
                (r.0 - first.0).abs() < 0.5
                    && (r.1 - first.1).abs() < 0.5
                    && (r.2 - first.2).abs() < 0.5
                    && (r.3 - first.3).abs() < 0.5
            })
        });
        if !steady_rect || !all_rects_equal {
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
                                "STALL: {subject} painted rect frozen at ({:.0},{:.0}) {:.0}x{:.0} for {:.0}ms (p={:.2})",
                                rc.0, rc.1, rc.2, rc.3, t - t_start, p
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

        // 2. jumps, per second.
        for w in recs.windows(2) {
            let (Some(a), Some(b)) = (rect(&w[0]), rect(&w[1])) else {
                continue;
            };
            let (Some(ta), Some(tb)) = (num(&w[0], &["t_ms"]), num(&w[1], &["t_ms"])) else {
                continue;
            };
            let dt = (tb - ta) / 1000.0;
            if dt <= 0.0 || dt > 0.5 {
                continue;
            }
            let d = (a.0 - b.0)
                .abs()
                .max((a.1 - b.1).abs())
                .max((a.2 - b.2).abs())
                .max((a.3 - b.3).abs());
            let speed = d / dt;
            if speed > jump_px_per_s && d >= 0.5 {
                let msg = format!(
                    "JUMP: {subject} moved {d:.1}px in {:.0}ms ({speed:.0}px/s) \
                     ({:.0},{:.0}) {:.0}x{:.0} -> ({:.0},{:.0}) {:.0}x{:.0}",
                    dt * 1000.0,
                    a.0,
                    a.1,
                    a.2,
                    a.3,
                    b.0,
                    b.1,
                    b.2,
                    b.3
                );
                if !failures.contains(&msg) {
                    println!("   !! {msg}");
                    failures.push(msg);
                }
            }
        }

        // 3. opacity: this end must spend its opacity AFTER the geometry settles. Both alpha fields are
        //    considered: a scene host fades an end whose own alpha stays 1, so reading only `alpha` reported
        //    a healthy scene-driven fade as "never fades in".
        let visible_alpha = |r: &Value| -> f64 {
            let a = num(r, &["alpha"]).unwrap_or(0.0);
            let e = num(r, &["effective_alpha"]).unwrap_or(a);
            a.max(e)
        };
        let is_target = matches!(role_of(&subject).as_deref(), Some("Target") | Some("Morph"));
        if last_p > 0.0 && is_target {
            let max_alpha = recs.iter().map(visible_alpha).fold(0.0, f64::max);
            if max_alpha <= 0.01 {
                let msg = format!(
                    "OPACITY MISSING: {subject} never becomes visible (max alpha {max_alpha:.3}) while its \
                     geometry moves to p={last_p:.2}"
                );
                println!("   !! {msg}");
                failures.push(msg);
            }
        } else if last_p > 0.0 {
            let zero_at = recs
                .iter()
                .skip(1)
                .find(|r| visible_alpha(r) <= 0.01)
                .and_then(|r| num(r, &["t_ms"]));
            let done_at = recs
                .iter()
                .find(|r| num(r, &["progress"]).unwrap_or(0.0) >= 0.99)
                .and_then(|r| num(r, &["t_ms"]));
            if let (Some(z), Some(d)) = (zero_at, done_at) {
                if z < d - 50.0 {
                    let msg = format!(
                        "OPACITY EARLY: {subject} became invisible {:.0}ms before the geometry settled",
                        d - z
                    );
                    println!("   !! {msg}");
                    failures.push(msg);
                }
            }
        }

        // 4. the announced end rect must be where the target settles. Matched by key, id and TIME (the last
        //    announcement at or before this row's records): taking the first `resolve` in the file reported a
        //    stale announcement after a retarget.
        if role_of(&subject).as_deref() == Some("Target") && !cancelled {
            let announced = events_by_subject.get(&base_key).and_then(|idx| {
                // Against the row's LAST record: a retarget emits a second announcement mid-flight, and
                // filtering by the row's first timestamp kept the stale one (measured false positive on the
                // reviewer's `f_retarget` fixture).
                let row_end = t_last(recs).unwrap_or(f64::INFINITY);
                idx.iter()
                    .filter_map(|i| events.get(*i))
                    .filter_map(|e| {
                        let detail = e.get("detail").and_then(|d| d.as_str())?;
                        let id_matches = match row.flight {
                            Some(f) => detail.contains(&format!("id={f} ")) || detail.ends_with(&format!("id={f}")),
                            None => true,
                        };
                        let t = e.get("t_ms").and_then(|t| t.as_f64())?;
                        if !id_matches || !detail.contains("end=(") || t > row_end + 50.0 {
                            return None;
                        }
                        let pair = detail.split("end=(").nth(1)?.split(')').next()?;
                        let nums: Vec<f64> =
                            pair.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                        (nums.len() == 2).then_some((t, nums[0], nums[1]))
                    })
                    .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(_, w, h)| (w, h))
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

        // 5. incomplete.
        if !settled && !cancelled {
            let (Some(first_t), Some(last_t)) = (t_first(recs), t_last(recs)) else {
                continue;
            };
            // A row that ends at the file's end is a truncated run; a short row in the middle of a file is
            // the same thing seen from a group that was cancelled without an event.
            let msg = format!(
                "INCOMPLETE: {subject} (flight {:?}) stops at p={last_p:.3} after {:.0}ms with no cancel event",
                row.flight,
                last_t - first_t
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

    // 6. the two ends of one flight must agree: same final rect (a flight that never grew was invisible to
    //    every other check) and the same settle time.
    let mut by_flight: BTreeMap<(String, Option<i64>, Option<u64>, Option<u64>), Vec<(String, f64, Option<(f64, f64, f64, f64)>)>> =
        BTreeMap::new();
    for (row, recs) in &rows {
        let base = key_of(&row.subject);
        let Some(role) = role_of(&row.subject) else { continue };
        if role != "Source" && role != "Target" {
            continue;
        }
        let settle = recs
            .iter()
            .find(|r| num(r, &["progress"]).unwrap_or(0.0) >= 0.99)
            .and_then(|r| num(r, &["t_ms"]));
        by_flight
            .entry((base, row.flight, row.scope, row.composer))
            .or_default()
            .push((row.subject.clone(), settle.unwrap_or(f64::NAN), last_rects.get(row).copied()));
    }
    for ((base, flight, _, _), ends) in by_flight {
        if ends.len() != 2 {
            continue;
        }
        // A cancelled end is expected to stop mid-way (see the per-row `cancelled` above).
        let end_cancelled = |subj: &str| {
            events_by_subject.get(subj).is_some_and(|idx| {
                idx.iter().filter_map(|i| events.get(*i)).any(|e| {
                    e.get("phase").and_then(|p| p.as_str()) == Some("cancel")
                        && match flight {
                            Some(f) => {
                                let d = e.get("detail").and_then(|d| d.as_str()).unwrap_or("");
                                d.contains(&format!("id={f} ")) || d.ends_with(&format!("id={f}"))
                            }
                            None => true,
                        }
                })
            })
        };
        if end_cancelled(&base) {
            continue;
        }
        let (a, b) = (&ends[0], &ends[1]);
        if let (Some(ra), Some(rb)) = (a.2, b.2) {
            let d = (ra.0 - rb.0)
                .abs()
                .max((ra.1 - rb.1).abs())
                .max((ra.2 - rb.2).abs())
                .max((ra.3 - rb.3).abs());
            if d > 2.0 {
                let msg = format!(
                    "ENDS DISAGREE: {base} (flight {flight:?}) settles at {:.0}x{:.0} vs {:.0}x{:.0} \
                     ({} vs {})",
                    ra.2, ra.3, rb.2, rb.3, a.0, b.0
                );
                println!("!! {msg}");
                failures.push(msg);
            }
        }
        if a.1.is_finite() && b.1.is_finite() {
            let d = (a.1 - b.1).abs();
            if d > 200.0 {
                let msg = format!(
                    "SETTLE MISMATCH: {base} (flight {flight:?}) ends settle {d:.0}ms apart ({} vs {})",
                    a.0, b.0
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
             jump>{jump_px_per_s:.0}px/s, opacity/geometry, target, ends, settle)"
        );
    } else {
        println!("{} anomalies:", failures.len());
        for f in &failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
