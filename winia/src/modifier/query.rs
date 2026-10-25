use super::*;
use super::param_eq::merge_graphics_params;

impl Modifier {
    /// 累积的四边 padding（start, top, end, bottom 各自独立）——
    /// 非对称 padding 的基础查询，其余查询方法由其派生。
    /// 动态值（State/闭包）在此求值（measure/layout 期间读 State 会
    /// 注册布局依赖——动画更新 State → 节点重测）
    /// 布局方向元素：返回自身覆盖的方向（无则 None——继承父作用域）
    pub fn get_layout_direction(&self) -> Option<crate::layout::LayoutDirection> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::LayoutDirection(d) => Some(*d),
            _ => None,
        })
    }

    /// This node's sibling paint order (`Modifier::z_index`); `None` means "tree order", which is what
    /// everything that does not set it gets.
    pub fn get_z_index(&self) -> Option<f32> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::ZIndex(z) => Some(*z),
            _ => None,
        })
    }

    pub fn get_padding_sides(&self) -> (f32, f32, f32, f32) {
        let resolve = |d: &Dimension| -> f32 { d.to_logical_px() };
        let (mut s, mut t, mut e, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for el in &self.elements {
            if let ModifierElement::PaddingSides { start, top, end, bottom } = el {
                s += resolve(start);
                t += resolve(top);
                e += resolve(end);
                b += resolve(bottom);
            }
        }
        (s, t, e, b)
    }

    /// 水平方向 padding——返回 (start, end)（各自独立，非对称时不同）
    /// The inset element at a chain index — either kind — for the walk's `SizeStep::InsetPadding` /
    /// `SizeStep::ConsumeInsets`. The controller is cloned out because the step list is `Copy`.
    pub(crate) fn inset_at(
        &self,
        index: usize,
    ) -> Option<(crate::window::insets::InsetsKind, crate::window::insets::WindowInsetsController)> {
        match self.elements.get(index)? {
            ModifierElement::WindowInsetsPadding { kind, controller }
            | ModifierElement::ConsumeWindowInsets { kind, controller } => Some((*kind, controller.clone())),
            _ => None,
        }
    }

    pub fn get_padding_horizontal(&self) -> (f32, f32) {
        let (s, _, e, _) = self.get_padding_sides();
        (s, e)
    }

    /// 垂直方向 padding——返回 (top, bottom)（各自独立，非对称时不同）
    pub fn get_padding_vertical(&self) -> (f32, f32) {
        let (_, t, _, b) = self.get_padding_sides();
        (t, b)
    }

    /// 累积 padding 总量——返回 (start+end, top+bottom)
    pub fn get_padding_values(&self) -> (f32, f32) {
        let (s, t, e, b) = self.get_padding_sides();
        (s + e, t + b)
    }

    /// 固定尺寸（从 Size modifier 提取）
    pub fn fixed_size(&self) -> Option<(Dimension, Dimension)> {
        for el in &self.elements {
            if let ModifierElement::Size { width: SizeValue::Length(w), height: SizeValue::Length(h) } = el {
                return Some((w.clone(), h.clone()));
            }
        }
        None
    }

    /// 解析 Size 元素的尺寸（静态/动态单轴独立解析）——返回 (width, height) 解析值，
    /// None 表示该轴不约束（Auto/Fill）。
    pub fn resolved_size(&self) -> Option<(Option<f32>, Option<f32>)> {
        use crate::unit::{Dp};
        use crate::runtime::density::current_density;
        // 合并所有 Size 元素（链序：后 push 的外层胜出——非 None 覆盖）。
        // ⚠ 不能只返回第一个：`width(300).height(dyn)` 是两个 Size 元素，
        // 只取第一个会丢 height（min_lines 动态高度失效的根因）
        let mut out: Option<(Option<f32>, Option<f32>)> = None;
        for el in &self.elements {
            if let ModifierElement::Size { width, height } = el {
                let resolve = |sv: &SizeValue| -> Option<f32> {
                    match sv {
                        SizeValue::Length(Dimension::Fixed(v)) | SizeValue::Length(Dimension::Dp(Dp(v))) => Some(*v),
                        SizeValue::Length(Dimension::Px(p)) => Some(p.to_logical(current_density())),
                        SizeValue::Auto => None,
                        SizeValue::Length(Dimension::Dynamic(f)) => Some(f()),
                        // An intrinsic request carries no number: it is resolved by the measure
                        // pipeline (`layout::node`), which fixes the axis to the content's
                        // measurement. Numeric consumers see it as "this axis says nothing".
                        SizeValue::Intrinsic(_) => None,
                    }
                };
                let (w, h) = (resolve(width), resolve(height));
                out = Some(match out {
                    Some((ow, oh)) => (w.or(ow), h.or(oh)),
                    None => (w, h),
                });
            }
        }
        out
    }

    /// 解析 MinWidth/MinHeight 元素——返回 (min_width, min_height)，None 表示
    /// 该轴无最小约束。动态值在布局期求值（State::get 注册 layout_dep——
    /// 动画可驱动 min 尺寸，只重测不重组）。
    pub fn min_size_constraint(&self) -> (Option<f32>, Option<f32>) {
        let resolve = |d: &Dimension| -> f32 { d.to_logical_px() };
        let mut out = (None, None);
        for el in &self.elements {
            match el {
                ModifierElement::MinWidth { value } => out.0 = Some(resolve(value)),
                ModifierElement::MinHeight { value } => out.1 = Some(resolve(value)),
                _ => {}
            }
        }
        out
    }

    /// The size-shaping elements in CHAIN ORDER: fixed sizes, `aspectRatio`, the min/max nodes, the
    /// range nodes (`widthIn`/`sizeIn`/`required*In`) and `wrapContent`.
    ///
    /// Compose nests one layout node per call and feeds each one the constraints the previous one
    /// produced, so every node's output range is a sub-range of its input's and the OUTER call
    /// decides a conflict: `.widthIn(max = 200.dp).widthIn(min = 300.dp)` measures 200, the reverse
    /// measures 300. [`Self::min_size_constraint`] and [`Self::max_size_constraint`] scan
    /// position-independently and collapse that away, so the measure pipeline replays the order with
    /// this instead. Dimensions are resolved here, during measure, so a `Dimension::Dynamic` registers
    /// its layout dependency and animates through layout.
    pub(crate) fn size_steps(&self) -> Vec<SizeStep> {
        let resolve = |d: &Dimension| -> f32 { d.to_logical_px() };
        let opt = |d: &Option<Dimension>| d.as_ref().map(resolve);
        let mut out = Vec::new();
        for (index, el) in self.elements.iter().enumerate() {
            match el {
                ModifierElement::Size { width, height } => {
                    let iw = matches!(width, SizeValue::Intrinsic(_));
                    let ih = matches!(height, SizeValue::Intrinsic(_));
                    if iw || ih {
                        out.push(SizeStep::Intrinsic { width: iw, height: ih });
                    }
                    let (w, h) = (width.to_logical_px(), height.to_logical_px());
                    if w.is_some() || h.is_some() {
                        out.push(SizeStep::Size { width: w, height: h });
                    }
                }
                ModifierElement::RequiredSize { width, height } => {
                    let iw = matches!(width.as_ref(), Some(SizeValue::Intrinsic(_)));
                    let ih = matches!(height.as_ref(), Some(SizeValue::Intrinsic(_)));
                    if iw || ih {
                        out.push(SizeStep::Intrinsic { width: iw, height: ih });
                    }
                    let w = width.as_ref().and_then(|v| v.to_logical_px());
                    let h = height.as_ref().and_then(|v| v.to_logical_px());
                    if w.is_some() || h.is_some() {
                        out.push(SizeStep::RequiredSize { width: w, height: h });
                    }
                }
                ModifierElement::FillMaxWidth { fraction } => out.push(SizeStep::Fill {
                    width: Some(*fraction),
                    height: None,
                }),
                ModifierElement::FillMaxHeight { fraction } => out.push(SizeStep::Fill {
                    width: None,
                    height: Some(*fraction),
                }),
                ModifierElement::FillMaxSize { fraction } => out.push(SizeStep::Fill {
                    width: Some(*fraction),
                    height: Some(*fraction),
                }),
                ModifierElement::MinWidth { value } => out.push(SizeStep::MinWidth(resolve(value))),
                ModifierElement::MaxWidth { value } => out.push(SizeStep::MaxWidth(resolve(value))),
                ModifierElement::MinHeight { value } => out.push(SizeStep::MinHeight(resolve(value))),
                ModifierElement::MaxHeight { value } => out.push(SizeStep::MaxHeight(resolve(value))),
                ModifierElement::SizeIn {
                    min_width,
                    min_height,
                    max_width,
                    max_height,
                    enforce_incoming,
                } => out.push(SizeStep::SizeIn {
                    min_width: opt(min_width),
                    min_height: opt(min_height),
                    max_width: opt(max_width),
                    max_height: opt(max_height),
                    enforce_incoming: *enforce_incoming,
                }),
                ModifierElement::DefaultMinSize { min_width, min_height } => {
                    out.push(SizeStep::DefaultMin {
                        min_width: opt(min_width),
                        min_height: opt(min_height),
                    })
                }
                // Insets are chain steps too: an inset has a position, and a size written inside
                // it must see the deflated range (the reviewer's "one place the two kinds of padding
                // differ"). The walk resolves the value; this only carries the element index.
                ModifierElement::WindowInsetsPadding { .. } => out.push(SizeStep::InsetPadding(index)),
                ModifierElement::ConsumeWindowInsets { .. } => out.push(SizeStep::ConsumeInsets(index)),
                ModifierElement::PaddingSides { start, top, end, bottom } => {
                    out.push(SizeStep::Padding {
                        start: resolve(start),
                        top: resolve(top),
                        end: resolve(end),
                        bottom: resolve(bottom),
                    })
                }
                ModifierElement::AspectRatio { ratio, match_height_first } => {
                    out.push(SizeStep::AspectRatio { ratio: *ratio, match_height_first: *match_height_first })
                }
                ModifierElement::WrapContent {
                    width,
                    height,
                    align,
                    unbounded,
                } => out.push(SizeStep::Wrap {
                    width: *width,
                    height: *height,
                    align: *align,
                    unbounded: *unbounded,
                }),
                _ => {}
            }
        }
        out
    }

    /// Resolve the MaxWidth/MaxHeight elements — `(max_width, max_height)`, `None` for an
    /// axis with no cap. Dynamic values are evaluated during layout (a `State::get` registers
    /// a layout dependency, so an animation can drive the cap without recomposing).
    pub fn max_size_constraint(&self) -> (Option<f32>, Option<f32>) {
        let resolve = |d: &Dimension| -> f32 { d.to_logical_px() };
        let mut out = (None, None);
        for el in &self.elements {
            match el {
                ModifierElement::MaxWidth { value } => out.0 = Some(resolve(value)),
                ModifierElement::MaxHeight { value } => out.1 = Some(resolve(value)),
                _ => {}
            }
        }
        out
    }

    /// 是否填满最大宽度
    pub fn is_fill_max_width(&self) -> bool {
        self.elements.iter().any(|el| matches!(el,
            ModifierElement::FillMaxWidth { .. } | ModifierElement::FillMaxSize { .. }
        ))
    }

    /// 是否填满最大高度
    pub fn is_fill_max_height(&self) -> bool {
        self.elements.iter().any(|el| matches!(el,
            ModifierElement::FillMaxHeight { .. } | ModifierElement::FillMaxSize { .. }
        ))
    }


    /// 垂直滚动状态（如果有 VerticalScroll modifier）
    /// The orientation declared by `clipScrollableContainer`, if the chain carries it.
    pub fn clip_scrollable_vertical(&self) -> Option<bool> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::ClipScrollableContainer { vertical } => Some(*vertical),
            _ => None,
        })
    }

    /// The overscroll effect attached to this chain, if any.
    pub fn overscroll_effect(&self) -> Option<&crate::overscroll::OverscrollEffect> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::Overscroll { effect } => Some(effect),
            _ => None,
        })
    }

    /// Whether this item carries the animation contract (`Modifier::animate_item`).
    /// Item-aware layouts (lazy lists and flows) honor it directly.
    pub fn has_animate_item(&self) -> bool {
        self.animate_item_spec().is_some()
    }

    /// The container's item-animation host, when this modifier belongs to an item-aware layout.
    pub(crate) fn item_animation_host(
        &self,
    ) -> Option<&crate::layout::item_animation::ItemAnimationHost> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::ItemAnimationHost(host) => Some(host),
            _ => None,
        })
    }

    /// The item placement/fade contract attached with `Modifier::animate_item`.
    pub(crate) fn animate_item_spec(
        &self,
    ) -> Option<crate::animation::item::ItemAnimationSpec> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::AnimateItem(spec) => Some(spec.clone()),
            _ => None,
        })
    }

    pub fn vertical_scroll_state(&self) -> Option<&ScrollState> {
        for el in &self.elements {
            if let ModifierElement::VerticalScroll { state } = el {
                return Some(state);
            }
        }
        None
    }

    /// lazy 列表内容高度 State（如果有 LazyScroll modifier）
    pub fn lazy_scroll_content_height(&self) -> Option<&crate::runtime::state::Backchannel<f32>> {
        for el in &self.elements {
            if let ModifierElement::LazyScroll { content_height, .. } = el {
                return Some(content_height);
            }
        }
        None
    }

    /// 水平滚动状态（如果有 HorizontalScroll modifier）
    pub fn horizontal_scroll_state(&self) -> Option<&ScrollState> {
        for el in &self.elements {
            if let ModifierElement::HorizontalScroll { state, .. } = el {
                return Some(state);
            }
        }
        None
    }

    /// 水平滚动是否反向（None = 无横向滚动容器）——scrollbar 横向 reverse
    /// 镜像用（P2-3：offset 语义镜像，条位置须跟内容走）。
    pub fn is_horizontal_scroll_reversed(&self) -> Option<bool> {
        for el in &self.elements {
            if let ModifierElement::HorizontalScroll { reverse, .. } = el {
                return Some(*reverse);
            }
        }
        None
    }

    /// 当前节点上的嵌套滚动连接。
    pub fn nested_scroll_connection(&self) -> Option<Arc<dyn crate::nested_scroll::NestedScrollConnection>> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::NestedScroll { connection } => Some(connection.clone()),
            _ => None,
        })
    }

    /// 布局权重（供 Column/Row 使用）
    pub fn get_layout_weight(&self) -> Option<f32> {
        for el in &self.elements {
            if let ModifierElement::LayoutWeight { weight, .. } = el {
                return Some(*weight);
            }
        }
        None
    }

    /// Whether a weighted node fills its share. `None` when the node carries no weight; a node
    /// with a weight but no explicit `fill` reads as `true`, the `Modifier::layout_weight` default.
    pub fn get_layout_weight_fill(&self) -> Option<bool> {
        for el in &self.elements {
            if let ModifierElement::LayoutWeight { fill, .. } = el {
                return Some(*fill);
            }
        }
        None
    }

    /// 测试标记（供调试树/UI 测试定位）
    pub fn get_test_tag(&self) -> Option<&str> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::TestTag { tag } = el {
                Some(tag.as_str())
            } else {
                None
            }
        })
    }

    /// The aspect-ratio element, if any: `(ratio, match_height_first)`.
    ///
    /// The MEASURE path consumes it through `SizeStep::AspectRatio`, in chain order; this getter
    /// exists for the intrinsic path, where the ratio answers from the OTHER axis the way Compose's
    /// `AspectRatioNode` does.
    pub fn aspect_ratio_constraint(&self) -> Option<(f32, bool)> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::AspectRatio { ratio, match_height_first } => Some((*ratio, *match_height_first)),
            _ => None,
        })
    }

    /// 强制尺寸（单轴 None = 未约束）
    ///
    /// Only numeric axes are reported here; an intrinsic request
    /// (`requiredWidth(IntrinsicSize::Max)`) carries no number and is handled by the measure
    /// pipeline together with `Modifier::width/height(IntrinsicSize)` — see
    /// [`Modifier::intrinsic_width_request`].
    pub fn required_size_constraint(&self) -> Option<(Option<f32>, Option<f32>)> {
        // A required size is a SIZE: `requiredWidth(IntrinsicSize)` is a real Compose overload, so
        // the layout modes stay, and only a numeric axis is reported here.
        self.elements.iter().find_map(|el| {
            if let ModifierElement::RequiredSize { width, height } = el {
                Some((
                    width.as_ref().and_then(|v| v.to_logical_px()),
                    height.as_ref().and_then(|v| v.to_logical_px()),
                ))
            } else {
                None
            }
        })
    }

    /// The width axis's intrinsic request, if this chain has one — Compose's
    /// `Modifier.width(IntrinsicSize)` / `requiredWidth(IntrinsicSize)`.
    ///
    /// Returns `(which measurement, enforce_incoming)`. `enforce_incoming` is `true` for the
    /// `width`/`height` form (the incoming constraints may override the result, Compose's
    /// `SizeNode(enforceIncoming = true)`) and `false` for the `required*` form, which ignores
    /// them. Numeric `size`/`sizeIn` modifiers win over it: the measure pipeline applies them
    /// first, and a fixed axis needs no intrinsic measurement.
    pub fn intrinsic_width_request(&self) -> Option<(IntrinsicSize, bool)> {
        self.intrinsic_request(true)
    }

    /// The height axis's intrinsic request — see [`Modifier::intrinsic_width_request`].
    pub fn intrinsic_height_request(&self) -> Option<(IntrinsicSize, bool)> {
        self.intrinsic_request(false)
    }

    fn intrinsic_request(&self, horizontal: bool) -> Option<(IntrinsicSize, bool)> {
        let pick = |sv: &SizeValue| match sv {
            SizeValue::Intrinsic(s) => Some(*s),
            _ => None,
        };
        for el in &self.elements {
            let found = match el {
                ModifierElement::Size { width, height } => {
                    pick(if horizontal { width } else { height }).map(|s| (s, true))
                }
                ModifierElement::RequiredSize { width, height } => {
                    let axis = if horizontal { width } else { height };
                    axis.as_ref().and_then(pick).map(|s| (s, false))
                }
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// The alignment line this child asks to be aligned by, if any (Compose's `alignBy`).
    pub fn get_align_by(&self) -> Option<crate::layout::AlignmentLine> {
        for el in &self.elements {
            if let ModifierElement::AlignBy { line } = el {
                return Some(*line);
            }
        }
        None
    }

    /// The FlowRow fill fraction on this child, if any (Compose's `fillMaxRowHeight`).
    pub fn get_fill_max_row_height(&self) -> Option<f32> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::FillMaxRowHeight { fraction } => Some(*fraction),
            _ => None,
        })
    }

    /// The FlowColumn fill fraction on this child, if any (Compose's `fillMaxColumnWidth`).
    pub fn get_fill_max_column_width(&self) -> Option<f32> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::FillMaxColumnWidth { fraction } => Some(*fraction),
            _ => None,
        })
    }

    /// Every `paddingFrom` on this modifier, in chain order — one entry per element, because
    /// `padding_from_baseline(top, bottom)` is two of them and both are in force (Compose chains the
    /// same way, `foundation/layout/AlignmentLine.kt:144`).
    ///
    /// The lines this node reports itself, each with the closure that positions it from the measured
    /// size — [`Modifier::alignment_line`]. Read after the content is measured, since that is when a
    /// closure over the size can run.
    pub fn get_alignment_line_values(
        &self,
    ) -> Vec<(
        crate::layout::AlignmentLine,
        std::sync::Arc<dyn Fn(crate::unit::Size) -> f32 + Send + Sync>,
    )> {
        self.elements
            .iter()
            .filter_map(|el| match el {
                ModifierElement::AlignmentLineValue { line, value } => Some((*line, value.clone())),
                _ => None,
            })
            .collect()
    }

    /// The touch-target minimum this node asks for, if any — the LAST one wins, as a chain of
    /// modifiers would leave the outermost in charge. `Some(0.0)` means "explicitly off".
    pub fn get_minimum_interactive_size(&self) -> Option<f32> {
        self.elements.iter().rev().find_map(|el| match el {
            ModifierElement::MinimumInteractiveSize { size } => Some(*size),
            _ => None,
        })
    }

    /// Applied after the content is measured: it is the content's own alignment line that says how
    /// much padding the node needs.
    pub fn get_padding_from(&self) -> Vec<(crate::layout::AlignmentLine, Option<f32>, Option<f32>)> {
        self.elements
            .iter()
            .filter_map(|el| match el {
                ModifierElement::PaddingFrom { line, before, after } => Some((*line, *before, *after)),
                _ => None,
            })
            .collect()
    }

    /// 交叉轴对齐覆盖（供 Column/Row 使用）
    ///
    /// The LAST element of this kind wins, like every other chain query (a modifier chain reads as
    /// "later overrides earlier"); returning the first made `.align_self(a).align_self(b)` resolve
    /// to `a`.
    pub fn get_align_self(&self) -> Option<crate::layout::Alignment> {
        self.elements.iter().rev().find_map(|el| match el {
            ModifierElement::AlignSelf { alignment } => Some(*alignment),
            _ => None,
        })
    }

    /// Whether this child asked to match its box's size (Compose's `matchParentSize`). Read by
    /// `Box`/`Stack`, which excludes it from the size computation and measures it at the box size.
    pub fn is_match_parent_size(&self) -> bool {
        self.elements
            .iter()
            .any(|el| matches!(el, ModifierElement::MatchParentSize))
    }

    /// Both-axis alignment override, read by `Box`/`Stack` (Compose's `BoxScope.align`). Last wins, as
    /// above.
    pub fn get_content_align_self(&self) -> Option<crate::layout::ContentAlignment> {
        self.elements.iter().rev().find_map(|el| match el {
            ModifierElement::ContentAlignSelf { alignment } => Some(*alignment),
            _ => None,
        })
    }

    /// 焦点请求器 ID
    pub fn focus_requester_id(&self) -> Option<u64> {
        for el in &self.elements {
            if let ModifierElement::FocusRequesterId { id } = el {
                return Some(*id);
            }
        }
        None
    }

    /// 点击回调（如果有 Clickable modifier；无枚举时回退到 ClickNode 首个）。
    /// 优先级：枚举 Clickable > node Click（旧行为优先，保证双轨迁移期稳定）。
    pub fn on_click(&self) -> Option<&Arc<dyn Fn() + Send + Sync>> {
        for el in &self.elements {
            if let ModifierElement::Clickable { on_click, .. } = el {
                return Some(on_click);
            }
        }
        None
    }

    /// 开放节点点击回调（含回调体——node 是 trait object，返回 Arc 供分发调用）。
    /// 单首语义（见 ClickNode 文档）：仅首个 ClickNode 生效。
    pub(crate) fn node_click(&self) -> Option<std::sync::Arc<dyn ClickNode>> {
        self.node_clicks().next()
    }

    /// 全部 ClickNode（单首语义下仅首个生效；保留迭代器供未来多播/诊断）。
    pub(crate) fn node_clicks(&self) -> impl Iterator<Item = std::sync::Arc<dyn ClickNode>> + '_ {
        self.nodes.iter().filter_map(|n| match n {
            ModifierNode::Click(cb) => Some(cb.clone()),
            _ => None,
        })
    }

    /// 开放节点点击绑定的交互源（press 波纹用）。
    pub fn node_click_interaction(&self) -> Option<MutableInteractionSource> {
        self.node_click().and_then(|n| n.interaction())
    }

    /// Clickable 绑定的交互源（无则 None）
    pub fn clickable_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Clickable { interaction: Some(s), .. } = el {
                Some(s)
            } else {
                None
            }
        })
    }

    /// Focusable 绑定的交互源（无则 None）
    pub fn focusable_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Focusable { interaction: Some(s) } = el {
                Some(s)
            } else {
                None
            }
        })
    }

    /// Hoverable 绑定的交互源
    pub fn hoverable_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Hoverable { interaction } = el {
                Some(interaction)
            } else {
                None
            }
        })
    }

    /// Ripple 绑定的交互源（首个 Ripple 元素——Switch 的 Handle 容器等
    /// ripple 与 clickable 不同节点时，按压坐标要换算到该节点本地空间）
    pub fn ripple_interaction(&self) -> Option<&MutableInteractionSource> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::Ripple { source, .. } = el {
                Some(source)
            } else {
                None
            }
        })
    }

    /// 是否声明了悬停交互（hover 路由判定）
    pub fn has_hoverable(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::Hoverable { .. }))
    }

    /// 是否声明了任意手势回调（tap/drag 系列）——app.rs 手势路由判定用。
    /// 只认 TapOn/DragOn 枚举（PointerNode 不参与——见 PointerNode 文档的
    /// P0-1 教训：装饰性 PointerNode 纳入路由会静默偷走外层 tap）。
    pub fn has_gesture(&self) -> bool {
        self.elements.iter().any(|el| matches!(
            el,
            ModifierElement::TapOnTap { .. }
                | ModifierElement::TapOnDoubleTap { .. }
                | ModifierElement::TapOnLongPress { .. }
                | ModifierElement::TapOnPress { .. }
                | ModifierElement::DragOnStart { .. }
                | ModifierElement::DragOnMove { .. }
                | ModifierElement::DragOnEnd { .. }
                | ModifierElement::DragOnCancel { .. }
        ))
    }

    /// 是否声明了拖拽回调（决定 slop 后走 drag 还是取消 tap）
    pub fn has_drag_gesture(&self) -> bool {
        self.elements.iter().any(|el| matches!(
            el,
            ModifierElement::DragOnStart { .. }
                | ModifierElement::DragOnMove { .. }
                | ModifierElement::DragOnEnd { .. }
                | ModifierElement::DragOnCancel { .. }
        ))
    }

    /// 是否声明了双击回调——决定 onTap 是否延迟到双击窗口结束
    /// （Compose detectTapGestures：onDoubleTap 存在时 onTap 延迟触发，
    /// 窗口内第二次按下同节点 → 取消；超时 → 补发）
    pub fn has_double_tap(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::TapOnDoubleTap { .. }))
    }

    pub fn graphics_layer_params(&self) -> Option<GraphicsLayerParams> {
        let mut merged = None;
        for el in self.elements.iter() {
            let ModifierElement::GraphicsLayer { params } = el else { continue };
            let next = params.eval();
            if let Some(current) = &mut merged {
                merge_graphics_params(current, next);
            } else {
                merged = Some(next);
            }
        }
        merged
    }

    /// 背景模糊半径（渲染期在节点内容绘制前即时处理）
    pub fn backdrop_blur_radius(&self) -> Option<f32> {
        self.elements.iter().find_map(|el| {
            if let ModifierElement::BackdropBlur { radius } = el { Some(radius.eval()) } else { None }
        })
    }

    /// 获取文本对齐方式
    pub fn align(&self) -> Option<crate::text::TextAlign> {
        for el in &self.elements {
            if let ModifierElement::TextContent { align, .. } = el {
                return Some(*align);
            }
        }
        None
    }

    /// 获取文本内容字节长度（Text / RichText）
    pub fn content_len(&self) -> usize {
        for el in &self.elements {
            if let ModifierElement::TextContent { content, .. } = el {
                return content.len();
            }
            if let ModifierElement::RichTextContent { content, .. } = el {
                return content.len();
            }
        }
        0
    }

    /// 位置偏移（如果有 Offset modifier）
    /// 普通偏移（RTL 下 x 镜像）——动态值在此求值（measure 期读 State 注册布局依赖）
    pub fn get_offset(&self) -> Option<(f32, f32)> {
        let resolve = |d: &Dimension| -> f32 { d.to_logical_px() };
        for el in &self.elements {
            if let ModifierElement::Offset { x, y } = el {
                return Some((resolve(x), resolve(y)));
            }
        }
        None
    }

    /// 绝对偏移（RTL 下不镜像）——动态值求值同 `get_offset`
    pub fn get_absolute_offset(&self) -> Option<(f32, f32)> {
        let resolve = |d: &Dimension| -> f32 { d.to_logical_px() };
        for el in &self.elements {
            if let ModifierElement::AbsoluteOffset { x, y } = el {
                return Some((resolve(x), resolve(y)));
            }
        }
        None
    }
}

// ── 辅助方法: 分类提取 ──

impl Modifier {
    /// Compose 阶段自动注册所有 ModifierElement 中引用的 State 依赖。
    /// 新增包含 State<T> 的 ModifierElement 变体时，必须在此方法中加对应分支。
    pub(crate) fn register_state_deps(&self) {
        for el in &self.elements {
            match el {
                ModifierElement::VerticalScroll { state } => { let _ = state.offset.get(); }
                ModifierElement::HorizontalScroll { state, .. } => { let _ = state.offset.get(); }
                _ => {}
            }
        }
    }
}

