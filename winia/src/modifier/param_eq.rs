use super::*;

// ── 参数相等性（Skip 判定） ──

impl Modifier {
    /// 参数相等性（start_restartable_group 的 Skip 判定用）——可比较元素
    /// （数值/枚举/字符串/颜色）精确比较；**闭包类元素**（背景色/点击回调/
    /// 图形层动态参数/滚动状态/富文本样式）**视为相同**——每次 build 重建的
    /// 闭包无法比较，精确比会破坏 Skip（列表每行永不 Skip）。
    ///
    /// 近似语义：modifier 的**数值参数**变化（width/padding/颜色等）→ 不等 →
    /// Enter（content 重跑）；闭包参数变化（回调重设）→ 相同 → Skip（保持）。
    /// 注意：动画驱动的动态尺寸（Dimension::Dynamic）视为相同——布局期
    /// layout_dep 每帧重测已覆盖，无需 Enter。
    pub(crate) fn param_eq(&self, other: &Modifier) -> bool {
        if self.elements.len() != other.elements.len() {
            return false;
        }
        // Node 轨道：按 node_key 序列比较（参数变化 → key 变 → Enter）。
        if self.nodes.len() != other.nodes.len() {
            return false;
        }
        if self
            .nodes
            .iter()
            .zip(&other.nodes)
            .any(|(a, b)| node_key_of(a) != node_key_of(b))
        {
            return false;
        }
        self.elements
            .iter()
            .zip(&other.elements)
            .all(|(a, b)| element_param_eq(a, b))
    }
}

fn element_param_eq(a: &ModifierElement, b: &ModifierElement) -> bool {
    use ModifierElement::*;
    match (a, b) {
        (Size { width: aw, height: ah }, Size { width: bw, height: bh }) => {
            size_value_eq(aw, bw) && size_value_eq(ah, bh)
        }
        (MinWidth { value: av }, MinWidth { value: bv }) => dimension_eq(av, bv),
        (MinHeight { value: av }, MinHeight { value: bv }) => dimension_eq(av, bv),
        (MaxWidth { value: av }, MaxWidth { value: bv }) => dimension_eq(av, bv),
        (MaxHeight { value: av }, MaxHeight { value: bv }) => dimension_eq(av, bv),
        (
            WindowInsetsPadding { kind: ak, controller: ac },
            WindowInsetsPadding { kind: bk, controller: bc },
        ) => ak == bk && ac.same(bc),
        (
            ConsumeWindowInsets { kind: ak, controller: ac },
            ConsumeWindowInsets { kind: bk, controller: bc },
        ) => ak == bk && ac.same(bc),
        (
            PaddingSides { start: as_, top: at, end: ae, bottom: ab },
            PaddingSides { start: bs, top: bt, end: be, bottom: bb },
        ) => {
            dimension_eq(as_, bs) && dimension_eq(at, bt) && dimension_eq(ae, be) && dimension_eq(ab, bb)
        }
        (FillMaxWidth { fraction: af }, FillMaxWidth { fraction: bf }) => af == bf,
        (FillMaxHeight { fraction: af }, FillMaxHeight { fraction: bf }) => af == bf,
        (FillMaxSize { fraction: af }, FillMaxSize { fraction: bf }) => af == bf,
        (
            SizeIn { min_width: amw, min_height: amh, max_width: axw, max_height: axh, enforce_incoming: ae },
            SizeIn { min_width: bmw, min_height: bmh, max_width: bxw, max_height: bxh, enforce_incoming: be },
        ) => {
            opt_dimension_eq(amw, bmw)
                && opt_dimension_eq(amh, bmh)
                && opt_dimension_eq(axw, bxw)
                && opt_dimension_eq(axh, bxh)
                && ae == be
        }
        (DefaultMinSize { min_width: amw, min_height: amh }, DefaultMinSize { min_width: bmw, min_height: bmh }) => {
            opt_dimension_eq(amw, bmw) && opt_dimension_eq(amh, bmh)
        }
        (
            WrapContent { width: aw, height: ah, align: aa, unbounded: au },
            WrapContent { width: bw, height: bh, align: ba, unbounded: bu },
        ) => aw == bw && ah == bh && aa == ba && au == bu,
        (Offset { x: ax, y: ay }, Offset { x: bx, y: by }) => dimension_eq(ax, bx) && dimension_eq(ay, by),
        (AbsoluteOffset { x: ax, y: ay }, AbsoluteOffset { x: bx, y: by }) => dimension_eq(ax, bx) && dimension_eq(ay, by),
        (AlignSelf { alignment: aa }, AlignSelf { alignment: ba }) => aa == ba,
        (ContentAlignSelf { alignment: aa }, ContentAlignSelf { alignment: ba }) => aa == ba,
        (MatchParentSize, MatchParentSize) => true,
        (Visible { visible: a }, Visible { visible: b }) => a == b,
        (
            GridItem { column_span: ac, row_span: ar, alignment: aa },
            GridItem { column_span: bc, row_span: br, alignment: ba },
        ) => ac == bc && ar == br && aa == ba,
        (AlignBy { line: a }, AlignBy { line: b }) => a == b,
        (FillMaxRowHeight { fraction: a }, FillMaxRowHeight { fraction: b }) => a == b,
        (FillMaxColumnWidth { fraction: a }, FillMaxColumnWidth { fraction: b }) => a == b,
        (LayoutWeight { weight: aw, fill: af }, LayoutWeight { weight: bw, fill: bf }) => aw == bw && af == bf,
        (AspectRatio { ratio: ar, match_height_first: am }, AspectRatio { ratio: br, match_height_first: bm }) => {
            ar == br && am == bm
        }
        (RequiredSize { width: aw, height: ah }, RequiredSize { width: bw, height: bh }) => {
            opt_size_value_eq(aw, bw) && opt_size_value_eq(ah, bh)
        }
        (TestTag { tag: at }, TestTag { tag: bt }) => at == bt,
        // Same rule as the colour background: a dynamic brush is a parameter, a changed STATIC brush
        // re-enters (otherwise the node keeps painting the old gradient), and the shape is part of it.
        (BackgroundBrush { brush: abrush, shape: ashape }, BackgroundBrush { brush: bbrush, shape: bshape }) => {
            abrush.same_parameter(bbrush) && ashape.same_parameter(bshape)
        }
        // Semantics carries visible state (a tab's selection): a change must re-enter the node.
        (Semantics(a), Semantics(b)) => a == b,
        (ClearAndSetSemantics(a), ClearAndSetSemantics(b)) => a == b,
        // A focus group is a flag; the changed-focus callback is rebuilt every composition (same rule
        // as the other observation callbacks).
        (FocusGroup, FocusGroup) => true,
        (FocusProperties { can_focus: a }, FocusProperties { can_focus: b }) => a == b,
        (OnFocusChanged { .. }, OnFocusChanged { .. }) => true,
        (LayoutDirection(ad), LayoutDirection(bd)) => ad == bd,
        (ZIndex(az), ZIndex(bz)) => az == bz,
        // Shared-element marker: only scope + key + kind decide Skip. `z_index`
        // is re-read from the arena marker every poll, and everything else
        // (bounds/transform/render_in_overlay) either rides flight progress or
        // is sampled at resolve — none of them forces Enter.
        (
            SharedTransition { scope_id: a_id, key: a_key, kind: a_kind, .. },
            SharedTransition { scope_id: b_id, key: b_key, kind: b_kind, .. },
        ) => a_id == b_id && a_key == b_key && a_kind == b_kind,
        // Overlay marker: scope or z changing must rebuild the node, because the
        // coordinator reads the value from this modifier in the arena.
        (
            SharedScopeOverlay { scope_id: a_id, z_index: a_z },
            SharedScopeOverlay { scope_id: b_id, z_index: b_z },
        ) => a_id == b_id && a_z == b_z,
        (Shadow { params: ap, shape: as_, clip: ac }, Shadow { params: bp, shape: bs, clip: bc }) => {
            ap.same_parameter(bp) && as_.same_parameter(bs) && ac == bc
        }
        // A dynamic colour is a parameter (it is read at paint time); a static one that CHANGES has to
        // re-enter, or the node keeps painting the old colour.
        (Background { color: ac, shape: as_ }, Background { color: bc, shape: bs }) => {
            ac.same_parameter(bc) && as_.same_parameter(bs)
        }
        // The colour is a carrier too: a changed STATIC colour re-enters, a dynamic one is a
        // parameter read at paint.
        (Border { width: aw, color: ac, shape: as_ }, Border { width: bw, color: bc, shape: bs }) => {
            dimension_eq(aw, bw) && ac.same_parameter(bc) && as_.same_parameter(bs)
        }
        (Clip { shape: as_ }, Clip { shape: bs }) => as_.same_parameter(bs),
        (Blur { radius: ar }, Blur { radius: br }) => ar.same_parameter(br),
        (BackdropBlur { radius: ar }, BackdropBlur { radius: br }) => ar.same_parameter(br),
        (TextContent { content: ac, font_size: af, color: acol, font_weight: afw, font_style: afs, max_lines: am, align: aa, overflow: ao, soft_wrap: asw, letter_spacing: als, line_height: alh },
         TextContent { content: bc, font_size: bf, color: bcol, font_weight: bfw, font_style: bfs, max_lines: bm, align: ba, overflow: bo, soft_wrap: bsw, letter_spacing: bls, line_height: blh }) => {
            ac == bc && af == bf && acol == bcol && afw == bfw && afs == bfs && am == bm && aa == ba && ao == bo && asw == bsw && als == bls && alh == blh
        }
        // 富文本：内容 + 内联元素数比较；样式范围视为相同（每次 build 重建）
        (RichTextContent { content: ac, drawables: ad, drawable_ranges: ar, .. },
         RichTextContent { content: bc, drawables: bd, drawable_ranges: br, .. }) => {
            ac == bc && ad.len() == bd.len() && ar == br
        }
        // 点击：交互源身份参与判定（换源需重入 content 捕获新源）；回调视为相同
        (Clickable { interaction: ai, .. }, Clickable { interaction: bi, .. }) => ai == bi,
        (TapOnTap { .. }, TapOnTap { .. }) => true,
        (TapOnDoubleTap { .. }, TapOnDoubleTap { .. }) => true,
        (TapOnLongPress { .. }, TapOnLongPress { .. }) => true,
        (TapOnPress { .. }, TapOnPress { .. }) => true,
        (DragOnStart { .. }, DragOnStart { .. }) => true,
        (DragOnMove { .. }, DragOnMove { .. }) => true,
        (DragOnEnd { .. }, DragOnEnd { .. }) => true,
        (DragOnCancel { .. }, DragOnCancel { .. }) => true,
        (Focusable { interaction: ai }, Focusable { interaction: bi }) => ai == bi,
        (Hoverable { interaction: ai }, Hoverable { interaction: bi }) => ai == bi,
        (
            PointerHoverIcon { icon: ai, override_descendants: ao },
            PointerHoverIcon { icon: bi, override_descendants: bo },
        ) => ai == bi && ao == bo,
        (
            Ripple { source: as_, color: ac, bounded: abc, shape: ash, state_layer_diameter: asl },
            Ripple { source: bs, color: bc, bounded: bbc, shape: bsh, state_layer_diameter: bsl },
        ) => {
            as_ == bs
                && ac.same_parameter(bc)
                && abc == bbc
                && crate::graphics::option_same_parameter(ash, bsh)
                && crate::graphics::option_same_parameter(asl, bsl)
        }
        (DrawIcon { spec: a }, DrawIcon { spec: b }) => a == b,
        (ImageContent { source: a, content_scale: as_, alignment: aa, alpha: aal, color_filter: acf, filter_quality: afq },
         ImageContent { source: b, content_scale: bs, alignment: ba, alpha: bal, color_filter: bcf, filter_quality: bfq }) => {
            a == b
                && as_ == bs
                && aa == ba
                && aal.same_parameter(bal)
                && crate::graphics::option_same_parameter(acf, bcf)
                && afq == bfq
        }
        (FocusRequesterId { id: ai }, FocusRequesterId { id: bi }) => ai == bi,
        (KbEvent { .. }, KbEvent { .. }) => true,
        (PointerEvent { .. }, PointerEvent { .. }) => true,
        (VerticalScroll { state: as_ }, VerticalScroll { state: bs }) => as_.offset.state_id() == bs.offset.state_id(),
        (LazyScroll { content_height: ac, reverse: ar }, LazyScroll { content_height: bc, reverse: br }) => {
            ac.state_id() == bc.state_id() && ar == br
        }
        (HorizontalScroll { state: as_, reverse: ar }, HorizontalScroll { state: bs, reverse: br }) => as_.offset.state_id() == bs.offset.state_id() && ar == br,
        (NestedScroll { .. }, NestedScroll { .. }) => true,
        // Compared by VALUE, not by Debug: a caller may rebuild the spec (fresh Arc<dyn Interpolator>)
        // every frame, and the comparison runs once per item per frame.
        (AnimateItem(as_), AnimateItem(bs)) => as_.same_parameter(bs),
        (ItemAnimationHost(_), ItemAnimationHost(_)) => true,
        (
            InnerShadow { shape: as_, color: ac, blur: ab, offset_x: ax, offset_y: ay },
            InnerShadow { shape: bs, color: bc, blur: bb, offset_x: bx, offset_y: by },
        ) => as_.same_parameter(bs) && ac == bc && ab == bb && ax == bx && ay == by,
        (
            ClipScrollableContainer { vertical: a },
            ClipScrollableContainer { vertical: b },
        ) => a == b,
        (Overscroll { effect: a }, Overscroll { effect: b }) => a.same(b),
        // Observation callbacks are rebuilt every composition; identity is the parameter (same rule
        // as Clickable's callback). Without these arms a chain carrying one could never Skip.
        (OnSizeChanged { .. }, OnSizeChanged { .. }) => true,
        (OnGloballyPositioned { .. }, OnGloballyPositioned { .. }) => true,
        (OnPlaced { .. }, OnPlaced { .. }) => true,
        (LayoutId { id: a }, LayoutId { id: b }) => a == b,
        (
            BringIntoViewRequester { requester: a },
            BringIntoViewRequester { requester: b },
        ) => a.same(b),
        // Scene tag: an equal id means the tag did not change. Without this arm the match falls through
        // to `_ => false`, so any modifier chain carrying a `SceneTag` could NEVER Skip — measured
        // consequence: the nav's transition-layer wrapper (which carries one) re-composed and re-measured
        // both scenes' whole content on every visit instead of restoring the cached subtree.
        //
        // A scene host must therefore put everything its wrapper's content closes over into that
        // wrapper's identity (the nav keys each layer by scene key + role + whether a transition is
        // running): a Skip reuses the previous frame's closures, and the tag alone cannot see them.
        (SceneTag { id: a }, SceneTag { id: b }) => a == b,
        // 图形层动态参数视为相同（渲染期求值——动画不触发 Enter）
        // Same rule as the background: static parameters compare by value, dynamic ones are the same
        // request, and a static set that changes re-enters.
        (GraphicsLayer { params: ap }, GraphicsLayer { params: bp }) => ap.same_parameter(bp),
        _ => false,
    }
}

pub(super) fn merge_graphics_params(current: &mut GraphicsLayerParams, next: GraphicsLayerParams) {
    current.scale_x *= next.scale_x;
    current.scale_y *= next.scale_y;
    current.alpha *= next.alpha;
    current.translation_x += next.translation_x;
    current.translation_y += next.translation_y;
    current.rotation_z += next.rotation_z;
    current.rotation_x += next.rotation_x;
    current.rotation_y += next.rotation_y;
    if next.camera_distance != GraphicsLayerParams::default().camera_distance {
        current.camera_distance = next.camera_distance;
    }
    if next.transform_origin != TransformOrigin::CENTER {
        current.transform_origin = next.transform_origin;
    }
    current.clip |= next.clip;
    if next.shadow_elevation > 0.0 {
        current.shadow_elevation = next.shadow_elevation;
    }
    if next.shadow_shape.is_some() {
        current.shadow_shape = next.shadow_shape;
    }
    if next.ambient_shadow_color != DEFAULT_AMBIENT_SHADOW_COLOR {
        current.ambient_shadow_color = next.ambient_shadow_color;
    }
    if next.spot_shadow_color != DEFAULT_SPOT_SHADOW_COLOR {
        current.spot_shadow_color = next.spot_shadow_color;
    }
    if next.color_filter.is_some() {
        current.color_filter = next.color_filter;
    }
}

/// Compare two optional size specs: both absent, or both present and equal by [`size_value_eq`].
fn opt_size_value_eq(a: &Option<SizeValue>, b: &Option<SizeValue>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => size_value_eq(a, b),
        _ => false,
    }
}

/// Two lengths are the same parameter — see [`Dimension::same_parameter`].
fn dimension_eq(a: &Dimension, b: &Dimension) -> bool {
    a.same_parameter(b)
}

fn opt_dimension_eq(a: &Option<Dimension>, b: &Option<Dimension>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => dimension_eq(a, b),
        _ => false,
    }
}

fn size_value_eq(a: &SizeValue, b: &SizeValue) -> bool {
    a.same_parameter(b)
}

/// Open-node Skip fingerprint (type + params).
pub(crate) fn node_key_of(n: &ModifierNode) -> String {
    match n {
        ModifierNode::Draw(d) => format!("draw:{}", d.node_key()),
        ModifierNode::DrawWrap(w) => format!("draw_wrap:{}", w.node_key()),
        ModifierNode::Click(c) => format!("click:{}", c.node_key()),
        ModifierNode::Pointer(p) => format!("pointer:{}", p.node_key()),
        ModifierNode::Key(k) => format!("key:{}", k.node_key()),
        ModifierNode::Layout(l) => format!("layout:{}", l.node_key()),
    }
}


