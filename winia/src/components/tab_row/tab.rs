use super::*;

// ── Tab 组件 ──

/// Material 3 Tab（对齐 Compose Tab.kt 通用版本）。
///
/// 用法：
/// ```ignore
/// Tab::new(selected, || { on_click() })
///     .text("Label")
///     .icon(|| Icon::new(icon_source).size(24.0).build(ctx))
///     .build(ctx);
/// ```
///
/// 或自定义 content：
/// ```ignore
/// Tab::new(selected, || { on_click() })
///     .content(|ctx| { Text::new("Tab").build(ctx); })
///     .build(ctx);
/// ```
pub struct Tab {
    pub(crate) selected: bool,
    pub(crate) on_click: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    pub(crate) text: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    pub(crate) icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    pub(crate) content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    pub(crate) enabled: bool,
    pub(crate) selected_content_color: Option<Color>,
    pub(crate) unselected_content_color: Option<Color>,
    /// LeadingIconTab 模式：icon 左 + text 右（水平排列，SmallTabHeight）
    pub(crate) leading: bool,
    /// 外部交互源注入（缺省内部创建）
    pub(crate) interaction_source: Option<crate::input::interaction::MutableInteractionSource>,
    pub(crate) modifier: Modifier,
}

impl Tab {
    pub fn new(selected: bool, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            selected,
            on_click: Some(std::sync::Arc::new(on_click)),
            text: None,
            icon: None,
            content: None,
            enabled: true,
            selected_content_color: None,
            unselected_content_color: None,
            leading: false,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    /// 设为 LeadingIconTab 模式（icon 左 + text 右，水平排列 SmallTabHeight）。
    pub fn leading_icon(mut self) -> Self {
        self.leading = true;
        self
    }

    /// 注入外部交互源（缺省内部创建），用于外部观察 pressed/hover/focus 状态。
    pub fn interaction_source(mut self, source: crate::input::interaction::MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn text(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.text = Some(Box::new(content));
        self
    }
    pub fn icon(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.icon = Some(Box::new(content));
        self
    }
    /// 自定义 content（替代 text/icon 槽）
    pub fn content(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.content = Some(Box::new(content));
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn selected_content_color(mut self, color: Color) -> Self {
        self.selected_content_color = Some(color);
        self
    }
    pub fn unselected_content_color(mut self, color: Color) -> Self {
        self.unselected_content_color = Some(color);
        self
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.selected);
        ctx.changed(&self.enabled);
        ctx.changed(&self.selected_content_color);
        ctx.changed(&self.unselected_content_color);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        let selected = self.selected;
        let enabled = self.enabled;

        // 颜色：默认取 contentColor（LocalContentColor.current）
        let content_color = WiniaTheme::content_color();
        let sel_color = self.selected_content_color.unwrap_or(content_color);
        let unsel_color = self.unselected_content_color.unwrap_or_else(|| {
            TabRowDefaults::unselected_content_color(&theme)
        });

        // The content colour, animated per Compose's `TabTransition` — see `tab_colour_spec` for the
        // reference and why the direction decides the spec. The remembered `was_selected` is what says
        // which side of that transition this frame is on.
        let was_selected = ctx.remember(|| selected);
        let arriving = selected && !was_selected.peek();
        was_selected.set(selected);
        let color_spec = crate::animation::AnimationSpec::Spring(tab_colour_spec(arriving));
        let color_anim = ctx.animate_color_as_state(if selected { sel_color } else { unsel_color }, color_spec);

        let has_text = self.text.is_some();
        let has_icon = self.icon.is_some();
        let has_content = self.content.is_some();
        let leading = self.leading;

        // 点击交互：优先用外部注入的交互源（须由调用方 remember 创建——
        // 与 Compose interactionSource 参数语义一致），缺省内部创建
        let interaction = match self.interaction_source.clone() {
            Some(src) => src,
            None => ctx.remember(|| crate::input::interaction::MutableInteractionSource::new()).get(),
        };
        let callback = self.on_click.clone();

        let mut item_modifier = Modifier::new();
        if enabled {
            if let Some(cb) = callback {
                item_modifier = item_modifier.clickable_with_source(&interaction, move || cb());
            }
        }
        // 全节点 ripple（bounded，使用选中色）
        let ripple_modifier = if enabled {
            Modifier::new().ripple_with_shape(&interaction, sel_color, true, Shape::Rectangle)
        } else {
            Modifier::new()
        };
        item_modifier = item_modifier.semantics(
            crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::Tab)
                .merge_descendants(true)
                .state(crate::semantics::SemanticsState::new().selected(self.selected).enabled(enabled)),
        );
        let item_modifier = item_modifier.then(self.modifier);

        // 自定义 content 版：直接包装点击+ripple（颜色动画由用户 content 自理）
        if has_content {
            let content = self.content.unwrap();
            match ctx.start_restartable_group(key, item_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    content(ctx);
                    // ripple leaf（fill_max_size 确保全尺寸覆盖——BoxLayout
                    // Center 测子节点用 loosen 约束，无尺寸则塌缩 0×0）
                    let ripple_key = ctx.next_key();
                    ctx.start_leaf(ripple_key, ripple_modifier.fill_max_size());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
            return;
        }

        // 动态颜色 tint（渲染期 peek 零重组；layout_deps 由 TabLayoutPolicy.measure
        // 读 color_anim.get() 注册——动画帧重测 Tab 节点 → 重绘 → 新色生效）
        let tint_for_icon = color_anim.clone();
        let icon_tint_modifier = Modifier::new().graphics_layer(move || crate::graphics::GraphicsLayerParams {
            color_filter: Some(crate::graphics::ColorFilter::Tint {
                color: tint_for_icon.peek(),
                blend_mode: crate::graphics::BlendMode::SrcIn,
            }),
            ..crate::graphics::GraphicsLayerParams::default()
        });
        let tint_for_text = color_anim.clone();
        let text_tint_modifier = Modifier::new().graphics_layer(move || crate::graphics::GraphicsLayerParams {
            color_filter: Some(crate::graphics::ColorFilter::Tint {
                color: tint_for_text.peek(),
                blend_mode: crate::graphics::BlendMode::SrcIn,
            }),
            ..crate::graphics::GraphicsLayerParams::default()
        });

        // 默认 text/icon 版：TabLayoutPolicy
        let policy = TabLayoutPolicy { has_text, has_icon, leading, direction, color_anim: color_anim.clone() };

        match ctx.start_restartable_group(key, item_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 1) icon (if present)——Tint 染色后无需 with_content_color 再注入
                if let Some(icon) = self.icon {
                    let icon_key = ctx.next_key();
                    match ctx.start_restartable_group(icon_key, icon_tint_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            icon(ctx);
                        }
                    }
                    ctx.end_restartable_group();
                }
                // 2) text (if present) — 非 leading 水平填充 16dp；leading 模式
                // 不加（间距由 LEADING_ICON_TEXT_SPACING 精确控制——加了会把
                // icon-text 间隙撑成 8+16）。颜色由 tint 统一驱动
                if let Some(text) = self.text {
                    let text_key = ctx.next_key();
                    let style = TabRowDefaults::label_text_style();
                    let mut text_modifier = Modifier::new().then(text_tint_modifier);
                    if !leading {
                        text_modifier = text_modifier.padding_horizontal(HORIZONTAL_TEXT_PADDING);
                    }
                    match ctx.start_restartable_group(text_key, text_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            crate::components::text::ProvideTextStyle(style, ctx, text);
                        }
                    }
                    ctx.end_restartable_group();
                }
                // 3) ripple leaf (full size, bounded)
                let ripple_key = ctx.next_key();
                ctx.start_leaf(ripple_key, ripple_modifier);
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    }
}

// ── TabLayoutPolicy ──

/// Tab 内部布局：对标 Compose TabBaselineLayout。
/// children = [icon?, text?, ripple]。
///
/// 简化基线：
/// - text-only / icon-only ⇒ 垂直居中
/// - text + icon ⇒ 垂直居中排列（icon 上、text 下，居中）
///   （偏差：Compose 用 FirstBaseline/LastBaseline 精确偏移）
#[derive(Debug)]
struct TabLayoutPolicy {
    pub(crate) has_text: bool,
    pub(crate) has_icon: bool,
    /// LeadingIconTab 模式：icon 左 + text 右
    pub(crate) leading: bool,
    /// 布局方向（RTL 时镜像 leading 排列）
    pub(crate) direction: LayoutDirection,
    /// 颜色动画 State——measure 开头 get() 注册 layout_dep，动画帧重测
    /// Tab 节点 → 触发重绘 → graphics_layer color_filter peek 新色
    pub(crate) color_anim: State<crate::graphics::Color>,
}

impl MeasurePolicy for TabLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // ⚠ 必须最先读颜色动画 State——此刻 ACTIVE_SLOT_KEY 仍是本节点（Tab，
        // measure_node 进入时设置）；递归测量子节点后 key 会被改写。注册
        // layout_dep → 动画帧只重测本节点（不重组）→ 重绘 → tint peek 新色。
        self.color_anim.get();
        let n = children.len();
        // 识别内容子节点：无 ripple（最后一个除外）
        let content_children = &children[..n - 1];
        let has_icon = self.has_icon;
        let has_text = self.has_text;
        let leading = self.leading;

        // ⚠ 用 loose 约束测量内容：TabRow 传 tight（等分宽 × 行高），若直接
        // 用 tight 测 slot，icon/text slot 各自被撑到行高（如 72），content_height
        // 求和翻倍 → tab_height 溢出（72+72+20=164），ripple 覆盖多余区域且
        // icon/text 间距失真。loose 约束拿到内容自然尺寸，再钳制回 incoming。
        let loose = Constraints::new(0.0, constraints.max_width, 0.0, f32::MAX);
        let mut content_sizes = Vec::with_capacity(content_children.len());
        for &child in content_children {
            let (size, _) = measure_node(nodes, policies, child, loose);
            content_sizes.push(size);
        }

        // 自然尺寸
        let tab_width_natural = if leading && content_sizes.len() >= 2 {
            // LeadingIconTab：icon + 8dp + text 水平排列——自然宽 = 三者之和
            content_sizes.iter().map(|s| s.width).sum::<f32>() + LEADING_ICON_TEXT_SPACING
        } else {
            content_sizes.iter().map(|s| s.width).fold(0.0f32, f32::max)
        };
        let spec_height = if leading {
            SMALL_TAB_HEIGHT
        } else if has_icon && has_text { LARGE_TAB_HEIGHT } else { SMALL_TAB_HEIGHT };
        let content_height: f32 = if leading {
            // leading 垂直取 max（水平排列不叠加高度）
            content_sizes.iter().map(|s| s.height).fold(0.0f32, f32::max)
        } else {
            content_sizes.iter().map(|s| s.height).sum()
        };
        let tab_height_natural = if leading {
            spec_height.max(content_height)
        } else {
            spec_height.max(content_height + ICON_TEXT_SPACING)
        };

        // 钳制到 incoming 约束：TabRow 传 tight（tabWidth × rowHeight）——
        // tab 必须填满分配的 slot（ripple 覆盖整个 tab 区域，对齐 Compose
        // selectable + fillMaxWidth）；父约束松时保持自然尺寸。
        let tab_width = tab_width_natural.clamp(constraints.min_width, constraints.max_width);
        let tab_height = tab_height_natural.clamp(constraints.min_height, constraints.max_height);

        // 布局
        let mut placements = Vec::with_capacity(n);

        if leading && has_icon && has_text && content_sizes.len() >= 2 {
            // LeadingIconTab：icon 左 + 8dp + text 右，整组水平居中、垂直居中
            // ⚠ RTL 镜像：icon 移右侧、text 移左侧（Compose Row 在 RTL 下
            // 自动镜像子节点顺序——物理排列反转）
            let icon_size = content_sizes[0];
            let text_size = content_sizes[1];
            let total_w = icon_size.width + LEADING_ICON_TEXT_SPACING + text_size.width;
            let start_x = (tab_width - total_w) / 2.0;
            let icon_y = (tab_height - icon_size.height) / 2.0;
            let text_y = (tab_height - text_size.height) / 2.0;
            let is_rtl = self.direction == LayoutDirection::Rtl;
            let icon_x = if is_rtl {
                start_x + text_size.width + LEADING_ICON_TEXT_SPACING
            } else {
                start_x
            };
            let text_x = if is_rtl {
                start_x
            } else {
                start_x + icon_size.width + LEADING_ICON_TEXT_SPACING
            };
            placements.push(Placement {
                size: icon_size,
                position: Offset::new(icon_x, icon_y),
            });
            placements.push(Placement {
                size: text_size,
                position: Offset::new(text_x, text_y),
            });
        } else if has_icon && has_text && content_sizes.len() >= 2 {
            // text+icon：icon 上、text 下，垂直居中排列，均水平居中
            let icon_size = content_sizes[0];
            let text_size = content_sizes[1];
            let total_h = icon_size.height + text_size.height;
            let start_y = (tab_height - total_h) / 2.0;
            placements.push(Placement {
                size: icon_size,
                position: Offset::new((tab_width - icon_size.width) / 2.0, start_y),
            });
            placements.push(Placement {
                size: text_size,
                position: Offset::new((tab_width - text_size.width) / 2.0, start_y + icon_size.height),
            });
        } else if has_icon || has_text {
            // 单元素：水平 + 垂直居中
            for &size in content_sizes.iter() {
                let y = (tab_height - size.height) / 2.0;
                placements.push(Placement {
                    size,
                    position: Offset::new((tab_width - size.width) / 2.0, y),
                });
            }
        } else {
            for &size in content_sizes.iter() {
                placements.push(Placement { size, position: Offset::new(0.0, 0.0) });
            }
        }

        // ripple leaf：全尺寸覆盖（填满整个 tab slot）
        placements.push(Placement {
            size: Size::new(tab_width, tab_height),
            position: Offset::new(0.0, 0.0),
        });

        (Size::new(tab_width, tab_height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            if let Some(p) = placements.get(index) {
                nodes[child].position = p.position;
                nodes[child].measured_size = p.size;
            }
        }
    }
}

