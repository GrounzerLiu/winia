//! Verifiable-animation trace: per-frame geometry and opacity of everything that animates.
//!
//! Why this exists: judging an animation from screenshots is unreliable — this project spent several
//! rounds measuring painted pixels at ~100 ms per sample, inserting temporary probes that had to be
//! reverted, and still mis-explaining what the eye reported. A structured, per-frame record of
//! `layout` vs `painted` geometry plus opacity answers those questions directly and can be asserted on
//! in a headless test.
//!
//! Design rules:
//! - **No app code.** The framework emits records on its own; identity (flight scope/key, role,
//!   node slot key, scene id) comes from the framework's own bookkeeping. The only knob is the
//!   `WINIA_ANIM_TRACE` environment variable, which names the output file.
//! - **Zero cost when off.** Without the `anim-trace` feature every function here is a no-op that
//!   compiles away; with the feature on but no env var set, nothing is written.
//! - **Opacity in three parts** so "who faded it" is answerable: the end's own alpha, the effective
//!   alpha used for the draw (own alpha composed with the scene layer's visibility), and the scene's
//!   visibility itself.
//!
//! Output: one JSON object per line (NDJSON), one line per subject per frame.

use crate::ui::shared_transition::NavSceneInfo;
// Only the feature-gated implementation keeps maps (the no-op variant has nothing to look up).
#[cfg(feature = "anim-trace")]
use std::collections::HashMap;

/// A rectangle in logical pixels, in the canvas frame of the composer that drew it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl TraceRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    fn to_json(self) -> String {
        // Non-finite components would produce `NaN`/`inf`, which JSON cannot express and which would make
        // the whole line unparseable; null keeps the line valid and visible in a report.
        fn f(v: f32) -> String {
            if v.is_finite() {
                format!("{v:.2}")
            } else {
                "null".to_string()
            }
        }
        format!(
            "{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
            f(self.x),
            f(self.y),
            f(self.w),
            f(self.h)
        )
    }
}

/// What kind of thing a record describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    /// One end of a shared-element flight.
    Flight,
    /// A scene published by a scene host (winia's nav layers).
    Scene,
    /// A plain layout node whose geometry is being watched.
    Node,
    /// A lifecycle event: a flight started, was cancelled, finished, or a morph was opened. Geometry
    /// records say WHAT moved; these say WHY it stopped, which is otherwise invisible in a trace.
    Event,
}

impl TraceKind {
    fn as_str(self) -> &'static str {
        match self {
            TraceKind::Flight => "flight",
            TraceKind::Scene => "scene",
            TraceKind::Node => "node",
            TraceKind::Event => "event",
        }
    }
}

/// One frame's worth of information about one animated subject.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceRecord {
    pub kind: TraceKind,
    /// Framework-derived identity, stable across frames, e.g. `flight:hero#Source`, `scene:8f2a`.
    pub subject: String,
    pub scope: Option<u64>,
    pub key: Option<String>,
    /// `Source` / `Target` / `Morph`.
    pub role: Option<&'static str>,
    pub flight: Option<u64>,
    pub scene: Option<u64>,
    /// `flying` / `settled` / `entering` / `leaving` …
    pub phase: Option<&'static str>,
    /// Animation progress 0..=1 (springs may overshoot).
    pub progress: Option<f32>,
    /// The node's layout rect (what the layout engine measured and placed).
    pub layout: Option<TraceRect>,
    /// The rect actually painted, after flight scale and clip — this is what the eye sees.
    pub painted: Option<TraceRect>,
    /// The end's own opacity (the flight's / transition's value).
    pub alpha: Option<f32>,
    /// The opacity actually used for the draw: `alpha` composed with the scene layer's visibility when
    /// the subject is painted inside that layer.
    pub effective_alpha: Option<f32>,
    /// Visibility published by the scene host this subject belongs to, if any.
    pub scene_visibility: Option<f32>,
    /// Normalized corner radii [TL, TR, BR, BL].
    pub radii: Option<[f32; 4]>,
    /// Whether the draw was clipped to the bounds.
    pub clip: Option<bool>,
    /// Free-form explanation for [`TraceKind::Event`] records (e.g. a cancel reason).
    pub detail: Option<String>,
}

impl TraceRecord {
    pub fn flight(subject: impl Into<String>) -> Self {
        Self {
            kind: TraceKind::Flight,
            subject: subject.into(),
            scope: None,
            key: None,
            role: None,
            flight: None,
            scene: None,
            phase: None,
            progress: None,
            layout: None,
            painted: None,
            alpha: None,
            effective_alpha: None,
            scene_visibility: None,
            radii: None,
            clip: None,
            detail: None,
        }
    }

    /// A lifecycle event: `name` says what happened (e.g. `cancel`), `detail` why.
    pub fn event(subject: impl Into<String>, name: &'static str, detail: impl Into<String>) -> Self {
        let mut r = Self::flight(subject);
        r.kind = TraceKind::Event;
        r.phase = Some(name);
        r.detail = Some(detail.into());
        r
    }

    pub fn scene(id: u64) -> Self {
        let mut r = Self::flight(format!("scene:{id:#x}"));
        r.kind = TraceKind::Scene;
        r.scene = Some(id);
        r
    }

    fn to_json(&self, frame: u64, t_ms: u128) -> String {
        fn opt_f32(v: Option<f32>) -> String {
            // JSON has no NaN/Infinity: a non-finite value would make the line unparseable, and a report
            // silently drops lines it cannot parse, so write null instead.
            match v {
                Some(v) if v.is_finite() => format!("{v:.4}"),
                _ => "null".to_string(),
            }
        }
        fn opt_rect(v: Option<TraceRect>) -> String {
            v.map(|v| v.to_json()).unwrap_or_else(|| "null".to_string())
        }
        let radii = self
            .radii
            .map(|r| {
                format!(
                    "[{:.2},{:.2},{:.2},{:.2}]",
                    r[0], r[1], r[2], r[3]
                )
            })
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"frame\":{frame},\"t_ms\":{t_ms},\"kind\":\"{}\",\"subject\":{},\
             \"scope\":{},\"key\":{},\"role\":{},\"flight\":{},\"scene\":{},\"phase\":{},\
             \"progress\":{},\"layout\":{},\"painted\":{},\"alpha\":{},\"effective_alpha\":{},\
             \"scene_visibility\":{},\"radii\":{radii},\"clip\":{},\"detail\":{}}}",
            self.kind.as_str(),
            json_str(&self.subject),
            self.scope.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string()),
            self.key
                .as_ref()
                .map(|k| json_str(k))
                .unwrap_or_else(|| "null".to_string()),
            self.role
                .map(|r| format!("{r:?}"))
                .unwrap_or_else(|| "null".to_string()),
            self.flight.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string()),
            self.scene.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string()),
            self.phase
                .map(|p| format!("{p:?}"))
                .unwrap_or_else(|| "null".to_string()),
            opt_f32(self.progress),
            opt_rect(self.layout),
            opt_rect(self.painted),
            opt_f32(self.alpha),
            opt_f32(self.effective_alpha),
            opt_f32(self.scene_visibility),
            self.clip
                .map(|c| c.to_string())
                .unwrap_or_else(|| "null".to_string()),
            self.detail
                .as_ref()
                .map(|d| json_str(d))
                .unwrap_or_else(|| "null".to_string()),
        )
    }
}

/// A JSON string literal valid for EVERY input.
///
/// `{:?}` (Rust's `escape_debug`) is not a JSON encoder: it writes `\0` for NUL and `\u{...}` for any
/// control, whitespace-like, grapheme-extender, format-control or private-use codepoint (U+00A0, U+3000,
/// ZWJ, VS16, PUA …), none of which JSON accepts. `serde_json::from_str` rejects such lines, and a report
/// drops what it cannot parse, so the loss was silent. JSON needs only `"`, `\` and the C0 controls
/// escaped; every other codepoint passes through as UTF-8, which JSON allows.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── Real implementation (feature on) ─────────────────────────────────────────────

#[cfg(feature = "anim-trace")]
mod imp {
    use super::*;
    use std::io::Write;

    enum Sink {
        Off,
        File(std::io::BufWriter<std::fs::File>),
        Memory(Vec<(u64, u128, TraceRecord)>),
    }

    thread_local! {
        static SINK: std::cell::RefCell<Sink> = std::cell::RefCell::new(init());
        /// Scene handles published by scene hosts, with the last frame each was noted on. Kept ACROSS
        /// frames on purpose: a scene host only reaches `with_nav_scene` when it composes, which during
        /// an animation is not every frame, so clearing this per frame recorded a live scene once and
        /// then lost it (measured: a scene that was on screen for the whole transition had exactly one
        /// record). Stale entries are dropped instead, after a short grace period.
        static SCENES: std::cell::RefCell<HashMap<u64, (NavSceneInfo, u64)>> =
            std::cell::RefCell::new(HashMap::new());
        static FRAME: std::cell::Cell<(u64, u128)> = const { std::cell::Cell::new((0, 0)) };
    }

    /// Rolling window of the most recent records, so the debug server can serve a live trace without a
    /// file being configured: `tr [n]` answers with the last n records.
    ///
    /// Deliberately NOT thread-local like the rest of this module: records are produced on the UI
    /// thread while the debug server's WebSocket task runs on a tokio thread, and a thread-local ring
    /// answered `tr` with zero lines (measured). The lock is uncontended in practice — one push per
    /// record per frame.
    static RING: std::sync::LazyLock<std::sync::Mutex<std::collections::VecDeque<(u64, u128, TraceRecord)>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::VecDeque::new()));

    /// Records kept for the live channel (a few seconds of a busy transition).
    const RING_CAP: usize = 4000;

    /// Frames a scene stays traced after the host last published it.
    const SCENE_GRACE_FRAMES: u64 = 30;

    fn init() -> Sink {
        match std::env::var("WINIA_ANIM_TRACE") {
            Ok(path) if !path.is_empty() => match std::fs::File::create(&path) {
                Ok(f) => Sink::File(std::io::BufWriter::new(f)),
                Err(e) => {
                    eprintln!("[anim-trace] cannot open {path}: {e}");
                    Sink::Off
                }
            },
            _ => Sink::Off,
        }
    }

    /// Is tracing active? True whenever the feature is compiled in: the ring that serves the debug
    /// server's live `tr` command is always fed, and a FILE is only written when `WINIA_ANIM_TRACE`
    /// names one. Emission sites gate on this, so with the feature on but no env var the framework
    /// still records into memory (a few thousand small records, bounded by the ring) and nothing is
    /// written to disk; with the feature off every call here compiles to a no-op.
    pub fn enabled() -> bool {
        true
    }

    /// Remember a scene host's handle (called by `with_nav_scene`), stamped with the current frame.
    pub fn note_scene(scene: NavSceneInfo) {
        if !enabled() {
            return;
        }
        let frame = FRAME.with(|f| f.get().0);
        SCENES.with(|s| {
            s.borrow_mut().insert(scene.id, (scene, frame));
        });
    }

    /// Start a frame: remembers `frame`/`t_ms`, records every scene still inside its grace window, and
    /// flushes the previous frame's lines. Called once per app-loop iteration.
    pub fn begin_frame(frame: u64, t_ms: u128) {
        if !enabled() {
            return;
        }
        FRAME.with(|f| f.set((frame, t_ms)));
        let live: Vec<NavSceneInfo> = SCENES.with(|s| {
            let mut m = s.borrow_mut();
            m.retain(|_, (_, last)| frame.saturating_sub(*last) <= SCENE_GRACE_FRAMES);
            m.values().map(|(sc, _)| sc.clone()).collect()
        });
        for sc in live {
            let mut r = TraceRecord::scene(sc.id);
            let vis = (sc.visibility)();
            r.scene_visibility = Some(vis);
            r.effective_alpha = Some(vis);
            record(r);
        }
        flush();
    }

    /// Append one record (written at the next [`begin_frame`] or [`flush`]; also kept in the ring the
    /// live channel reads).
    pub fn record(r: TraceRecord) {
        let (frame, t_ms) = FRAME.with(|f| f.get());
        if let Ok(mut ring) = RING.lock() {
            ring.push_back((frame, t_ms, r.clone()));
            while ring.len() > RING_CAP {
                ring.pop_front();
            }
        }
        SINK.with(|s| match &mut *s.borrow_mut() {
            Sink::Off => {}
            Sink::File(w) => {
                let _ = writeln!(w, "{}", r.to_json(frame, t_ms));
            }
            Sink::Memory(v) => v.push((frame, t_ms, r)),
        });
    }

    /// The most recent `n` records as NDJSON lines, oldest first — what the debug server's `tr`
    /// command returns. Works whether or not a file sink is configured, and from any thread.
    pub fn recent_lines(n: usize) -> Vec<String> {
        let Ok(ring) = RING.lock() else { return Vec::new() };
        ring.iter()
            .skip(ring.len().saturating_sub(n))
            .map(|(frame, t_ms, r)| r.to_json(*frame, *t_ms))
            .collect()
    }

    /// Flush buffered lines (called at frame end; safe to call any time).
    pub fn flush() {
        SINK.with(|s| {
            if let Sink::File(w) = &mut *s.borrow_mut() {
                let _ = w.flush();
            }
        });
    }

    /// Tests: collect into memory instead of a file (and disable any file sink).
    pub fn capture_start() {
        SINK.with(|s| *s.borrow_mut() = Sink::Memory(Vec::new()));
    }

    /// Tests: take everything captured since [`capture_start`].
    pub fn capture_take() -> Vec<(u64, u128, TraceRecord)> {
        SINK.with(|s| match &mut *s.borrow_mut() {
            Sink::Memory(v) => std::mem::take(v),
            _ => Vec::new(),
        })
    }

    /// Tests: stop capturing.
    pub fn capture_stop() {
        SINK.with(|s| *s.borrow_mut() = Sink::Off);
    }
}

// ── No-op implementation (feature off) ──────────────────────────────────────────

#[cfg(not(feature = "anim-trace"))]
mod imp {
    use super::*;

    pub fn enabled() -> bool {
        false
    }
    pub fn note_scene(_scene: NavSceneInfo) {}
    pub fn begin_frame(_frame: u64, _t_ms: u128) {}
    pub fn record(_r: TraceRecord) {}
    /// Nothing is recorded with the feature off, so the live channel has nothing to serve.
    pub fn recent_lines(_n: usize) -> Vec<String> {
        Vec::new()
    }
    pub fn flush() {}
    pub fn capture_start() {}
    pub fn capture_take() -> Vec<(u64, u128, TraceRecord)> {
        Vec::new()
    }
    pub fn capture_stop() {}
}

pub use imp::*;
