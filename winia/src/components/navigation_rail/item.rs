use super::*;

// ── NavigationRailItem ──

pub struct NavigationRailItem {
    pub(crate) selected: bool,
    pub(crate) on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) label: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,

    pub(crate) enabled: bool,
    pub(crate) always_show_label: bool,
    pub(crate) colors: Option<NavigationRailItemColors>,
    pub(crate) interaction_source: Option<MutableInteractionSource>,
    pub(crate) modifier: Modifier,
}

impl NavigationRailItem {
    pub fn new(selected: bool, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            selected,
            on_click: None,
            icon: Box::new(icon),
            label: None,
            enabled: true,
            always_show_label: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn on_click(mut self, callback: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn label(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.label = Some(Box::new(content));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn always_show_label(mut self, always: bool) -> Self {
        self.always_show_label = always;
        self
    }

    pub fn colors(mut self, colors: NavigationRailItemColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
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
        ctx.changed(&self.always_show_label);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| rail_item_colors(&theme));
        let selected = self.selected;
        let enabled = self.enabled;
        let has_label = self.label.is_some();

        // 双进度动画（同导航栏）
        let alpha_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_alpha_spec());
        let size_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_item_size_spec());

        let icon_color = colors.icon_color(selected, enabled);
        let label_color = colors.text_color(selected, enabled);
        let ripple_color = icon_color;

        let interaction = self.interaction_source.clone()
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());

        // 彩色胶囊背景：颜色 x alphaProgress——绘制期 peek 不注册依赖
        let indicator_color = colors.indicator;
        let indicator_alpha = alpha_progress.clone();
        let indicator_modifier = Modifier::new().background(
            move || with_alpha_factor(indicator_color, indicator_alpha.peek()),
            Shape::Pill,
        );
        // ripple 状态层载体——独立节点、恒定全尺寸（未选中不收拢）
        let ripple_modifier = if enabled {
            Modifier::new().ripple_with_shape(&interaction, ripple_color, true, Shape::Pill)
        } else {
            Modifier::new()
        };

        // label 包装层：alwaysShowLabel=false 时透明度跟 alphaProgress
        let label_alpha_state = alpha_progress.clone();
        let label_slot_modifier = if has_label && !self.always_show_label {
            Modifier::new().graphics_layer(move || GraphicsLayerParams {
                alpha: label_alpha_state.peek(),
                ..GraphicsLayerParams::default()
            })
        } else {
            Modifier::new()
        };

        let policy = NavigationRailItemLayoutPolicy {
            size_progress: size_progress.clone(),
            always_show_label: self.always_show_label,
            has_label,
        };

        // item：min 宽 80（NarrowContainerWidth）；androidx defaultMinSize(minHeight=ItemHeight)
        // 由 policy 内 min_h 表达（56 基准）
        let mut item_modifier = Modifier::new().min_width(NAVIGATION_RAIL_WIDTH);
        if enabled {
            if let Some(callback) = self.on_click.clone() {
                item_modifier = item_modifier.clickable_with_source(&interaction, move || callback());
            }
        }
        let item_modifier = item_modifier.then(self.modifier);

        match ctx.start_restartable_group(key, item_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 子节点顺序 = [indicator, icon, label?, ripple]——ripple 最后放置
                // （z 序最上层，对齐 androidx placeRelative 顺序）
                // 1) 彩色指示器胶囊（宽度随 sizeProgress 展开动画）
                let ind_key = ctx.next_key();
                ctx.start_leaf(ind_key, indicator_modifier);
                ctx.end_node();
                // 2) 图标（内容色注入）
                let icon = self.icon;
                wrap_slot(ctx, Modifier::new(), |ctx| {
                    WiniaTheme::with_content_color(icon_color, ctx, icon);
                });
                // 3) label（LabelMedium + 文字色）
                if let Some(label) = self.label {
                    wrap_slot(ctx, label_slot_modifier, |ctx| {
                        let mut style = crate::components::navigation_bar::NavigationBarDefaults::label_style();
                        style.color = Some(label_color);
                        crate::components::text::ProvideTextStyle(style, ctx, label);
                    });
                }
                // 4) 指示器 ripple（恒定全尺寸 leaf——悬浮/按压状态层载体）
                let ripple_key = ctx.next_key();
                ctx.start_leaf(ripple_key, ripple_modifier);
                ctx.end_node();
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

/// 单子节点包装（每个逻辑槽恰好贡献一个 arena 子节点——索引稳定）。
pub(crate) fn wrap_slot(ctx: &mut ComposeCtx, modifier: Modifier, content: impl FnOnce(&mut ComposeCtx)) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(Alignment::Center)) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

/// item 布局：复刻 androidx NavigationRailItemLayout 的
/// placeLabelAndIcon / placeIcon 数学。
///
/// 与导航栏 policy 的差异：
/// - 高基准 56dp（非 80）
/// - 指示器纵向内边距按 has_label 分支（4 vs 16——无标签为圆形胶囊）
/// - 无标签走 placeIcon 全居中路径
///
/// 子节点索引约定：children[0] = 彩色指示器胶囊，
/// children[1] = icon，children[2] = label（仅有 label 时存在），
/// 最后一个子节点 = indicator ripple（恒定全尺寸，z 序最上层）。
#[derive(Debug)]
struct NavigationRailItemLayoutPolicy {
    pub(crate) size_progress: State<f32>,
    pub(crate) always_show_label: bool,
    pub(crate) has_label: bool,
}

impl MeasurePolicy for NavigationRailItemLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 进度最先读（注册 layout_deps 到本节点）
        let size_p = self.size_progress.get().max(0.0);

        let min_h = NAVIGATION_RAIL_ITEM_HEIGHT.max(constraints.min_height);
        let loose = constraints.loosen();

        // icon 先测（loose）
        let (icon_size, _) = measure_node(nodes, policies, children[1], loose);

        let total_indicator_w = icon_size.width + INDICATOR_H_PADDING * 2.0;
        let animated_indicator_w = total_indicator_w * size_p;
        // 纵向内边距按有无 label 分支（androidx IndicatorVerticalPaddingWithLabel/NoLabel）
        let indicator_v_padding = if self.has_label {
            INDICATOR_V_PADDING_WITH_LABEL
        } else {
            INDICATOR_V_PADDING_NO_LABEL
        };
        let indicator_h = icon_size.height + indicator_v_padding * 2.0;
        // ripple：恒定全尺寸（Constraints.fixed(totalW, h)）
        let ripple_idx = children.len() - 1;
        let (ripple_size, _) = measure_node(
            nodes,
            policies,
            children[ripple_idx],
            Constraints::new(total_indicator_w, total_indicator_w, indicator_h, indicator_h),
        );
        // 彩色指示器 tight（animatedW x indicatorH）
        let (indicator_size, _) = measure_node(
            nodes,
            policies,
            children[0],
            Constraints::new(animated_indicator_w, animated_indicator_w, indicator_h, indicator_h),
        );

        let label_size = if self.has_label {
            let (size, _) = measure_node(nodes, policies, children[2], loose);
            Some(size)
        } else {
            None
        };

        // 容器宽：拥抱内容、至少 rail 宽 80（androidx widthIn(min=80)，
        // item 不横向填满——M3 规范视觉胶囊居中于 rail）
        let container_w = if constraints.max_width < f32::MAX {
            let content_w = icon_size.width
                .max(total_indicator_w)
                .max(label_size.as_ref().map(|l| l.width).unwrap_or(0.0));
            content_w.max(NAVIGATION_RAIL_WIDTH).min(constraints.max_width)
        } else {
            icon_size.width.max(total_indicator_w)
        };
        let max_h = if constraints.max_height.is_finite() { constraints.max_height } else { f32::MAX };

        let (height, selected_icon_y, unselected_icon_y, label_y) = if let Some(label) = &label_size {
            // placeLabelAndIcon：
            // contentHeight = iconH + VPadWithLabel + ItemVerticalPadding + labelH
            let content_h = icon_size.height
                + INDICATOR_V_PADDING_WITH_LABEL
                + ITEM_ICON_LABEL_GAP
                + label.height;
            let v_pad = ((min_h - content_h) / 2.0).max(INDICATOR_V_PADDING_WITH_LABEL);
            let height = (content_h + v_pad * 2.0).min(max_h);
            let selected_icon_y = v_pad;
            let unselected_icon_y = if self.always_show_label {
                selected_icon_y
            } else {
                (height - icon_size.height) / 2.0
            };
            let label_y = selected_icon_y
                + icon_size.height
                + INDICATOR_V_PADDING_WITH_LABEL
                + ITEM_ICON_LABEL_GAP;
            (height, selected_icon_y, unselected_icon_y, label_y)
        } else {
            // placeIcon：全部垂直居中于 constrain(ItemHeight=56)
            let height = min_h.min(max_h);
            let center_y = (height - icon_size.height) / 2.0;
            (height, center_y, center_y, 0.0)
        };

        // 位置插值：offset = iconDistance x (1 - progress)
        let offset = (unselected_icon_y - selected_icon_y) * (1.0 - size_p);
        let indicator_y = selected_icon_y - INDICATOR_V_PADDING_WITH_LABEL + offset;

        let mut placements = Vec::with_capacity(children.len());
        placements.push(Placement {
            size: indicator_size,
            position: Offset::new((container_w - indicator_size.width) / 2.0, indicator_y),
        });
        placements.push(Placement {
            size: icon_size,
            position: Offset::new((container_w - icon_size.width) / 2.0, selected_icon_y + offset),
        });
        if let Some(label) = label_size {
            placements.push(Placement {
                size: label,
                position: Offset::new((container_w - label.width) / 2.0, label_y + offset),
            });
        }
        // ripple：恒定全尺寸、跟随指示器动画位置，z 序最上层
        placements.push(Placement {
            size: ripple_size,
            position: Offset::new((container_w - ripple_size.width) / 2.0, indicator_y),
        });

        (Size::new(container_w, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}

