use super::*;

// ═══════════════════════════════════════════════════════════
// TextField — composable widget
// ═══════════════════════════════════════════════════════════


pub(crate) fn text_style_line_height(style: &TextStyle, font_size: f32) -> f32 {
    style.line_height
        .map(|height| height.to_logical_px(crate::runtime::density::current_density()))
        .unwrap_or(font_size * 1.4)
}

pub(crate) fn interpolate_text_style(from: &TextStyle, to: &TextStyle, progress: f32, color: crate::graphics::Color) -> TextStyle {
    let progress = progress.clamp(0.0, 1.0);
    let from_size = from.font_size.unwrap_or(16.0.into()).to_logical_px(crate::runtime::density::current_density());
    let to_size = to.font_size.unwrap_or(12.0.into()).to_logical_px(crate::runtime::density::current_density());
    let font_size = from_size + (to_size - from_size) * progress;
    let from_line_height = text_style_line_height(from, from_size);
    let to_line_height = text_style_line_height(to, to_size);
    let line_height = from_line_height + (to_line_height - from_line_height) * progress;
    let from_spacing = from.letter_spacing.unwrap_or(0.0);
    let to_spacing = to.letter_spacing.unwrap_or(0.0);

    TextStyle::new()
        .font_size(font_size)
        .line_height(line_height)
        .letter_spacing(from_spacing + (to_spacing - from_spacing) * progress)
        .font_weight(if progress < 0.5 {
            from.font_weight.unwrap_or_default()
        } else {
            to.font_weight.unwrap_or_default()
        })
        .font_style(if progress < 0.5 {
            from.font_style.unwrap_or_default()
        } else {
            to.font_style.unwrap_or_default()
        })
        .color(color)
}

/// TextField 容器布局 policy——子节点按角色定位（M3 specs）：
/// leading(12dp 左，垂直居中) | label(悬浮顶部/展开输入位，progress 插值)
/// | placeholder/prefix(输入前) | input(剩余宽) | suffix(输入后)
/// | trailing(右 12dp 居中)。图标与文本间距 16dp。
#[derive(Debug)]
pub(crate) struct TextFieldLayout {
    /// label 悬浮动画进度（0 = 展开 / 1 = 悬浮）——measure 期 peek 注册
    /// layout_dep，位置随动画每帧重测
    pub(crate) label_progress: crate::runtime::state::State<f32>,
    /// 容器视觉变体（决定悬浮 label 的锚点——M3 specs）：
    /// - Filled：label 顶在容器 8dp（悬浮区 8..24，中心 16）
    /// - Outlined：label 中心跨边框线（容器顶 0）
    pub(crate) variant: Option<TextFieldVariant>,
    /// 容器 padding（M3 specs：图标垂直居中于**容器**（56dp）——内容区
    /// 顶部即 pad_top——图标锚点 = 容器中心）
    pub(crate) pad_top: f32,
    pub(crate) pad_bottom: f32,
    /// supporting 区高（4 + body_small line height——容器 min_height 含此，居中推导须扣除）
    pub(crate) supporting_h: f32,
    /// 输入文本行高（px）——空文本输入子节点最小尺寸用。
    pub(crate) line_height: f32,
}

/// 内容区高度推导（label/图标容器居中锚点用）：
/// - 约束高有限（非滚动容器）→ 直接用
/// - 滚动容器内约束高 = f32::MAX → 用约束 min 高（modifier 层
///   min_height 经 padding offset 后 = **内容区 + supporting** min 高——
///   ⚠ 含 supporting（4+16=20），须扣除；空字段输入节点测量高 0
///   （空文本 paragraph），降级 input_size.height 会算负中心
/// - 多行：input_size.height 超过 min_height 兜底（24）时取 input 高——
///   否则 label/图标居中锚点恒为单行高，容器长高后图标/展开 label 不
///   跟随中心下移
/// - 兜底输入区高
/// The content height the field's slots centre in: the field's OWN box, not the room its parent happens to
/// have.
///
/// It used to return `constraints.max_height` whenever that was finite, which centres a slot in the SPACE
/// ABOVE the field instead of in the field — a trailing icon landed 88px into a 56px-tall field (measured:
/// slot `(260, 391, 13, 19)` against field `(16, 303, 280, 56)`), and only looked right where the parent
/// handed out an unbounded height (a scrolling column, which is every demo).
///
/// `constrain_height` honours a tight parent as well, so a fixed-height field (a 40dp pill) still centres
/// its slots in 40.
/// The colour spec for the field's focus transition: M3's `FastEffects` spring (1.0 / 3800), which is
/// what `TextFieldDefaults` passes to `animateColorAsState` (`material3/TextFieldDefaults.kt:246-248`
/// for the filled style, `:1068-1070` for the outlined one) instead of a 100 ms tween.
///
/// Recorded deviation: Compose animates `colors.containerColor(enabled, isError, focused)` there —
/// the CONTAINER. winia has no container-colour transition in this component and animates the
/// indicator/border colour instead, so the spring matches while the animated object does not
/// (`indicator_color` is not animated by M3). Named so the token is assertable.
pub(crate) fn text_field_colour_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::effects_fast()
}

pub(crate) fn text_field_content_height(constraints: &crate::layout::Constraints, supporting_h: f32, input_height: f32) -> f32 {
    constraints.constrain_height(input_height + supporting_h)
}

impl TextFieldLayout {
    pub(crate) fn new(
        label_progress: crate::runtime::state::State<f32>,
        variant: Option<TextFieldVariant>,
        pad_top: f32,
        pad_bottom: f32,
        supporting_h: f32,
        line_height: f32,
    ) -> Self {
        Self { label_progress, variant, pad_top, pad_bottom, supporting_h, line_height }
    }
}

impl crate::layout::MeasurePolicy for TextFieldLayout {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::Placement>) {
        use crate::layout::node::measure_node;
        use crate::modifier::ModifierElement;
        let role_of = |idx: usize| -> TextFieldSlotRole {
            nodes[idx].modifier.elements().iter().find_map(|el| {
                if let ModifierElement::TextFieldSlot { role } = el { Some(*role) } else { None }
            }).unwrap_or(TextFieldSlotRole::Input)
        };
        let roles: Vec<TextFieldSlotRole> = children.iter().map(|&c| role_of(c)).collect();
        let icon_c = crate::layout::Constraints::fixed(24.0, 24.0);
        let text_c = |max_w: f32| crate::layout::Constraints::new(0.0, max_w.max(0.0), 0.0, 30.0);
        // 第一轮：leading/trailing（图标）+ prefix/suffix（文本）——宽度
        // 决定 input 剩余空间
        let (mut leading_w, mut trailing_w) = (0.0f32, 0.0f32);
        let (mut prefix_w, mut suffix_w) = (0.0f32, 0.0f32);
        let mut placements: Vec<crate::layout::Placement> = Vec::new();
        for (i, &c) in children.iter().enumerate() {
            match roles[i] {
                TextFieldSlotRole::Leading => {
                    let (s, _) = measure_node(nodes, policies, c, icon_c);
                    leading_w = s.width;
                    // 贴内容区左端（容器左 padding 12 提供 M3 图标距边 12dp）
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                TextFieldSlotRole::Trailing => {
                    let (s, _) = measure_node(nodes, policies, c, icon_c);
                    trailing_w = s.width;
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                TextFieldSlotRole::Prefix => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    prefix_w = s.width;
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                TextFieldSlotRole::Suffix => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    suffix_w = s.width;
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                _ => placements.push(crate::layout::Placement { size: crate::unit::Size::ZERO, position: crate::unit::Offset::new(0.0, 0.0) }),
            }
        }
        // 第二轮：input——剩余宽 = **容器实际宽**（constraints.max_width 是
        // 可用空间上限——容器最终宽 = min_width 提升（如 280）而非 max（如
        // 380）——用 max 定位右对齐元素会越界（suffix/trailing 画到容器外）。
        // M3 间距（源码 TextFieldImpl.kt）：
        // - PrefixSuffixTextPadding = 2dp：prefix↔输入、输入↔suffix
        // - leading 后内容间距 = 4dp（startPadding = 16 - iconPadding(12)）
        // - suffix 右端 = trailing 左端（无间距）
        // - leading/trailing 垂直居中容器；prefix/suffix 与输入文本同位
        let left = if leading_w > 0.0 { leading_w + 4.0 } else { 0.0 };
        let right = if trailing_w > 0.0 { trailing_w } else { 0.0 };
        const AFFIX_GAP: f32 = 2.0; // PrefixSuffixTextPadding
        // 容器宽：min_width 兜底起步——input 测量后再按内容回算（超长输入
        // 撑宽容器）。⚠ 循环依赖：input 测量需要 input_w → 用 min 起步
        let width = constraints.min_width;
        let input_w = (width - left - prefix_w - AFFIX_GAP - suffix_w - right).max(0.0);
        let mut input_size = crate::unit::Size::ZERO;
        let mut input_pos_x = 0.0f32;
        for (i, &c) in children.iter().enumerate() {
            if roles[i] == TextFieldSlotRole::Input {
                let (s, _) = measure_node(nodes, policies, c, crate::layout::Constraints::new(0.0, input_w, 0.0, constraints.max_height));
                // ⚠ 空文本 paragraph 测量 0×0 → 输入节点尺寸 0 → render_pass1
                // `w<=0 || h<=0 return` → 光标不绘制（空字段聚焦无光标）。
                // 强制最小尺寸：高 ≥ token 行高、宽 ≥ 1（渲染进入 +
                // 光标绘制在内容起点）。
                let line_h = self.line_height;
                let s = crate::unit::Size::new(
                    if s.width < 1.0 { 1.0 } else { s.width },
                    if s.height < line_h { line_h } else { s.height },
                );
                input_size = s;
                // prefix 与输入 2dp（PrefixSuffixTextPadding）
                input_pos_x = left + prefix_w + AFFIX_GAP;
                // Bare no_container fields: container height == input height (see final
                // size below), so input y=0 is already centered. (Variants: M3 lower
                // text zone semantics, also y=0.)
                let input_y = 0.0;
                placements[i] = crate::layout::Placement { size: s, position: crate::unit::Offset::new(input_pos_x, input_y) };
                break;
            }
        }
        // 第三轮：label/placeholder 测量 + 全部定位
        let progress = self.label_progress.peek();
        // Bare-field content height: measured children max (NOT the constraint max —
        // loose parents hand down screen-sized maxes). Variants keep the
        // constraint-derived center (usually tight, M3 zone semantics).
        let bare_content_h = if self.variant.is_none() {
            let mut m = input_size.height;
            for p in placements.iter() {
                m = m.max(p.size.height);
            }
            // Policy constraints are already padding-stripped (content-area space):
            // do NOT subtract pad again (that double-counts and top-aligns).
            // Collapsed pill: min 40 → center in 40; expanded loose: min 0 → wrap.
            m.max(constraints.min_height.max(0.0))
        } else {
            0.0 // unused for variants
        };
        for (i, &c) in children.iter().enumerate() {
            match roles[i] {
                TextFieldSlotRole::Label => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    // 展开：**垂直居中于容器**（M3 specs：Label alignment
                    // (unpopulated) = vertically centered——容器中心；源码
                    // startY = CenterVertically.align(label.height, height)）；
                    // 悬浮：内容区顶部**上方**——锚点按变体：
                    // - Filled：label 顶对齐容器 8dp（内容区顶 24 → 偏移 -16）
                    // - Outlined：label 中心跨边框线（顶对齐容器 -8 → 偏移 -24）
                    // ⚠ policy 收到的是扣除 padding 后的约束——内容区顶部即
                    // 容器 padding 边界
                    let content_h = text_field_content_height(&constraints, self.supporting_h, input_size.height);
                    let container_center = (content_h + self.pad_bottom - self.pad_top) / 2.0;
                    let expanded_y = container_center - s.height / 2.0;
                    let float_y = -(s.height / 2.0)
                        - match self.variant {
                            Some(TextFieldVariant::Filled) => 8.0,
                            Some(TextFieldVariant::Outlined) => 16.0,
                            None => 0.0,
                        };
                    let y = expanded_y + (float_y - expanded_y) * progress;
                    placements[i] = crate::layout::Placement { size: s, position: crate::unit::Offset::new(input_pos_x, y) };
                }
                TextFieldSlotRole::Placeholder => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    // Bare fields: center in measured content height (see above).
                    // Variants: content top (M3 zone semantics).
                    let ph_y = if self.variant.is_none() {
                        ((bare_content_h - s.height) / 2.0).max(0.0)
                    } else {
                        0.0
                    };
                    placements[i] = crate::layout::Placement { size: s, position: crate::unit::Offset::new(input_pos_x, ph_y) };
                }
                TextFieldSlotRole::Leading => {
                    // Bare fields: center in measured content height (constraint max is
                    // screen-sized under loose parents).
                    // Variants: center in container (constraint-derived, usually tight).
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(
                            0.0,
                            ((bare_content_h - placements[i].size.height) / 2.0).max(0.0),
                        );
                        continue;
                    }
                    // 垂直居中于**容器**（M3 specs：Icon alignment = vertically
                    // centered——容器中心；输入文本区在容器下部（Filled 底 8 /
                    // Outlined 居中），居中于输入区会偏下）。容器中心（内容区
                    // 坐标）= (内容区高 + pad_bottom - pad_top)/2；允许负 y
                    //（Filled 图标跨 16..40 区）。⚠ 滚动容器内约束高 =
                    // f32::MAX（有限但巨大）——降级见 text_field_content_height
                    let content_h = text_field_content_height(&constraints, self.supporting_h, input_size.height);
                    let container_center = (content_h + self.pad_bottom - self.pad_top) / 2.0;
                    placements[i].position = crate::unit::Offset::new(
                        0.0,
                        container_center - placements[i].size.height / 2.0,
                    );
                }
                TextFieldSlotRole::Trailing => {
                    // Bare fields: same measured-height centering as Leading above.
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(
                            (width - placements[i].size.width).max(0.0),
                            ((bare_content_h - placements[i].size.height) / 2.0).max(0.0),
                        );
                        continue;
                    }
                    let content_h = text_field_content_height(&constraints, self.supporting_h, input_size.height);
                    let container_center = (content_h + self.pad_bottom - self.pad_top) / 2.0;
                    placements[i].position = crate::unit::Offset::new(
                        (width - placements[i].size.width).max(0.0),
                        container_center - placements[i].size.height / 2.0,
                    );
                }
                TextFieldSlotRole::Prefix => {
                    // Bare: center in measured content (input itself centered).
                    // Variants: with input text same vertical position.
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(left, ((bare_content_h - placements[i].size.height) / 2.0).max(0.0));
                    } else {
                        placements[i].position = crate::unit::Offset::new(left, (input_size.height - placements[i].size.height).max(0.0) / 2.0);
                    }
                }
                TextFieldSlotRole::Suffix => {
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(
                            (width - right - placements[i].size.width).max(0.0),
                            ((bare_content_h - placements[i].size.height) / 2.0).max(0.0),
                        );
                    } else {
                        // M3：suffix 右端 = trailing 左端（无间距）；与输入文本
                        // 间距 2dp（PrefixSuffixTextPadding——由 input_w 预留）
                        placements[i].position = crate::unit::Offset::new(
                            (width - right - placements[i].size.width).max(0.0),
                            (input_size.height - placements[i].size.height).max(0.0) / 2.0,
                        );
                    }
                }
                TextFieldSlotRole::Input => {
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(input_pos_x, ((bare_content_h - placements[i].size.height) / 2.0).max(0.0));
                    }
                }
            }
        }
        let _ = progress;
        // 容器尺寸：宽 = 内容（输入+前后缀+间距+图标区）受约束夹取（min_width
        // 280 兜底——超长输入撑宽）；高 = input 高 + supporting 区。
        // ⚠ supporting 恒在输入内容区外（渲染端画在容器底部 4dp 外侧，且
        // container_rect = 节点高 - supporting_h）——measure 高度必须含
        // supporting_h，否则多行换行（input 高超过 min_height 兜底）时
        // 节点总高缺 supporting 20px → 指示线/边框上移 → 穿过末行文字
        let width = constraints.constrain_width(input_size.width + left + prefix_w + AFFIX_GAP + suffix_w + right);
        let height = constraints.constrain_height(input_size.height + self.supporting_h);
        (crate::unit::Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[crate::layout::Placement]) {
        for (i, &c) in children.iter().enumerate() {
            if let Some(p) = placements.get(i) {
                nodes[c].position = p.position;
                nodes[c].measured_size = p.size;
            }
        }
    }
}


