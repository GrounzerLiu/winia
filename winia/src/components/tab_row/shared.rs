// ── Token 常量 ──

/// 容器高度（PrimaryNavigationTabTokens.ContainerHeight = 48dp）
pub const TAB_ROW_HEIGHT: f32 = 48.0;
/// 指示条高度（PrimaryNavigationTabTokens.ActiveIndicatorHeight = 3dp）
pub const ACTIVE_INDICATOR_HEIGHT: f32 = 3.0;
/// 水平文本内边距（HorizontalTextPadding = 16dp）
pub const HORIZONTAL_TEXT_PADDING: f32 = 16.0;
/// 最小指示条宽度（24dp 触控目标）
pub const MIN_INDICATOR_WIDTH: f32 = 24.0;
/// 大号 Tab 高度（text+icon 时 = 72dp）
pub const LARGE_TAB_HEIGHT: f32 = 72.0;
/// 小号 Tab 高度（text-only 或 icon-only = 48dp）
pub const SMALL_TAB_HEIGHT: f32 = 48.0;
/// 图标与文本间距（IconDistanceFromBaseline 近似 = 20dp）
pub const ICON_TEXT_SPACING: f32 = 20.0;
/// LeadingIconTab 图标与文本间距（TextDistanceFromLeadingIcon = 8dp）
pub const LEADING_ICON_TEXT_SPACING: f32 = 8.0;
/// 可滚动 TabRow 最小 tab 宽（ScrollableTabRowMinTabWidth = 90dp）
pub const SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH: f32 = 90.0;
/// 可滚动 TabRow 起始边缘 padding（ScrollableTabRowEdgeStartPadding = 52dp）
pub const SCROLLABLE_TAB_ROW_EDGE_START_PADDING: f32 = 52.0;

// ── 动画规格 ──

/// The indicator's slide, which M3 runs on the `DefaultSpatial` token
/// (`material3/TabRow.kt:406`: `MotionSchemeKeyTokens.DefaultSpatial.value<Dp>()`) — damping 0.9,
/// stiffness 700 (`tokens/StandardMotionTokens.kt:20-21`). winia used damping 0.6, which bounced more
/// than M3's motion does.
pub(crate) fn indicator_spring() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
}

// ── TabPosition ──

/// 单个 tab 的位置信息，供指示条计算 offset/width。
///
/// 对标 Compose `TabPosition` 的 left/width/contentWidth/right。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabPosition {
    /// 左边缘 x 坐标（绝对坐标，LTR 下从左算起，RTL 下从右算起但存物理 x）
    pub left: f32,
    /// 宽度（等分宽）
    pub width: f32,
    /// 内容宽度——用于 Primary 指示条（followContentSize=true）；min 24dp
    pub content_width: f32,
    /// 右边缘 x 坐标（left + width）
    pub right: f32,
}

impl TabPosition {
    pub fn new(left: f32, width: f32, content_width: f32) -> Self {
        Self { left, width, content_width, right: left + width }
    }
}

