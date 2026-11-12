//! Badge / BadgedBox — material3's `Badge` / `BadgedBox`.
//!
//! The M3 details it reproduces:
//! - small badge (no content): 6×6, CornerFull (3dp corner), Error container;
//! - large badge (with content): min 16×16, CornerFull (8dp corner), 4dp horizontal padding,
//!   LabelSmall text (11sp/Medium) with the container's paired content colour;
//! - BadgedBox: a custom layout (anchor + badge children) that places the badge at the anchor's
//!   top-trailing corner — offset 6×6 with no content (the badge's bottom-leading corner from the
//!   anchor's top-trailing one) and 12×14 with content
//!   (Compose's `BadgeOffset` / `BadgeWithContent*Offset`);
//! - a badge wider than 6dp counts as "has content"
//!   (Compose: `badgePlaceable.width > BadgeTokens.Size`).

use crate::composable;
use crate::graphics::{Color, Shape};
use crate::layout::constraints::Constraints;
use crate::layout::node::measure_node;
use crate::layout::{Alignment, BoxLayout, LayoutNode, MeasurePolicy, Placement};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::theme::WiniaTheme;
use crate::unit::{Offset, Size};

/// The small badge's size (`BadgeTokens.Size = 6dp`).
pub const BADGE_SIZE: f32 = 6.0;
/// The large badge's minimum size (`BadgeTokens.LargeSize = 16dp` — one digit).
pub const BADGE_LARGE_SIZE: f32 = 16.0;
/// The small badge's corner (`BadgeTokens.Shape = CornerFull` — 6/2 = 3dp).
pub const BADGE_CORNER: f32 = 3.0;
/// The large badge's corner (`BadgeTokens.LargeShape = CornerFull` — 16/2 = 8dp).
pub const BADGE_LARGE_CORNER: f32 = 8.0;
/// The large badge's horizontal padding (Compose `BadgeWithContentHorizontalPadding = 4dp`).
pub const BADGE_CONTENT_PADDING: f32 = 4.0;
/// The no-content badge's offset (Compose `BadgeOffset = 6dp` — the badge's bottom-leading corner
/// 6×6 from the anchor's top-trailing one).
pub const BADGE_OFFSET: f32 = 6.0;
/// The with-content badge's horizontal offset (Compose `BadgeWithContentHorizontalOffset = 12dp`).
pub const BADGE_CONTENT_OFFSET_X: f32 = 12.0;
/// The with-content badge's vertical offset (Compose `BadgeWithContentVerticalOffset = 14dp` — the
/// badge's bottom edge 14dp from the anchor's top edge).
pub const BADGE_CONTENT_OFFSET_Y: f32 = 14.0;

/// The `Badge` builder (material3's `Badge(containerColor, contentColor, content)`) — no content
/// means a small dot; `.content()` makes it a large badge (a number or a short label).
pub struct Badge {
    container_color: Option<Color>,
    content_color: Option<Color>,
    content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
}

impl Badge {
    pub fn new() -> Self {
        Self {
            container_color: None,
            content_color: None,
            content: None,
            modifier: Modifier::new(),
        }
    }

    /// The badge's content (a number or a short label) — setting it makes this a large badge
    /// (min 16×16, corner 8, padding 4).
    pub fn content(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.content = Some(Box::new(f));
        self
    }

    /// The container colour (default `BadgeTokens.Color = Error`).
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// The content colour (default: `contentColorFor(containerColor)`, which is OnError for the
    /// default Error container — Compose's `Badge.kt:135`).
    pub fn content_color(mut self, c: Color) -> Self {
        self.content_color = Some(c);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let has_content = self.content.is_some();
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.error);
        // Compose's default is `contentColorFor(containerColor)`, which maps a scheme colour to its paired
        // content colour and falls back to `LocalContentColor.current` for anything else
        // (`Badge.kt:135`, `ColorScheme.kt:1064-1115`) — winia used `on_error` unconditionally.
        let content_color = self
            .content_color
            .or_else(|| theme.content_color_for(container))
            .unwrap_or_else(WiniaTheme::content_color);
        // Declare the RESOLVED values, not the raw `Option<Color>`s — the rule `button.rs` states
        // ("resolve BEFORE declaring parameters: the resolved set is itself a parameter"). The two
        // options compare equal while the derived content colour moves with the container, so a Clean
        // slot kept painting the old label on a background that had already repainted.
        ctx.changed(&container);
        ctx.changed(&content_color);
        let size = if has_content {
            BADGE_LARGE_SIZE
        } else {
            BADGE_SIZE
        };
        let shape = Shape::rounded(if has_content {
            BADGE_LARGE_CORNER
        } else {
            BADGE_CORNER
        });

        // Container: default min size + background (CornerFull shape) + content horizontal padding,
        // with the CALLER's modifier OUTERMOST — Compose's chain is
        // `modifier.defaultMinSize(size).background(color, shape).padding(horizontal)`, so a caller's
        // own padding sits outside the badge's background and its own size wins over the token floor.
        // Both halves of that matter together: `default_min_size` only applies where the incoming
        // minimum is still 0, so a caller's `width(2.dp)` must be resolved BEFORE this element — put
        // the caller last (the way winia used to) and the floor would win, pinning the badge at its
        // token size no matter what the caller asked for.
        let mut m = Modifier::new()
            .default_min_size(size, size)
            .background(container, shape);
        if has_content {
            m = m.padding_horizontal(BADGE_CONTENT_PADDING);
        }
        m = self.modifier.then(m);

        let key = ctx.next_key();
        let content = self.content;
        match ctx.start_restartable_group(key, m, BoxLayout::new().alignment(Alignment::Center)) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // The label repaints through the provider below: the inner `Row` declares only its
                // arrangement/alignment/spacing, so it stays Clean and Skips, and an untracked
                // provider would never run again — a white `onError` label outlived a switch to
                // `surfaceContainerHigh` (measured).
                if let Some(content) = content {
                    // The content colour and the default LabelSmall style, in ONE call — Compose's
                    // `ProvideContentColorTextStyle(LargeLabelTextFont)`.
                    crate::components::text::ProvideContentColorTextStyle(
                        content_color,
                        WiniaTheme::typography().label_small,
                        ctx,
                        |ctx| {
                            crate::layout::components::Row::new()
                                .alignment(Alignment::Center)
                                // The stored content is a `FnOnce(&mut ComposeCtx)`; the row hands
                                // its scope, which derefs to `ComposeCtx`, so forward it.
                                .build(ctx, |scope| content(scope));
                        },
                    );
                }
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::new()
    }
}

/// The BadgedBox layout policy: the anchor (child 0) is measured with the full constraints and the
/// badge (child 1) with a loosened height; the size is the anchor's; the badge sits at the anchor's
/// top-trailing corner (the offset is picked from the badge's width, which is how "has content" is
/// inferred).
///
/// This is Compose's `BadgedBox` layout without the `BadgeEndRuler`/`BadgeTopRuler` navigation-bar
/// clamping — winia has no such concept; `docs/badge.md` records the gap.
#[derive(Debug, Clone)]
struct BadgedBoxPolicy {
    /// The direction the badge mirrors under — Compose places both children with `placeRelative`, so the
    /// badge sits at the anchor's TRAILING edge: the right one in LTR, the left one under RTL.
    direction: crate::layout::LayoutDirection,
}

impl MeasurePolicy for BadgedBoxPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        debug_assert_eq!(
            children.len(),
            2,
            "BadgedBox needs exactly two children: the anchor and the badge"
        );
        if children.len() < 2 {
            return (Size::ZERO, Vec::new());
        }
        // The badge is measured first, with a loosened height so its text does not claim more space
        // than it needs (Compose's `copy(minHeight = 0)`); the anchor gets the full constraints.
        let (badge_size, _) =
            measure_node(nodes, policies, children[1], constraints.loosen_height());
        let (anchor_size, _) = measure_node(nodes, policies, children[0], constraints);

        // The badge's WIDTH is what says whether it has content (Compose: `width > BadgeTokens.Size`).
        let has_content = badge_size.width > BADGE_SIZE;
        let (off_x, off_y) = if has_content {
            (BADGE_CONTENT_OFFSET_X, BADGE_CONTENT_OFFSET_Y)
        } else {
            (BADGE_OFFSET, BADGE_OFFSET)
        };

        let width = constraints.constrain_width(anchor_size.width);
        let height = constraints.constrain_height(anchor_size.height);

        let placements = vec![
            Placement {
                size: anchor_size,
                position: Offset::new(0.0, 0.0),
            },
            Placement {
                size: badge_size,
                // The badge's leading edge sits `off_x` from the anchor's trailing edge and its bottom
                // `off_y` below the anchor's top (`-h + off_y`, so it may hang above it). Under RTL the
                // horizontal half mirrors — `placeRelative`'s rule (`containerWidth - x - width`), which is
                // why the badge ends up at the anchor's left edge there.
                position: Offset::new(
                    if self.direction == crate::layout::LayoutDirection::Rtl {
                        width - (width - off_x) - badge_size.width
                    } else {
                        width - off_x
                    },
                    -badge_size.height + off_y,
                ),
            },
        ];
        (Size::new(width, height), placements)
    }

    /// The box reports the ANCHOR's lines and nothing else — Compose passes them as the layout's own
    /// `alignmentLines` so "default baseline calculations" cannot include the badge
    /// (`material3/Badge.kt:92-100`). The anchor is placed at the origin, so its lines are already
    /// expressed in this box's coordinates.
    fn alignment_lines(
        &self,
        nodes: &[LayoutNode],
        children: &[usize],
    ) -> Option<Vec<(crate::layout::AlignmentLine, f32)>> {
        Some(match children.first() {
            Some(&anchor) => nodes[anchor].alignment_lines.clone(),
            None => Vec::new(),
        })
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            if i >= placements.len() {
                break;
            }
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

/// The `BadgedBox` builder (material3's `BadgedBox(badge, modifier, content)`) — pairs the anchor
/// (the build content) with a badge it places at the anchor's top-trailing corner.
///
/// The usual case: a status count hung on an icon (a navigation bar item, a message icon).
pub struct BadgedBox {
    badge: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    modifier: Modifier,
}

impl BadgedBox {
    /// The badge's content closure (usually a `Badge`).
    pub fn new(badge: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            badge: Box::new(badge),
            modifier: Modifier::new(),
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// The build content is the ANCHOR — the thing the badge hangs on.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // The direction comes from the modifier when a caller set one and from the ambient scope otherwise,
        // the way `Stack` resolves it — the policy needs it at build time because it is not re-created for a
        // measure (and a modifier's direction is a per-node override).
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or_else(crate::layout::direction::current);
        // The container node: a custom layout policy over the two children.
        ctx.start_container(key, self.modifier, BadgedBoxPolicy { direction });
        // Child 1: the anchor, centred — its content is the caller's `content`.
        let ak = ctx.next_key();
        match ctx.start_restartable_group(
            ak,
            Modifier::new(),
            BoxLayout::new().alignment(Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                content(ctx);
            }
        }
        ctx.end_restartable_group();
        // Child 2: the badge.
        let bk = ctx.next_key();
        match ctx.start_restartable_group(
            bk,
            Modifier::new(),
            BoxLayout::new().alignment(Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                (self.badge)(ctx);
            }
        }
        ctx.end_restartable_group();
        ctx.end_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light_theme() -> crate::theme::ThemeColors {
        crate::theme::ThemeColors::light_from_seed(0x6750A4)
    }

    // ── Audit probes: the four things Compose does that winia did not ──

    /// Compose derives a Badge's content colour from its CONTAINER colour — `contentColorFor` maps the
    /// scheme's pairs (`error → onError`, `primary → onPrimary`, …) and falls back to `LocalContentColor`
    /// for anything else (`ColorScheme.kt:1064-1115`, used at `Badge.kt:135`). winia always used `on_error`,
    /// so a badge on a primary container painted onError text and never consulted the ambient content colour.
    #[test]
    fn the_content_colour_follows_the_container_colour() {
        let theme = light_theme();
        let seen: std::sync::Arc<std::sync::Mutex<Vec<Color>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = |colours: std::sync::Arc<std::sync::Mutex<Vec<Color>>>| {
            move |ctx: &mut ComposeCtx| {
                colours.lock().unwrap().push(WiniaTheme::content_color());
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        let s = seen.clone();
        composer.compose(move |ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                WiniaTheme::with_content_color(theme.tertiary, ctx, |ctx| {
                    // A scheme colour: the mapping decides, not the ambient colour. surfaceContainerHigh
                    // → onSurface, which is dark in a light scheme — unlike onError, so the two answers
                    // are distinguishable.
                    Badge::new()
                        .container_color(theme.surface_container_high)
                        .content(record(s.clone()))
                        .build(ctx);
                    // Not a scheme colour: Compose falls back to the ambient content colour.
                    Badge::new()
                        .container_color(Color::from_argb(255, 1, 2, 3))
                        .content(record(s.clone()))
                        .build(ctx);
                    // An explicit content colour wins over both.
                    Badge::new()
                        .container_color(theme.primary)
                        .content_color(Color::from_argb(255, 9, 9, 9))
                        .content(record(s.clone()))
                        .build(ctx);
                });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let got = seen.lock().unwrap();
        assert_eq!(got.len(), 3, "three badges composed");
        assert_eq!(
            got[0], theme.on_surface,
            "a scheme container maps to its own content colour (surfaceContainerHigh → onSurface)"
        );
        assert_eq!(
            got[1], theme.tertiary,
            "a custom container falls back to the ambient content colour"
        );
        assert_eq!(
            got[2],
            Color::from_argb(255, 9, 9, 9),
            "an explicit content colour wins"
        );
    }

    /// Compose places the anchor and the badge with `placeRelative`, so the badge sits at the anchor's
    /// TRAILING edge — the right one in LTR, the left one under RTL (`Badge.kt:130-131`). winia always used
    /// `width - offset`, which is the right edge in both directions.
    #[test]
    fn a_badge_mirrors_under_rtl() {
        let theme = light_theme();
        let scene = |direction: crate::layout::LayoutDirection| {
            let theme = theme.clone();
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(theme.clone(), direction, ctx, |ctx| {
                    BadgedBox::new(|ctx| {
                        Badge::new().build(ctx);
                    })
                    .build(ctx, |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(
                            k,
                            Modifier::new()
                                .size(24.0, 24.0)
                                .background(Color::WHITE, Shape::Rectangle),
                        );
                        ctx.end_node();
                    });
                });
            }
        };
        let badge_x = |direction| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(scene(direction));
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let nodes = composer.arena_nodes();
            for n in nodes.iter() {
                if n.measured_size.width == 24.0
                    && n.measured_size.height == 24.0
                    && n.children.len() == 2
                {
                    return nodes[n.children[1]].position.x;
                }
            }
            panic!("no BadgedBox found");
        };
        assert_eq!(
            badge_x(crate::layout::LayoutDirection::Ltr),
            18.0,
            "LTR: flush with the right edge"
        );
        assert_eq!(
            badge_x(crate::layout::LayoutDirection::Rtl),
            0.0,
            "RTL: mirrored to the left edge (a 6-wide badge at 18 mirrored is 24 - 18 - 6)"
        );
    }

    /// Compose measures the badge with `constraints.copy(minHeight = 0)` — the comment says why (a badge's
    /// text must not claim more height than it needs), and only the HEIGHT is loosened, so a min width
    /// reaching the box reaches the badge too. winia loosened both axes.
    #[test]
    fn the_badge_keeps_the_min_width_of_the_box() {
        let theme = light_theme();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                BadgedBox::new(|ctx| {
                    Badge::new().build(ctx);
                })
                .build(ctx, |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(24.0, 24.0));
                    ctx.end_node();
                });
            });
        });
        composer.layout(crate::layout::Constraints::new(40.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        for n in nodes.iter() {
            if n.children.len() == 2 && n.measured_size.width >= 40.0 {
                let badge = &nodes[n.children[1]];
                assert_eq!(
                    badge.measured_size.width, 40.0,
                    "the badge takes the box's min width (only the height is loosened), as Compose does"
                );
                return;
            }
        }
        panic!("no BadgedBox found");
    }

    /// Compose's BadgedBox reports the ANCHOR's baselines as its own
    /// (`alignmentLines = mapOf(FirstBaseline to firstBaseline, LastBaseline to lastBaseline)`,
    /// `Badge.kt:97-100`), with the comment saying why: "to avoid default baseline calculations from
    /// including by any badge content". winia's container merged every child's lines, so a badge's own text
    /// baseline moved the line a Row aligns a whole row of icons by.
    #[test]
    fn a_badges_baseline_does_not_move_the_anchors() {
        let theme = light_theme();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                BadgedBox::new(|ctx| {
                    Badge::new()
                        .content(|ctx| {
                            crate::components::Text::new("9").font_size(30.0).build(ctx);
                        })
                        .build(ctx);
                })
                .build(ctx, |ctx| {
                    crate::components::Text::new("anchor")
                        .font_size(40.0)
                        .build(ctx);
                });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        for n in nodes.iter() {
            if n.children.len() == 2
                && n.alignment_line(crate::layout::AlignmentLine::FIRST_BASELINE)
                    .is_some()
            {
                let own = n
                    .alignment_line(crate::layout::AlignmentLine::FIRST_BASELINE)
                    .unwrap();
                let anchor_child = &nodes[n.children[0]];
                let badge_child = &nodes[n.children[1]];
                let anchor_line = anchor_child
                    .alignment_line(crate::layout::AlignmentLine::FIRST_BASELINE)
                    .expect("the anchor reports a baseline");
                assert_eq!(
                    own, anchor_line,
                    "the box reports the anchor's baseline, not a merge with the badge's"
                );
                return;
            }
        }
        panic!("no BadgedBox with a baseline found");
    }

    #[test]
    fn badge_default_colors_from_theme() {
        let theme = light_theme();
        assert_ne!(
            theme.error, theme.on_error,
            "error and on_error must be distinguishable"
        );
    }

    // ── Structural tests: where BadgedBox places the badge ──
    // The convention: anchor 24×24 (its top-trailing corner at (24,0)); a 6×6 dot badge lands at
    // (24-6, -6+6) = (18, 0); a 16×16 content badge at (24-12, -16+14) = (12, -2).
    struct BadgeScene {
        anchor: (f32, f32, f32, f32),
        badge: (f32, f32, f32, f32),
    }

    fn render_badged_box(badge_content: bool) -> BadgeScene {
        let theme = light_theme();
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                if badge_content {
                    BadgedBox::new(|ctx| {
                        Badge::new()
                            .content(|ctx| {
                                crate::components::Text::new("3").build(ctx);
                            })
                            .build(ctx);
                    })
                    .build(ctx, |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(
                            k,
                            Modifier::new()
                                .size(24.0, 24.0)
                                .background(Color::WHITE, Shape::Rectangle),
                        );
                        ctx.end_node();
                    });
                } else {
                    BadgedBox::new(|ctx| {
                        Badge::new().build(ctx);
                    })
                    .build(ctx, |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(
                            k,
                            Modifier::new()
                                .size(24.0, 24.0)
                                .background(Color::WHITE, Shape::Rectangle),
                        );
                        ctx.end_node();
                    });
                }
            });
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // The BadgedBox container (24×24 with two children) → children[0] is the anchor group and
        // children[1] the badge group, placed by `BadgedBoxPolicy`.
        let nodes = composer.arena_nodes();
        let mut anchor = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut badge = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for (i, n) in nodes.iter().enumerate() {
            if n.measured_size.width == 24.0
                && n.measured_size.height == 24.0
                && n.children.len() == 2
            {
                let a = &nodes[n.children[0]];
                anchor = (
                    a.position.x,
                    a.position.y,
                    a.measured_size.width,
                    a.measured_size.height,
                );
                let b = &nodes[n.children[1]];
                badge = (
                    b.position.x,
                    b.position.y,
                    b.measured_size.width,
                    b.measured_size.height,
                );
                break;
            }
        }
        assert!(
            badge.2 != 0.0 || badge.3 != 0.0,
            "no BadgedBox container found (24×24 with two children)"
        );
        BadgeScene { anchor, badge }
    }

    #[test]
    fn badged_box_positions_small_badge() {
        let s = render_badged_box(false);
        assert_eq!(
            (s.anchor.0, s.anchor.1),
            (0.0, 0.0),
            "the anchor sits at the origin"
        );
        assert_eq!(
            (s.badge.0, s.badge.1),
            (18.0, 0.0),
            "the dot badge sits at (18,0) — its bottom-leading corner 6×6 from the anchor's \
             top-trailing one (actual {:?})",
            s.badge
        );
        assert_eq!((s.badge.2, s.badge.3), (6.0, 6.0), "the dot badge is 6×6");
    }

    #[test]
    fn badged_box_positions_large_badge() {
        let s = render_badged_box(true);
        assert_eq!(
            (s.anchor.0, s.anchor.1),
            (0.0, 0.0),
            "the anchor sits at the origin"
        );
        assert_eq!(
            s.badge.0, 12.0,
            "the large badge's leading edge is 12dp from the anchor's trailing edge (actual {:?})",
            s.badge
        );
        assert_eq!(
            s.badge.1, -2.0,
            "the large badge's bottom edge is 14dp from the anchor's top edge → y = -h+14 = -2 \
             (actual {:?})",
            s.badge
        );
    }

    // ── Badge content reactivity (Compose's idiom: read a State in the nearest scope that uses it)

    /// The correct pattern: `State::get` is called INSIDE the badge content closure, so the
    /// dependency registers on the badge content group (the innermost scope). A `set` then marks
    /// that group dirty directly, plus its ancestors, so the recomposition necessarily reaches the
    /// content through the groups Badge itself owns.
    #[test]
    fn badge_content_recomposes_when_state_read_inside_content_closure() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let theme = light_theme();
        let count = crate::runtime::state::State::new(3i32);
        let hits = std::sync::Arc::new(AtomicUsize::new(0));

        let make_scene = |count: crate::runtime::state::State<i32>,
                          hits: std::sync::Arc<AtomicUsize>,
                          theme: crate::theme::ThemeColors| {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    BadgedBox::new(move |ctx| {
                        Badge::new()
                            .content(move |ctx| {
                                hits.fetch_add(1, Ordering::SeqCst);
                                // Read in the INNERMOST scope: the dependency hangs on the badge
                                // content group.
                                let n = count.get();
                                crate::components::Text::new(n.to_string()).build(ctx);
                            })
                            .build(ctx);
                    })
                    .build(ctx, |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().size(24.0, 24.0));
                        ctx.end_node();
                    });
                });
            }
        };

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(count.clone(), hits.clone(), theme.clone()));
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "the first composition runs it once"
        );

        count.set(4);
        let ran = composer.recompose(make_scene(count.clone(), hits.clone(), theme));
        assert!(ran, "a state change must trigger a recomposition");
        assert_eq!(
            hits.load(Ordering::SeqCst),
            2,
            "the badge content group must re-run when the state moves"
        );
    }

    // Note: reading OUTSIDE the content closure (at the container group) also propagates in the
    // isolated harness above (the ancestor is marked dirty and the container re-runs with the new
    // closure), but a real component tree is deeper and which layer a composition dependency lands
    // on moves with the structure. Compose's idiom is to read in the NEAREST scope that uses the
    // value — a navigation-bar count badge must use the pattern the test above pins down.

    // ── Pixel tests: what Badge renders ──
    fn render_badge_px(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = light_theme();
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        // `scene` captures a FnOnce (`build`), so it can only be composed once.
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), 300)
    }

    fn at(px: &[[u8; 4]], w: usize, x: f32, y: f32) -> (i32, i32, i32) {
        let p = px[(y as usize) * w + (x as usize)];
        (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
    }

    fn close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 6 && (a.1 - b.1).abs() <= 6 && (a.2 - b.2).abs() <= 6
    }

    #[test]
    fn badge_dot_renders_error_color() {
        let theme = light_theme();
        let (px, w) = render_badge_px(|ctx| {
            Badge::new().build(ctx);
        });
        // The 6×6 dot sits at (0,0), so its centre (3,3) is the Error colour.
        let c = at(&px, w, 3.0, 3.0);
        assert!(
            close(
                c,
                (
                    theme.error.r as i32,
                    theme.error.g as i32,
                    theme.error.b as i32
                )
            ),
            "the dot badge's centre is the Error colour (actual {c:?}, expected {:?})",
            (theme.error.r, theme.error.g, theme.error.b)
        );
    }

    #[test]
    fn badge_with_content_renders_container_and_label() {
        let theme = light_theme();
        let (px, w) = render_badge_px(|ctx| {
            Badge::new()
                .content(|ctx| {
                    crate::components::Text::new("3").build(ctx);
                })
                .build(ctx);
        });
        // Statistics over the 16×16 badge: the Error container colour should dominate (it is the
        // background), and OnError label pixels must be present too ("3" sits over the centre, so a
        // single-point sample could land on text instead of background).
        let error_target = (
            theme.error.r as i32,
            theme.error.g as i32,
            theme.error.b as i32,
        );
        let mut error_px = 0;
        let mut label_px = 0;
        for y in 0..16i32 {
            for x in 0..16i32 {
                let p = at(&px, w, x as f32, y as f32);
                if close(p, error_target) {
                    error_px += 1;
                } else if !(p.0 > 245 && p.1 > 245 && p.2 > 245) {
                    // Neither white nor Error — a label pixel (on_error is near-white).
                    label_px += 1;
                }
            }
        }
        assert!(
            error_px > 100,
            "the large badge's container is the Error colour ({error_px} pixels in the 16×16 area)"
        );
        assert!(
            label_px > 5,
            "the large badge has OnError label pixels (actual {label_px})"
        );
    }

    #[test]
    fn badged_box_badge_visible_on_render() {
        // Render-level proof that BadgedBox actually DRAWS the badge (Error pixels in the
        // top-trailing corner).
        let theme = light_theme();
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                BadgedBox::new(|ctx| {
                    Badge::new().build(ctx);
                })
                .build(ctx, |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(
                        k,
                        Modifier::new()
                            .size(24.0, 24.0)
                            .background(Color::from_argb(255, 200, 200, 200), Shape::Rectangle),
                    );
                    ctx.end_node();
                });
            });
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        // The badge's centre (21,3) — a 6×6 box starting at (18,0) — is the Error colour.
        let c = at(21.0, 3.0);
        assert!(
            close(
                c,
                (
                    theme.error.r as i32,
                    theme.error.g as i32,
                    theme.error.b as i32
                )
            ),
            "the BadgedBox badge area is the Error colour (actual {c:?})"
        );
        // Inside the anchor (12,12) is grey — the anchor's own content.
        let a = at(12.0, 12.0);
        assert!(
            close(a, (200, 200, 200)),
            "the anchor's content renders normally (actual {a:?})"
        );
    }

    // ── Regression: the label must follow a container colour that moves at runtime ──

    /// Declaring the raw `Option<Color>`s as parameters is not enough. Two options can compare EQUAL
    /// while the derived content colour moves with the container, and the inner `Row` declares no
    /// parameters at all — so it stayed Clean, `with_content_color` never ran again, and the label
    /// kept the old colour. Measured before the fix: a white `onError` label outlived a switch to
    /// `surfaceContainerHigh`, whose paired content colour (`onSurface`) is dark.
    #[test]
    fn the_label_follows_a_container_colour_that_moves_at_runtime() {
        let theme = light_theme();
        assert_ne!(
            theme.on_error, theme.on_surface,
            "the two answers must be distinguishable for this test to mean anything"
        );
        let colour: crate::runtime::state::State<Color> =
            crate::runtime::state::State::new(theme.error);
        let seen: std::sync::Arc<std::sync::Mutex<Vec<Color>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let scene = |colour: crate::runtime::state::State<Color>,
                     seen: std::sync::Arc<std::sync::Mutex<Vec<Color>>>| {
            let theme = theme.clone();
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    // Read in the OUTER scope: the badge is handed a new container colour, it does not
                    // read the state itself.
                    let container = colour.get();
                    Badge::new()
                        .container_color(container)
                        .content(move |ctx| {
                            seen.lock().unwrap().push(WiniaTheme::content_color());
                            crate::components::Text::new("3").build(ctx);
                        })
                        .build(ctx);
                });
            }
        };

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(scene(colour.clone(), seen.clone()));
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!(
            seen.lock().unwrap().last().copied(),
            Some(theme.on_error),
            "an error container starts with its paired onError label"
        );

        colour.set(theme.surface_container_high);
        // Converge the way a frame does: the provider's notify is queued during the pass that
        // discovers the move and acted on by the next one, so a single `recompose` would assert
        // `mark_subtree_dirty`'s same-pass timing rather than the signal's arrival.
        let ran = composer.recompose_to_stable(|| scene(colour.clone(), seen.clone()));
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(
            ran,
            "the container colour change must trigger a recomposition"
        );
        assert_eq!(
            seen.lock().unwrap().last().copied(),
            Some(theme.on_surface),
            "the label must pick up the new container's paired content colour (it kept onError before)"
        );
    }

    /// An EXPLICIT content colour must re-enter too — it is a declared parameter, but the group that
    /// provides it is the inner `Row`, which declares none.
    #[test]
    fn an_explicit_content_colour_that_moves_reaches_the_label() {
        let theme = light_theme();
        let colour: crate::runtime::state::State<Color> =
            crate::runtime::state::State::new(Color::from_argb(255, 11, 11, 11));
        let seen: std::sync::Arc<std::sync::Mutex<Vec<Color>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let scene = |colour: crate::runtime::state::State<Color>,
                     seen: std::sync::Arc<std::sync::Mutex<Vec<Color>>>| {
            let theme = theme.clone();
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let c = colour.get();
                    Badge::new()
                        .content_color(c)
                        .content(move |ctx| {
                            seen.lock().unwrap().push(WiniaTheme::content_color());
                            crate::components::Text::new("3").build(ctx);
                        })
                        .build(ctx);
                });
            }
        };

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(scene(colour.clone(), seen.clone()));
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));

        colour.set(Color::from_argb(255, 99, 99, 99));
        // Same reason as `the_label_follows_a_container_colour_that_moves_at_runtime`: the notify is
        // queued during the pass that sees the new colour, and the reader re-runs on the next one.
        composer.recompose_to_stable(|| scene(colour.clone(), seen.clone()));
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!(
            seen.lock().unwrap().last().copied(),
            Some(Color::from_argb(255, 99, 99, 99)),
            "an explicit content colour that changed must reach the label"
        );
    }

    /// Compose chains `defaultMinSize`, not a hard floor: `defaultMinSize` yields to a minimum the
    /// CALLER already set, so `Badge(modifier = Modifier.width(2.dp))` measures 2dp wide. A
    /// `min_width` floor wins the other way and pins the badge at its token size.
    #[test]
    fn a_caller_can_shrink_the_badge_below_its_token_size() {
        let theme = light_theme();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Badge::new().modifier(Modifier::new().width(2.0)).build(ctx);
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("root");
        assert_eq!(
            nodes[root].measured_size.width, 2.0,
            "the caller's width wins over the badge's own floor (Compose's `defaultMinSize`)"
        );
    }

    /// And with no caller override the token size still applies — both axes.
    #[test]
    fn the_token_size_still_applies_without_a_caller_override() {
        let theme = light_theme();
        let measure = |content: bool| {
            let theme = theme.clone();
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let badge = Badge::new();
                    let badge = if content {
                        badge.content(|ctx| {
                            crate::components::Text::new("3").build(ctx);
                        })
                    } else {
                        badge
                    };
                    badge.build(ctx);
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("root");
            (
                nodes[root].measured_size.width,
                nodes[root].measured_size.height,
            )
        };
        assert_eq!(
            measure(false),
            (BADGE_SIZE, BADGE_SIZE),
            "the dot badge is 6×6"
        );
        assert_eq!(
            measure(true),
            (BADGE_LARGE_SIZE, BADGE_LARGE_SIZE),
            "a one-digit large badge is 16×16"
        );
    }
}
