//! `SearchBar` / `DockedSearchBar` — search input with expandable results
//! (cf. Compose Material3 `SearchBar`, classic active/inactive semantics).
//!
//! Reference: D:/any/androidx-ref/.../material3/SearchBar.kt (sparse checkout).
//! The upstream new split API (`SearchBarState` + `ExpandedFullScreenSearchBar` /
//! `ExpandedDockedSearchBar`) is collapsed here into the classic shape:
//! `SearchBarState { query, active }` + `SearchBar` (fullscreen on active) +
//! `DockedSearchBar` (bounded dropdown on active).
//!
//! State machine (classic semantics):
//! - inactive: pill Surface + read-only InputField; tap/focus activates.
//! - active (SearchBar): full-screen Surface (Rectangle) + editable InputField +
//!   Divider + content. No scrim (opaque container covers content, upstream same).
//! - active (DockedSearchBar): Popup anchored under the bar (BottomLeft + gap) with
//!   dockedShape; outside tap dismisses (Popup dismiss_on_outside).
//! - Enter in single-line field → `on_search(query)` (IME Search equivalent);
//!   callers conventionally deactivate inside on_search. Esc → deactivate.
//! - No system Back on desktop: Esc substitutes (documented divergence).

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::text_field::{TextField, TextFieldColors, TextFieldValue};
use std::sync::Arc;

/// Search bar state: text query + active flag (cf. classic `query`/`active` params).
#[derive(Clone)]
pub struct SearchBarState {
    /// Current query text (controlled — callers read `query.get().text`).
    pub query: State<TextFieldValue>,
    /// Active (expanded) flag.
    pub active: State<bool>,
}

impl SearchBarState {
    pub fn new() -> Self {
        Self {
            query: State::new(TextFieldValue::new("")),
            active: State::new(false),
        }
    }

    pub fn query_text(&self) -> String {
        self.query.get().text.clone()
    }

    pub fn set_query(&self, text: impl Into<String>) {
        let text = text.into();
        self.query.update(|v| {
            v.text = text.clone();
            let len = v.text.len();
            v.selection = len..len;
        });
    }

    pub fn open(&self) {
        self.active.set(true);
    }

    pub fn close(&self) {
        self.active.set(false);
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }
}

impl Default for SearchBarState {
    fn default() -> Self {
        Self::new()
    }
}

/// Search bar colors (cf. `SearchBarColors`: only container + divider; input field
/// colors travel separately via `TextFieldColors`).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchBarColors {
    pub container: Color,
    pub divider: Color,
}

/// Search bar defaults (cf. `SearchBarDefaults`).
pub struct SearchBarDefaults;

impl SearchBarDefaults {
    /// Collapsed bar / input field shape: full pill.
    pub fn input_field_shape() -> Shape {
        Shape::Pill
    }

    /// Expanded fullscreen container shape: rectangle.
    pub fn full_screen_shape() -> Shape {
        Shape::Rectangle
    }

    /// Docked bar + dropdown shape: rounded 12 (upstream `dockedDropdownShape`).
    pub fn docked_shape() -> Shape {
        Shape::rounded(12.0)
    }

    /// Gap between docked bar and dropdown (upstream `dockedDropdownGapSize` = 2dp).
    pub fn docked_gap() -> f32 {
        2.0
    }

    /// Default colors from theme (container = surface-container-high-ish, divider = outline).
    pub fn colors(theme: &crate::ui::theme::ThemeColors) -> SearchBarColors {
        SearchBarColors {
            container: theme.surface_container_high,
            divider: theme.outline_variant,
        }
    }
}

/// Material Icons `search` path (24x24 viewport, Apache 2.0). Used as the default
/// leading icon so SearchBar needs no font feature; callers override via leading_icon().
pub const SEARCH_ICON_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";

/// Material Icons `arrow_back` path (24x24 viewport, Apache 2.0). Default expanded
/// leading icon (cf. fullscreen SearchBar navigation icon).
pub const BACK_ICON_PATH: &str = "M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z";

/// Collapsed bar height (Compose `SearchBarTokens.ContainerHeight` = 56dp).
pub const SEARCH_BAR_HEIGHT: f32 = 56.0;
/// Shared input field: TextField without container visuals, single line, with
/// leading/trailing/placeholder slots and Enter → on_search. Fills parent width
/// (the bar shape comes from the surrounding Surface). When `fill_height` the field
/// stretches to the bar height (collapsed pill: content vertically centered);
/// otherwise it wraps content (expanded: natural input height).
#[allow(clippy::too_many_arguments)]
fn input_field(
    ctx: &mut ComposeCtx,
    query: &State<TextFieldValue>,
    on_query_change: &Option<Arc<dyn Fn(String) + Send + Sync>>,
    on_search: &Option<Arc<dyn Fn(String) + Send + Sync>>,
    enabled: bool,
    read_only: bool,
    fill_height: bool,
    placeholder: &Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    leading_icon: &Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    trailing_icon: &Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    colors: &Option<TextFieldColors>,
    interaction: &Option<crate::ui::interaction::MutableInteractionSource>,
) {
    let oc = on_query_change.clone();
    // NOTE: typing already writes into `query` (it IS the TextField value State);
    // on_query_change is purely the notification (cf. Compose onQueryChange).
    let mut field = TextField::new(query.clone())
        .no_container()
        .single_line(true)
        .enabled(enabled)
        .read_only(read_only)
        .on_value_change(move |v| {
            if let Some(cb) = &oc {
                cb(v.text.clone());
            }
        });
    if let Some(ph) = placeholder {
        let ph = ph.clone();
        field = field.placeholder(move |ctx| ph(ctx));
    }
    if let Some(li) = leading_icon {
        let li = li.clone();
        field = field.leading_icon(move |ctx| li(ctx));
    }
    if let Some(ti) = trailing_icon {
        let ti = ti.clone();
        field = field.trailing_icon(move |ctx| ti(ctx));
    }
    if let Some(c) = colors {
        field = field.colors(c.clone());
    }
    if let Some(src) = interaction {
        field = field.interaction_source(src.clone());
    }
    if let Some(search) = on_search {
        let search = search.clone();
        field = field.on_search(move |v| search(v.text.clone()));
    }
    let mut fm = Modifier::new().fill_max_width();
    if fill_height {
        // Collapsed pill: fixed bar height (tighten, NOT fill — a loose Box parent
        // hands down inf max, so fill never raises min and content top-aligns).
        fm = fm.height(SEARCH_BAR_HEIGHT);
    }
    field = field.modifier(fm);
    field.build(ctx);
}

/// Full-screen search bar (cf. classic `SearchBar`).
pub struct SearchBar {
    state: Option<SearchBarState>,
    on_query_change: Option<Arc<dyn Fn(String) + Send + Sync>>,
    on_search: Option<Arc<dyn Fn(String) + Send + Sync>>,
    enabled: bool,
    placeholder: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    leading_icon: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    trailing_icon: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    shape: Option<Shape>,
    colors: Option<SearchBarColors>,
    input_colors: Option<TextFieldColors>,
    shadow_elevation: f32,
    /// Docked dropdown shadow, independent of the bar (upstream expanded and
    /// collapsed elevations are separate params, both default Level0).
    dropdown_shadow_elevation: f32,
    modifier: Modifier,
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            state: None,
            on_query_change: None,
            on_search: None,
            enabled: true,
            placeholder: None,
            leading_icon: None,
            trailing_icon: None,
            shape: None,
            colors: None,
            input_colors: None,
            shadow_elevation: 0.0,
            dropdown_shadow_elevation: 0.0,
            modifier: Modifier::new(),
        }
    }

    pub fn state(mut self, s: SearchBarState) -> Self {
        self.state = Some(s);
        self
    }

    pub fn on_query_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_query_change = Some(Arc::new(f));
        self
    }

    /// Search action (IME Search / Enter). Callers conventionally deactivate here.
    pub fn on_search(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.on_search = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }

    pub fn placeholder(mut self, f: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.placeholder = Some(Arc::new(f));
        self
    }

    pub fn leading_icon(mut self, f: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.leading_icon = Some(Arc::new(f));
        self
    }

    pub fn trailing_icon(mut self, f: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.trailing_icon = Some(Arc::new(f));
        self
    }

    pub fn shape(mut self, s: Shape) -> Self {
        self.shape = Some(s);
        self
    }

    pub fn colors(mut self, c: SearchBarColors) -> Self {
        self.colors = Some(c);
        self
    }

    pub fn input_colors(mut self, c: TextFieldColors) -> Self {
        self.input_colors = Some(c);
        self
    }

    pub fn shadow_elevation(mut self, v: f32) -> Self {
        self.shadow_elevation = v;
        self
    }

    /// Shadow under the docked dropdown panel only (bar keeps
    /// `shadow_elevation`). Both default 0.0 (upstream Level0 parity).
    pub fn dropdown_shadow_elevation(mut self, v: f32) -> Self {
        self.dropdown_shadow_elevation = v;
        self
    }

    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + 'static) {
        let theme = crate::ui::theme::WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| SearchBarDefaults::colors(&theme));
        let state = self
            .state
            .unwrap_or_else(|| ctx.remember(SearchBarState::new).get());
        let active = state.active.get();
        let shape = self
            .shape
            .unwrap_or_else(SearchBarDefaults::input_field_shape);
        // Collapsed pill: ALWAYS composed (stable anchor + layout slot). When active
        // it sits under the fullscreen Dialog overlay (invisible, harmless). Tap
        // anywhere activates.
        let st = state.clone();
        let oc = self.on_query_change.clone();
        let os = self.on_search.clone();
        let en = self.enabled;
        let ph = self.placeholder.clone();
        let li = self.leading_icon.clone();
        let ti = self.trailing_icon.clone();
        let ic = self.input_colors.clone();
        crate::ui::surface::Surface::new()
            .shape(shape)
            .color(colors.container)
            .shadow_elevation(self.shadow_elevation)
            .on_click(move || st.open())
            // Collapsed bar fills parent width at fixed input height; user
            // modifier appends outside and may override.
            .modifier(
                Modifier::new()
                    .fill_max_width()
                    .height(SEARCH_BAR_HEIGHT)
                    .then(self.modifier),
            )
            .build(ctx, |ctx| {
                input_field(
                    ctx,
                    &state.query,
                    &oc,
                    &os,
                    en,
                    true, // read-only: tap expands instead of editing
                    true, // fill pill height: content centered
                    &ph, &li, &ti, &ic, &None,
                );
            });
        if !active {
            return;
        }
        // Expanded: fullscreen Dialog overlay (NOT in-tree — in-tree expansion would
        // push siblings off screen). Esc closes via the app-level overlay dismiss
        // hook (app.rs: topmost overlay closes first); Esc inside the overlay tree
        // would never fire (key dispatch only walks the main-tree focus path).
        let oc = self.on_query_change;
        let os = self.on_search;
        let en = self.enabled;
        let ph = self.placeholder;
        let li = self.leading_icon;
        let ti = self.trailing_icon;
        let ic = self.input_colors;
        let container = colors.container;
        let divider = colors.divider;
        let content = Arc::new(content);
        let st_dismiss = state.clone();
        crate::ui::overlay::Dialog::new(true)
            .on_dismiss_request(move || st_dismiss.close())
            // Fullscreen unfolds from the top + fades (approximates the upstream
            // bounds-morph expand; true shared-element morph needs anchor geometry).
            .enter_animation(Some(crate::ui::overlay::OverlayAnimSpec::expand_fade(
                std::time::Duration::from_millis(400),
            )))
            .exit_animation(Some(crate::ui::overlay::OverlayAnimSpec::expand_fade(
                std::time::Duration::from_millis(400),
            )))
            .build(ctx, move |ctx| {
                let key = ctx.next_key();
                match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        // Expanded leading defaults to a back arrow (cf. fullscreen
                        // SearchBar navigation icon); caller leading_icon overrides it.
                        let li: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>> =
                            match li.clone() {
                                Some(custom) => Some(custom),
                                None => {
                                    let st = state.clone();
                                    Some(Arc::new(move |ctx: &mut ComposeCtx| {
                                        let st = st.clone();
                                        crate::ui::icon_button::IconButton::new()
                                            .on_click(move || st.close())
                                            .build(ctx, |ctx| {
                                                crate::ui::icon::Icon::svg_path(BACK_ICON_PATH)
                                                    .size(24.0)
                                                    .build(ctx);
                                            });
                                    }))
                                }
                            };
                        crate::ui::surface::Surface::new()
                            .shape(Shape::Rectangle)
                            .color(container)
                            .modifier(Modifier::new().fill_max_size())
                            .build(ctx, |ctx| {
                                crate::ui::layout_components::Column::new().build(ctx, |ctx| {
                                        input_field(
                                            ctx,
                                            &state.query,
                                            &oc,
                                            &os,
                                            en,
                                            false,
                                            true, // fixed bar height (56) like collapsed
                                            &ph, &li, &ti, &ic, &None,
                                        );
                                        crate::ui::divider::Divider::horizontal()
                                            .color(divider)
                                            .build(ctx);
                                        content(ctx);
                                    });
                            });
                    }
                }
                ctx.end_restartable_group();
            });
    }
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Docked search bar (cf. `DockedSearchBar`): collapsed pill identical to `SearchBar`;
/// when active the results drop down under the bar (Popup-anchored, rounded, gap 2)
/// instead of fullscreen. No scrim in v1 (upstream default has one — divergence noted).
pub struct DockedSearchBar {
    inner: SearchBar,
}

impl DockedSearchBar {
    pub fn new() -> Self {
        Self { inner: SearchBar::new() }
    }

    pub fn state(mut self, s: SearchBarState) -> Self {
        self.inner = self.inner.state(s);
        self
    }

    pub fn on_query_change(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_query_change(f);
        self
    }

    pub fn on_search(mut self, f: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_search(f);
        self
    }

    pub fn enabled(mut self, v: bool) -> Self {
        self.inner = self.inner.enabled(v);
        self
    }

    pub fn placeholder(mut self, f: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.placeholder(f);
        self
    }

    pub fn leading_icon(mut self, f: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.leading_icon(f);
        self
    }

    pub fn trailing_icon(mut self, f: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.trailing_icon(f);
        self
    }

    pub fn shape(mut self, s: Shape) -> Self {
        self.inner = self.inner.shape(s);
        self
    }

    pub fn colors(mut self, c: SearchBarColors) -> Self {
        self.inner = self.inner.colors(c);
        self
    }

    pub fn input_colors(mut self, c: TextFieldColors) -> Self {
        self.inner = self.inner.input_colors(c);
        self
    }

    pub fn shadow_elevation(mut self, v: f32) -> Self {
        self.inner = self.inner.shadow_elevation(v);
        self
    }

    pub fn dropdown_shadow_elevation(mut self, v: f32) -> Self {
        self.inner = self.inner.dropdown_shadow_elevation(v);
        self
    }

    pub fn modifier(mut self, m: Modifier) -> Self {
        self.inner = self.inner.modifier(m);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + 'static) {
        let theme = crate::ui::theme::WiniaTheme::colors();
        let defaults = SearchBarDefaults::colors(&theme);
        let colors = self.inner.colors.clone().unwrap_or(defaults);
        let state = self
            .inner
            .state
            .clone()
            .unwrap_or_else(|| ctx.remember(SearchBarState::new).get());
        let active = state.active.get();
        // Collapsed pill (same as SearchBar, docked shape default).
        let st = state.clone();
        let inner = self.inner;
        let bar_shape = inner.shape.unwrap_or_else(SearchBarDefaults::docked_shape);
        let oc = inner.on_query_change.clone();
        let os = inner.on_search.clone();
        let en = inner.enabled;
        let ph = inner.placeholder.clone();
        let li = inner.leading_icon.clone();
        let ti = inner.trailing_icon.clone();
        let ic = inner.input_colors.clone();
        let shadow = inner.shadow_elevation;
        let drop_shadow = inner.dropdown_shadow_elevation;
        let user_mod = inner.modifier.clone();
        // Anchor container (DropdownMenu pattern): the pill lives inside an
        // explicit group whose node slot key anchors the dropdown. A bare
        // prev_sibling capture cannot work — statement keys and node keys live
        // in different key spaces (verified: prev_sibling=0xec61… vs pill
        // node=0xcc11…).
        // ⚠ Anchor = the WRAPPER group key (anchor_key) itself: the group
        // materializes one Box node carrying exactly this key at the pill's
        // position. composer_slot_key() is wrong here — it yields the last
        // materialized *inner* node (the input text leaf at x=38), shifting the
        // whole dropdown right to the text position.
        // ⚠ `read_only` is NOT registered via `ctx.changed()` (TextField never
        // declares it — see TextField::build: no `ctx.changed(&self.read_only)`).
        // Its only propagation path is the anchor group's Skip/Enter gate via
        // the parent Surface's modifier (Surface::build has no `changed` calls
        // either — so its modifier equality decides) plus the TextField key
        // remap below. `read_only` must therefore alter the *modifier chain*,
        // never just the builder field — otherwise the value flips silently
        // inside a reused slot and the dropdown input stays read-only
        // (typed keys swallowed, `filtered()` never re-runs → "no filtering").
        // We encode it as `fill_max_width(bool_marker)`: collapsed carries a
        // FillMaxWidth element, expanded does not (see overlay comment for the
        // full modifier-equality chain).
        let anchor_key = ctx.next_key();
        let anchor_modifier = if !active {
            Modifier::new().fill_max_width()
        } else {
            Modifier::new()
        };
        match ctx.start_restartable_group(anchor_key, anchor_modifier, crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                crate::ui::surface::Surface::new()
                    .shape(bar_shape)
                    .color(colors.container)
                    .shadow_elevation(shadow)
                    .on_click(move || st.open())
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(SEARCH_BAR_HEIGHT)
                            .then(user_mod),
                    )
                    .build(ctx, |ctx| {
                        input_field(
                            ctx,
                            &state.query,
                            &oc,
                            &os,
                            en,
                            !active, // collapsed: read-only to expand on tap; expanded: editable (single input — no duplicate in popup)
                            true, // fill pill height: content centered
                            &ph, &li, &ti, &ic, &None,
                        );
                    });
            }
        }
        let anchor_slot = anchor_key;
        ctx.end_restartable_group();
        // Docked dropdown: Popup anchored BottomLeft (= under anchor) + gap.
        // Input stays in the pill (which becomes editable after expand — pill
        // is not hidden behind an overlay like fullscreen). Popup only carries
        // the results, so we don't duplicate the input field (fixes the extra
        // input that appeared inside the panel).
        // Popup::build ALWAYS executes (visible=active contract): on close it
        // records active=false so sync_overlays deletes the overlay. An early
        // `if !active return` would look like a Skip frame and leak a zombie.
        let container = colors.container;
        let content = Arc::new(content);
        crate::ui::overlay::Popup::new(active)
            .position(crate::ui::overlay::PopupPosition::BottomLeft)
            .offset(0.0, SearchBarDefaults::docked_gap())
            .anchor_slot(Some(anchor_slot))
            .enter_animation(Some(crate::ui::overlay::OverlayAnimSpec::slide_down(
                std::time::Duration::from_millis(350),
            )))
            .exit_animation(Some(crate::ui::overlay::OverlayAnimSpec::slide_down(
                std::time::Duration::from_millis(350),
            )))
            .on_dismiss_request({
                let s = state.clone();
                move || s.close()
            })
            .build(ctx, move |ctx| {
                crate::ui::surface::Surface::new()
                    .shape(SearchBarDefaults::docked_shape())
                    .color(container)
                    .shadow_elevation(drop_shadow)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        content(ctx);
                    });
            });
    }
}

impl Default for DockedSearchBar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::constraints::Constraints;

    /// Compose needs a tokio runtime (TextField coroutine scope) — same helper
    /// pattern as text_field.rs `build_field`.
    fn with_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn node_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        let mut count = 0;
        fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, count: &mut usize) {
            *count += 1;
            for &c in &nodes[idx].children {
                walk(nodes, c, count);
            }
        }
        walk(nodes, root, &mut count);
        count
    }

    /// Count Text leaves in the tree (results content marker).
    fn text_leaves(composer: &Composer, needle: &str) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        let mut count = 0;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            needle: &str,
            count: &mut usize,
        ) {
            let n = &nodes[idx];
            if n.modifier.elements().iter().any(|el| {
                matches!(el, crate::modifier::ModifierElement::TextContent { content, .. } if content.contains(needle))
            }) {
                *count += 1;
            }
            for &c in &n.children {
                walk(nodes, c, needle, count);
            }
        }
        walk(nodes, root, needle, &mut count);
        count
    }

    #[test]
    fn inactive_hides_results_active_registers_overlay() {
        // Core state machine: inactive → no overlay, no results; open() → fullscreen
        // Dialog overlay registered (results live there, NOT in-tree — in-tree
        // expansion would push siblings off screen); close() → overlay gone.
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        let build = |composer: &mut Composer, state: &SearchBarState| {
            composer.compose(|ctx| {
                SearchBar::new()
                    .state(state.clone())
                    .placeholder(|ctx| {
                        crate::ui::text::Text::new("Search").build(ctx);
                    })
                    .build(ctx, |ctx| {
                        crate::ui::text::Text::new("RESULT_MARKER").build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "inactive: no overlay");
        assert_eq!(text_leaves(&composer, "RESULT_MARKER"), 0, "inactive: no results");
        assert!(node_count(&composer) > 0, "inactive: collapsed bar exists");
        state.open();
        build(&mut composer, &state);
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "active: fullscreen Dialog overlay registered");
        assert!(overlays[0].modal, "expanded search is modal (outside tap dismisses)");
        assert!(node_count(&composer) > 0, "active: pill persists under overlay");
        state.close();
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "closed: overlay gone");
        assert_eq!(text_leaves(&composer, "RESULT_MARKER"), 0, "closed: no results");
    }

    #[test]
    fn query_state_round_trips() {
        let state = SearchBarState::new();
        assert_eq!(state.query_text(), "");
        state.set_query("hello");
        assert_eq!(state.query_text(), "hello");
        assert_eq!(state.query.get().text, "hello");
    }

    #[test]
    fn docked_inactive_hides_results() {
        // Docked collapsed: no results, no popup. (Popup overlay needs app loop;
        // compose-level assert covers the collapsed half; expanded popup covered by
        // docked demo + Popup's own overlay tests.)
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        composer.compose(|ctx| {
            DockedSearchBar::new()
                .state(state.clone())
                .build(ctx, |ctx| {
                    crate::ui::text::Text::new("DOCKED_RESULT").build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        assert_eq!(text_leaves(&composer, "DOCKED_RESULT"), 0, "docked inactive: no results");
        assert!(node_count(&composer) > 0, "docked inactive: bar exists");
    }

    #[test]
    fn docked_active_registers_anchored_popup() {
        // Active docked: exactly one non-modal Popup overlay WITH an anchor slot
        // (explicit wrapper-group key — prev_sibling capture cannot work: statement
        // keys and node keys live in different key spaces). The anchor must
        // resolve to a laid-out node (the pill box), otherwise the dropdown
        // falls back to window alignment (lands at screen bottom).
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        let build = |composer: &mut Composer, state: &SearchBarState| {
            composer.compose(|ctx| {
                DockedSearchBar::new()
                    .state(state.clone())
                    .build(ctx, |ctx| {
                        crate::ui::text::Text::new("DOCKED_RESULT").build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "docked inactive: no overlay");
        state.open();
        build(&mut composer, &state);
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "docked active: popup overlay registered");
        assert!(!overlays[0].modal, "docked dropdown is non-modal");
        let anchor = overlays[0].anchor_slot.expect("docked popup must carry an anchor slot");
        let root = composer.layout_root_idx().expect("layout root exists");
        let nodes = composer.arena_nodes();
        let nid = crate::layout::node::find_node_id_by_slot_key(nodes, root, anchor);
        assert!(nid.is_some(), "anchor slot must resolve to a laid-out node (pill box)");
        state.close();
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "docked closed: overlay gone");
    }

    #[test]
    fn defaults_shapes_are_distinct() {
        // Pill (collapsed) vs Rectangle (fullscreen) vs rounded-12 (docked).
        assert_eq!(SearchBarDefaults::input_field_shape(), Shape::Pill);
        assert_eq!(SearchBarDefaults::full_screen_shape(), Shape::Rectangle);
        assert_ne!(
            SearchBarDefaults::docked_shape(),
            Shape::Rectangle,
            "docked keeps rounded corners"
        );
    }
}
