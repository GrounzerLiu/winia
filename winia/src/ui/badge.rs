//! Badge / BadgedBox 组件 — 对标 material3 `Badge` / `BadgedBox`
//!
//! M3 实现要点（对齐项）：
//! - 小徽章（无内容）：6×6、CornerFull（3dp 圆角）、Error 底色；
//! - 大徽章（有内容）：min 16×16、CornerFull（8dp 圆角）、水平 padding 4dp、
//!   文本 LabelSmall（11sp/Medium）+ OnError 内容色；
//! - BadgedBox：自定义布局（anchor + badge 双子），badge 定位锚点右上角——
//!   无内容偏移 6×6（徽章左下角距锚点右上角），有内容偏移 12×14
//!   （Compose BadgeOffset / BadgeWithContent*Offset）；
//! - 徽章宽 > 6dp 视为有内容（Compose `badgePlaceable.width > BadgeTokens.Size`）。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::{Alignment, BoxLayout, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::layout::constraints::Constraints;
use crate::layout::node::measure_node;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::theme::WiniaTheme;

/// 小徽章尺寸（`BadgeTokens.Size = 6dp`）
pub const BADGE_SIZE: f32 = 6.0;
/// 大徽章最小尺寸（`BadgeTokens.LargeSize = 16dp`——单字符数字）
pub const BADGE_LARGE_SIZE: f32 = 16.0;
/// 小徽章圆角（`BadgeTokens.Shape = CornerFull`——6/2 = 3dp）
pub const BADGE_CORNER: f32 = 3.0;
/// 大徽章圆角（`BadgeTokens.LargeShape = CornerFull`——16/2 = 8dp）
pub const BADGE_LARGE_CORNER: f32 = 8.0;
/// 大徽章水平 padding（Compose `BadgeWithContentHorizontalPadding = 4dp`）
pub const BADGE_CONTENT_PADDING: f32 = 4.0;
/// 无内容徽章偏移（Compose `BadgeOffset = 6dp`——徽章左下角距锚点右上角 6×6）
pub const BADGE_OFFSET: f32 = 6.0;
/// 有内容徽章水平偏移（Compose `BadgeWithContentHorizontalOffset = 12dp`）
pub const BADGE_CONTENT_OFFSET_X: f32 = 12.0;
/// 有内容徽章垂直偏移（Compose `BadgeWithContentVerticalOffset = 14dp`——
/// 徽章底边距锚点顶边 14dp）
pub const BADGE_CONTENT_OFFSET_Y: f32 = 14.0;
/// 徽章文本字号（`TypographyKeyTokens.LabelSmall = 11sp`）
pub const BADGE_LABEL_FONT_SIZE: f32 = 11.0;

/// Badge 组件 Builder（对标 material3 `Badge(containerColor, contentColor,
/// content)`）——无内容 = 小圆点；`.content()` 后 = 大徽章（数字/短文本）。
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

    /// 徽章内容（数字/短文本等）——设置后为大徽章（min 16×16、圆角 8、padding 4）
    pub fn content(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.content = Some(Box::new(f));
        self
    }

    /// 容器色（默认 `BadgeTokens.Color = Error`）
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// 内容色（默认 `BadgeTokens.LargeLabelTextColor = OnError`）
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
        ctx.changed(&self.container_color);
        ctx.changed(&self.content_color);
        let has_content = self.content.is_some();
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.error);
        let content_color = self.content_color.unwrap_or(theme.on_error);
        let size = if has_content { BADGE_LARGE_SIZE } else { BADGE_SIZE };
        let shape = Shape::rounded(if has_content { BADGE_LARGE_CORNER } else { BADGE_CORNER });

        // 容器：min 尺寸 + 背景（CornerFull 形状）+ 内容水平 padding
        //（背景在 padding 前——画整个节点 rect 含 padding 区，Compose 同序）
        let mut m = Modifier::new()
            .min_width(size)
            .min_height(size)
            .background(container, shape);
        if has_content {
            m = m.padding_horizontal(BADGE_CONTENT_PADDING);
        }
        m = m.then(self.modifier);

        let key = ctx.next_key();
        let content = self.content;
        match ctx.start_restartable_group(
            key,
            m,
            BoxLayout::new().alignment(Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                if let Some(content) = content {
                    // 内容色下传 + 默认 LabelSmall 样式（Compose
                    // `ProvideContentColorTextStyle(LargeLabelTextFont)`）
                    WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                        let mut text_style = WiniaTheme::typography().label_small;
                        text_style.color = Some(content_color);
                        crate::ui::text::ProvideTextStyle(
                            text_style,
                            ctx,
                            |ctx| {
                                crate::ui::layout_components::Row::new()
                                    .alignment(Alignment::Center)
                                    .build(ctx, content);
                            },
                        );
                    });
                }
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for Badge {
    fn default() -> Self { Self::new() }
}

/// BadgedBox 布局策略：anchor（子 0）全约束测量 + badge（子 1）宽松高度测量；
/// 尺寸 = anchor 尺寸；badge 定位锚点右上角（偏移按徽章宽度推断有无内容）。
/// 对标 Compose `BadgedBox` 的 Layout 实现（无 BadgeEndRuler/BadgeTopRuler
/// 导航栏边界钳制——winia 无对应概念，文档注明差距）。
#[derive(Debug, Clone)]
struct BadgedBoxPolicy;

impl MeasurePolicy for BadgedBoxPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        debug_assert_eq!(children.len(), 2, "BadgedBox 需要 anchor + badge 两个子节点");
        if children.len() < 2 {
            return (Size::ZERO, Vec::new());
        }
        // badge 先测（宽松高度——文本不占多余空间，Compose copy(minHeight = 0)）；
        // anchor 全约束
        let (badge_size, _) = measure_node(nodes, policies, children[1], constraints.loosen());
        let (anchor_size, _) = measure_node(nodes, policies, children[0], constraints);

        // 用徽章宽度推断有无内容（Compose：width > BadgeTokens.Size）
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
                position: Point::new(0.0, 0.0),
            },
            Placement {
                size: badge_size,
                // 徽章左下角距锚点右上角 (off_x, off_y)：
                // x = 锚点右缘 - off_x；y = -徽章高 + off_y（徽章可越出锚点顶部）
                position: Point::new(width - off_x, -badge_size.height + off_y),
            },
        ];
        (Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            if i >= placements.len() { break; }
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

/// BadgedBox 组件 Builder（对标 material3 `BadgedBox(badge, modifier, content)`）
/// ——anchor（content）与 badge 组合，badge 定位锚点右上角。
/// 典型用法：图标右上角挂数字徽章（导航栏/消息图标）。
pub struct BadgedBox {
    badge: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    modifier: Modifier,
}

impl BadgedBox {
    /// badge 内容闭包（通常放 `Badge` 组件）
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

    /// build 的内容闭包 = anchor（徽章挂载的主体）
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // 容器节点：自定义布局策略（anchor + badge 双子）
        ctx.start_container(key, self.modifier, BadgedBoxPolicy);
        // 子 1：anchor（居中——anchor 内容即用户 content）
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
        // 子 2：badge
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

    fn light_theme() -> crate::ui::theme::ThemeColors {
        crate::ui::theme::ThemeColors::light_from_seed(0x6750A4)
    }

    #[test]
    fn badge_default_colors_from_theme() {
        let theme = light_theme();
        assert_ne!(theme.error, theme.on_error, "error/on_error 应有区分");
    }

    // ── 结构测试：BadgedBox 的 badge 定位 ──
    // 约定：anchor 24×24（右上角 (24,0)），badge 无内容 6×6 → 位置
    // (24-6, -6+6) = (18, 0)；有内容 16×16 → (24-12, -16+14) = (12, -2)
    struct BadgeScene {
        anchor: (f32, f32, f32, f32),
        badge: (f32, f32, f32, f32),
    }

    fn render_badged_box(badge_content: bool) -> BadgeScene {
        let theme = light_theme();
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                if badge_content {
                    BadgedBox::new(|ctx| {
                        Badge::new().content(|ctx| {
                            crate::ui::Text::new("3").build(ctx);
                        }).build(ctx);
                    }).build(ctx, |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new()
                            .size(24.0, 24.0)
                            .background(Color::WHITE, Shape::Rectangle));
                        ctx.end_node();
                    });
                } else {
                    BadgedBox::new(|ctx| {
                        Badge::new().build(ctx);
                    }).build(ctx, |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new()
                            .size(24.0, 24.0)
                            .background(Color::WHITE, Shape::Rectangle));
                        ctx.end_node();
                    });
                }
            });
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // BadgedBox 容器（尺寸 24×24 且 children=2）→ children[0]=anchor 组、
        // children[1]=badge 组（位置由 BadgedBoxPolicy 放置）
        let nodes = composer.arena_nodes();
        let mut anchor = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut badge = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for (i, n) in nodes.iter().enumerate() {
            if n.measured_size.width == 24.0 && n.measured_size.height == 24.0 && n.children.len() == 2 {
                let a = &nodes[n.children[0]];
                anchor = (a.position.x, a.position.y, a.measured_size.width, a.measured_size.height);
                let b = &nodes[n.children[1]];
                badge = (b.position.x, b.position.y, b.measured_size.width, b.measured_size.height);
                break;
            }
        }
        assert!(badge.2 != 0.0 || badge.3 != 0.0, "未找到 BadgedBox 容器（24×24 ch=2）");
        BadgeScene { anchor, badge }
    }

    #[test]
    fn badged_box_positions_small_badge() {
        let s = render_badged_box(false);
        assert_eq!((s.anchor.0, s.anchor.1), (0.0, 0.0), "anchor 应位于原点");
        assert_eq!((s.badge.0, s.badge.1), (18.0, 0.0),
            "小徽章应位于 (18,0)——左下角距锚点右上角 6×6（实际 {:?}）", s.badge);
        assert_eq!((s.badge.2, s.badge.3), (6.0, 6.0), "小徽章 6×6");
    }

    #[test]
    fn badged_box_positions_large_badge() {
        let s = render_badged_box(true);
        assert_eq!((s.anchor.0, s.anchor.1), (0.0, 0.0), "anchor 应位于原点");
        assert_eq!(s.badge.0, 12.0,
            "大徽章水平：左缘距锚点右缘 12dp（实际 {:?}）", s.badge);
        assert_eq!(s.badge.1, -2.0,
            "大徽章垂直：底边距锚点顶 14dp → y = -h+14 = -2（实际 {:?}）", s.badge);
    }

    // ── 像素测试：Badge 渲染 ──
    fn render_badge_px(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = light_theme();
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        // scene 捕获 FnOnce（build）——只能 compose 一次
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
        // 小点 6×6 在 (0,0)——中心 (3,3) 应为 Error 色
        let c = at(&px, w, 3.0, 3.0);
        assert!(
            close(c, (theme.error.r as i32, theme.error.g as i32, theme.error.b as i32)),
            "小徽章中心应为 Error 色（实际 {c:?}，期望 {:?}）",
            (theme.error.r, theme.error.g, theme.error.b)
        );
    }

    #[test]
    fn badge_with_content_renders_container_and_label() {
        let theme = light_theme();
        let (px, w) = render_badge_px(|ctx| {
            Badge::new().content(|ctx| {
                crate::ui::Text::new("3").build(ctx);
            }).build(ctx);
        });
        // 16×16 徽章区域统计：Error 底色像素应占多数（容器背景），
        // 同时存在 OnError 文字像素（"3" 覆盖中心——避免单点采样命中文字）
        let error_target = (theme.error.r as i32, theme.error.g as i32, theme.error.b as i32);
        let mut error_px = 0;
        let mut label_px = 0;
        for y in 0..16i32 {
            for x in 0..16i32 {
                let p = at(&px, w, x as f32, y as f32);
                if close(p, error_target) {
                    error_px += 1;
                } else if !(p.0 > 245 && p.1 > 245 && p.2 > 245) {
                    // 非白且非 Error——文字像素（on_error 为近白色系）
                    label_px += 1;
                }
            }
        }
        assert!(error_px > 100, "大徽章底色应为 Error（16×16 内实际 {error_px} 像素）");
        assert!(label_px > 5, "大徽章内应有 OnError 文字像素（实际 {label_px}）");
    }

    #[test]
    fn badged_box_badge_visible_on_render() {
        // 渲染级验证：BadgedBox 的徽章确实画出来（右上角有 Error 像素）
        let theme = light_theme();
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                BadgedBox::new(|ctx| {
                    Badge::new().build(ctx);
                }).build(ctx, |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new()
                        .size(24.0, 24.0)
                        .background(Color::from_argb(255, 200, 200, 200), Shape::Rectangle));
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
        // 徽章中心 (21,3)（(18,0) 起 6×6）应为 Error 色
        let c = at(21.0, 3.0);
        assert!(
            close(c, (theme.error.r as i32, theme.error.g as i32, theme.error.b as i32)),
            "BadgedBox 徽章区域应有 Error 像素（实际 {c:?}）"
        );
        // anchor 内部 (12,12) 为灰色（anchor 内容）
        let a = at(12.0, 12.0);
        assert!(close(a, (200, 200, 200)), "anchor 内容应正常渲染（实际 {a:?}）");
    }
}


