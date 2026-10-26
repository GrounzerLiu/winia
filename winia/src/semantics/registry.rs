use super::*;

// ═══════════════════════════════════════════════════════════
// Published snapshot
// ═══════════════════════════════════════════════════════════

/// One overlay's elements, and the overlay's screen origin: its own tree is in the overlay's local
/// coordinates (it is rendered translated), so a consumer that wants screen coordinates adds it.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySemantics {
    pub id: u64,
    pub origin: (f32, f32),
    pub nodes: Vec<SemanticsNode>,
}

/// Everything one window declared in a frame: the main tree and every overlay above it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowSemantics {
    pub main: Vec<SemanticsNode>,
    pub overlays: Vec<OverlaySemantics>,
    /// What a platform bridge needs to turn logical bounds into physical ones: the window's client
    /// area in logical pixels and the scale it draws at. Where the window IS on screen is not here —
    /// that is the platform's to ask (the UIA bridge reads it off the window handle).
    pub window_size: (f32, f32),
    pub scale_factor: f32,
}

impl WindowSemantics {
    /// The window's elements as JSON — the debug channel's `sem` answer.
    pub fn json(&self) -> String {
        let overlays: Vec<String> = self
            .overlays
            .iter()
            .map(|overlay| {
                format!(
                    "{{\"id\":{},\"screen\":[{:.0},{:.0}],\"tree\":{}}}",
                    overlay.id,
                    overlay.origin.0,
                    overlay.origin.1,
                    semantics_json(&overlay.nodes)
                )
            })
            .collect();
        format!("{{\"main\":{},\"overlays\":[{}]}}", semantics_json(&self.main), overlays.join(","))
    }

    /// The window's top-level elements: the main tree's, then each overlay's — with the origin each
    /// one's coordinates are relative to (an overlay's tree is in the overlay's own space).
    pub fn top_level(&self) -> Vec<(&SemanticsNode, (f32, f32))> {
        let mut out: Vec<(&SemanticsNode, (f32, f32))> =
            self.main.iter().map(|node| (node, (0.0, 0.0))).collect();
        for overlay in &self.overlays {
            out.extend(overlay.nodes.iter().map(|node| (node, overlay.origin)));
        }
        out
    }

    /// The element a path addresses, and the origin its coordinates are relative to.
    ///
    /// A path is indices into the tree: `[2]` is the third top-level element, `[2, 0]` its first
    /// child. This is the addressing a UIA provider navigates with, and it is the same shape for the
    /// main tree and for overlays, so nothing has to special-case a dialog.
    pub fn resolve(&self, path: &[usize]) -> Option<(&SemanticsNode, (f32, f32))> {
        let (&first, rest) = path.split_first()?;
        let (mut node, origin) = *self.top_level().get(first)?;
        for &index in rest {
            node = node.children.get(index)?;
        }
        Some((node, origin))
    }

    /// The children of a path, in tree order (the top-level elements for the empty path).
    pub fn children_at(&self, path: &[usize]) -> Vec<(&SemanticsNode, (f32, f32))> {
        match self.resolve(path) {
            Some((node, origin)) => node.children.iter().map(|child| (child, origin)).collect(),
            // The empty path is the window itself, whose children are the top-level elements.
            None if path.is_empty() => self.top_level(),
            None => Vec::new(),
        }
    }

    /// A fingerprint of the tree's SHAPE: which elements exist, in what order, with how many
    /// children each has. Names and states are deliberately not in it — a label changing is a
    /// property change, not a structural one, and a client that cares re-reads the property.
    ///
    /// A platform bridge compares this frame's fingerprint with the last one to decide whether to tell
    /// its clients the tree changed, so it has to be cheap (one pass, no allocation) and stable
    /// (the same tree always hashes the same).
    pub fn structure_fingerprint(&self) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        let mut mix = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x100000001b3);
        };
        // The overlay ids are part of the shape: a dialog appearing IS a structural change.
        for overlay in &self.overlays {
            mix(overlay.id);
        }
        for (node, _) in self.flatten() {
            mix(node.node_id);
            mix(node.children.len() as u64);
        }
        hash
    }

    /// The path of the element that currently has the keyboard focus, if any.
    ///
    /// This is what a focus-changed notification needs: the path addresses the element, and the bridge
    /// turns it into a provider. Depth-first, so the innermost focused element wins — an element whose
    /// child is also focused is not the focus.
    pub fn focused_path(&self) -> Option<Vec<usize>> {
        let mut found: Option<Vec<usize>> = None;
        let mut visit = |node: &SemanticsNode, _origin: (f32, f32), path: Vec<usize>| {
            if node.focused {
                found = Some(path);
            }
        };
        for (index, (node, origin)) in self.top_level().into_iter().enumerate() {
            walk_with_paths(node, vec![index], origin, &mut visit);
        }
        found
    }

    /// Every element of the window, parents before children, main tree first then each overlay — what
    /// a query that is not addressed by path needs ("the focused element", "the element under this
    /// point").
    pub fn flatten(&self) -> Vec<(&SemanticsNode, (f32, f32))> {
        let mut out = Vec::new();
        for (node, origin) in self.top_level() {
            node.walk(&mut |n| out.push((n, origin)));
        }
        out
    }
}

// ═══════════════════════════════════════════════════════════
// Actions requested from outside the UI thread
// ═══════════════════════════════════════════════════════════

/// Something another thread (a platform bridge) asked the UI to do on a node.
///
/// UIA calls a provider at a moment the application does not control, so the provider only queues
/// this; the frame loop drains it and runs the same code a real interaction would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    /// Fire the node's click action.
    Invoke(u64),
    /// Move the keyboard focus to the node.
    Focus(u64),
    /// Compose's `ScrollToIndex`: bring item `index` into view in a collection.
    ///
    /// `node` names the element that declared the action (a platform bridge knows which element it
    /// is answering for); `None` means "the window's first collection that offers it", which is what
    /// the debug channel uses — a test has no node id to hand.
    ScrollToIndex { node: Option<u64>, index: usize },
}

pub(crate) static ACTIONS: std::sync::Mutex<Option<Vec<(u64, UiAction)>>> = std::sync::Mutex::new(None);

/// Queue an action for a window's next frame.
pub fn request_action(window_id: u64, action: UiAction) {
    let mut store = ACTIONS.lock().unwrap();
    store.get_or_insert_with(Default::default).push((window_id, action));
}

/// Whether anything is queued — the event loop checks this before walking its windows.
pub fn has_actions() -> bool {
    ACTIONS
        .lock()
        .map(|store| store.as_ref().is_some_and(|store| !store.is_empty()))
        .unwrap_or(false)
}

/// Take a window's queued actions.
pub fn take_actions(window_id: u64) -> Vec<UiAction> {
    let Ok(mut store) = ACTIONS.lock() else {
        return Vec::new();
    };
    let Some(store) = store.as_mut() else {
        return Vec::new();
    };
    let mut taken = Vec::new();
    store.retain(|(id, action)| {
        if *id == window_id {
            taken.push(*action);
            false
        } else {
            true
        }
    });
    taken
}

pub(crate) static PUBLISHED: std::sync::Mutex<Option<std::collections::HashMap<u64, std::sync::Arc<WindowSemantics>>>> =
    std::sync::Mutex::new(None);

/// Whether the frame loop should build snapshots at all. Both consumers are optional, and the walk is
/// pure overhead without one — so it compiles away entirely in a plain build.
pub const fn publishing_enabled() -> bool {
    cfg!(feature = "debug-server") || cfg!(feature = "accessibility")
}

/// Replace one window's snapshot — called once per rendered frame, so a query answers with what the
/// frame that just drew declared rather than what a later frame will.
pub fn publish(window_id: u64, snapshot: WindowSemantics) {
    let mut store = PUBLISHED.lock().unwrap();
    store
        .get_or_insert_with(Default::default)
        .insert(window_id, std::sync::Arc::new(snapshot));
}

/// The last published snapshot of one window.
pub fn published(window_id: u64) -> Option<std::sync::Arc<WindowSemantics>> {
    PUBLISHED.lock().ok()?.as_ref()?.get(&window_id).cloned()
}

/// Drop a window's snapshot (its window closed — a stale tree would answer for a window that is gone).
pub fn forget(window_id: u64) {
    if let Ok(mut store) = PUBLISHED.lock() {
        if let Some(store) = store.as_mut() {
            store.remove(&window_id);
        }
    }
}

