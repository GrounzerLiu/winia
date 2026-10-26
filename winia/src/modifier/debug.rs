use super::*;

impl Debug for ModifierElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size { width, height } => f.debug_struct("Size").field("width", width).field("height", height).finish(),
            Self::MinWidth { value } => f.debug_struct("MinWidth").field("value", value).finish(),
            Self::MinHeight { value } => f.debug_struct("MinHeight").field("value", value).finish(),
            Self::MaxWidth { value } => f.debug_struct("MaxWidth").field("value", value).finish(),
            Self::MaxHeight { value } => f.debug_struct("MaxHeight").field("value", value).finish(),
            Self::WindowInsetsPadding { kind, .. } => {
                f.debug_tuple("WindowInsetsPadding").field(kind).finish()
            }
            Self::ConsumeWindowInsets { kind, .. } => {
                f.debug_tuple("ConsumeWindowInsets").field(kind).finish()
            }
            Self::PaddingSides { start, top, end, bottom } => f
                .debug_struct("PaddingSides")
                .field("start", start)
                .field("top", top)
                .field("end", end)
                .field("bottom", bottom)
                .finish(),
            Self::FillMaxWidth { fraction } => f.debug_struct("FillMaxWidth").field("fraction", fraction).finish(),
            Self::FillMaxHeight { fraction } => f.debug_struct("FillMaxHeight").field("fraction", fraction).finish(),
            Self::FillMaxSize { fraction } => f.debug_struct("FillMaxSize").field("fraction", fraction).finish(),
            Self::SizeIn { min_width, min_height, max_width, max_height, enforce_incoming } => f
                .debug_struct("SizeIn")
                .field("min_width", min_width)
                .field("min_height", min_height)
                .field("max_width", max_width)
                .field("max_height", max_height)
                .field("enforce_incoming", enforce_incoming)
                .finish(),
            Self::DefaultMinSize { min_width, min_height } => f
                .debug_struct("DefaultMinSize")
                .field("min_width", min_width)
                .field("min_height", min_height)
                .finish(),
            Self::WrapContent { width, height, align, unbounded } => f
                .debug_struct("WrapContent")
                .field("width", width)
                .field("height", height)
                .field("align", align)
                .field("unbounded", unbounded)
                .finish(),
            Self::Offset { x, y } => f.debug_struct("Offset").field("x", x).field("y", y).finish(),
            Self::AbsoluteOffset { x, y } => f.debug_struct("AbsoluteOffset").field("x", x).field("y", y).finish(),
            Self::AlignSelf { alignment } => f.debug_struct("AlignSelf").field("alignment", alignment).finish(),
            Self::ContentAlignSelf { alignment } => {
                f.debug_struct("ContentAlignSelf").field("alignment", alignment).finish()
            }
            Self::MatchParentSize => f.write_str("MatchParentSize"),
            Self::Visible { visible } => f.debug_struct("Visible").field("visible", visible).finish(),
            Self::GridItemArea { area_id } => {
                f.debug_struct("GridItemArea").field("area_id", area_id).finish()
            }
            Self::GridItem { column_span, row_span, alignment } => f
                .debug_struct("GridItem")
                .field("column_span", column_span)
                .field("row_span", row_span)
                .field("alignment", alignment)
                .finish(),
            Self::AlignBy { line } => f.debug_struct("AlignBy").field("line", line).finish(),
            Self::FillMaxRowHeight { fraction } => {
                f.debug_struct("FillMaxRowHeight").field("fraction", fraction).finish()
            }
            Self::FillMaxColumnWidth { fraction } => {
                f.debug_struct("FillMaxColumnWidth").field("fraction", fraction).finish()
            }
            Self::AlignmentLineValue { line, .. } => {
                f.debug_struct("AlignmentLineValue").field("line", line).finish()
            }
            Self::MinimumInteractiveSize { size } => {
                f.debug_struct("MinimumInteractiveSize").field("size", size).finish()
            }
            Self::PaddingFrom { line, before, after } => f
                .debug_struct("PaddingFrom")
                .field("line", line)
                .field("before", before)
                .field("after", after)
                .finish(),
            Self::LayoutWeight { weight, fill } => f.debug_struct("LayoutWeight").field("weight", weight).field("fill", fill).finish(),
            Self::AspectRatio { ratio, .. } => f.debug_struct("AspectRatio").field("ratio", ratio).finish(),
            Self::RequiredSize { width, height } => f
                .debug_struct("RequiredSize")
                .field("width", width)
                .field("height", height)
                .finish(),
            Self::OnSizeChanged { .. } => f.write_str("OnSizeChanged"),
            Self::OnGloballyPositioned { .. } => f.write_str("OnGloballyPositioned"),
            Self::OnPlaced { .. } => f.write_str("OnPlaced"),
            Self::LayoutId { id } => f.debug_struct("LayoutId").field("id", id).finish(),
            Self::BringIntoViewRequester { .. } => f.write_str("BringIntoViewRequester"),
            Self::TestTag { tag } => f.debug_struct("TestTag").field("tag", tag).finish(),
            Self::Semantics(config) => f.debug_tuple("Semantics").field(config).finish(),
            Self::ClearAndSetSemantics(config) => {
                f.debug_tuple("ClearAndSetSemantics").field(config).finish()
            }
            Self::FocusGroup => f.write_str("FocusGroup"),
            Self::FocusProperties { can_focus } => {
                f.debug_struct("FocusProperties").field("can_focus", can_focus).finish()
            }
            Self::OnFocusChanged { .. } => f.write_str("OnFocusChanged"),
            Self::LayoutDirection(d) => f.debug_tuple("LayoutDirection").field(d).finish(),
            Self::ZIndex(z) => f.debug_tuple("ZIndex").field(z).finish(),
            Self::TextFieldSlot { role } => f.debug_struct("TextFieldSlot").field("role", role).finish(),
            Self::Shadow { params, .. } => f.debug_struct("Shadow").field("params", params).finish(),
            Self::Background { .. } => f.debug_struct("Background").finish(),
            Self::BackgroundBrush { shape, .. } => {
                f.debug_struct("BackgroundBrush").field("shape", shape).finish()
            }
            Self::Border { width, color, shape } => f.debug_struct("Border").field("width", width).field("color", color).field("shape", shape).finish(),
            Self::Clip { shape } => f.debug_struct("Clip").field("shape", shape).finish(),
            Self::TextContent { content, font_size, .. } => f
                .debug_struct("TextContent")
                .field("content", content)
                .field("font_size", font_size)
                .finish(),
            Self::RichTextContent { content, drawable_ranges, .. } => f
                .debug_struct("RichTextContent")
                .field("content", content)
                .field("drawables", &format_args!("{} drawables", drawable_ranges.len()))
                .finish(),
            Self::Clickable { .. } => f.write_str("Clickable(<fn>)"),
            Self::TapOnTap { .. } => f.write_str("TapOnTap(<fn>)"),
            Self::TapOnDoubleTap { .. } => f.write_str("TapOnDoubleTap(<fn>)"),
            Self::TapOnLongPress { .. } => f.write_str("TapOnLongPress(<fn>)"),
            Self::TapOnPress { .. } => f.write_str("TapOnPress(<fn>)"),
            Self::DragOnStart { .. } => f.write_str("DragOnStart(<fn>)"),
            Self::DragOnMove { .. } => f.write_str("DragOnMove(<fn>)"),
            Self::DragOnEnd { .. } => f.write_str("DragOnEnd(<fn>)"),
            Self::DragOnCancel { .. } => f.write_str("DragOnCancel(<fn>)"),
            Self::Focusable { .. } => f.write_str("Focusable"),
            Self::Hoverable { .. } => f.write_str("Hoverable"),
            Self::AnimateItem(spec) => f
                .debug_struct("AnimateItem")
                .field("placement", &spec.placement)
                .field("fade_in", &spec.fade_in)
                .field("fade_out", &spec.fade_out)
                .finish(),
            Self::ItemAnimationHost(_) => f.write_str("ItemAnimationHost"),
            Self::InnerShadow { shape, color, blur, .. } => f
                .debug_struct("InnerShadow")
                .field("shape", shape)
                .field("color", color)
                .field("blur", blur)
                .finish(),
            Self::PointerHoverIcon { icon, override_descendants } => f
                .debug_struct("PointerHoverIcon")
                .field("icon", icon)
                .field("override_descendants", override_descendants)
                .finish(),
            Self::Ripple { color, bounded, .. } => f
                .debug_struct("Ripple")
                .field("color", color)
                .field("bounded", bounded)
                .finish(),
            Self::CustomDraw { .. } => f.write_str("CustomDraw"),
Self::NoFocusRing => f.write_str("NoFocusRing"),
Self::DrawIcon { .. } => f.write_str("DrawIcon"),
            Self::ImageContent { content_scale, alignment, alpha, color_filter, filter_quality, .. } => f
                .debug_struct("ImageContent")
                .field("scale", content_scale)
                .field("align", alignment)
                .field("alpha", alpha)
                .field("color_filter", color_filter)
                .field("filter_quality", filter_quality)
                .finish(),
            Self::KbEvent { on_key, on_pre_key } => f.debug_struct("KbEvent").field("on_key", &on_key.is_some()).field("on_pre_key", &on_pre_key.is_some()).finish(),
            Self::PointerEvent { on_ptr, on_pre_ptr } => f.debug_struct("PointerEvent").field("on_ptr", &on_ptr.is_some()).field("on_pre_ptr", &on_pre_ptr.is_some()).finish(),
            Self::FocusRequesterId { id } => f.debug_tuple("FocusRequesterId").field(id).finish(),
            Self::VerticalScroll { .. } => f.write_str("VerticalScroll(<state>)"),
            Self::ScrollInputDisabled => f.write_str("ScrollInputDisabled"),
            Self::LazyScroll { .. } => f.write_str("LazyScroll(<state>)"),
            Self::HorizontalScroll { .. } => f.write_str("HorizontalScroll(<state>)"),
            Self::NestedScroll { .. } => f.write_str("NestedScroll(<connection>)"),
            Self::ClipScrollableContainer { vertical } => f
                .debug_struct("ClipScrollableContainer")
                .field("vertical", vertical)
                .finish(),
            Self::Overscroll { .. } => f.write_str("Overscroll"),
            Self::GraphicsLayer { .. } => f.debug_struct("GraphicsLayer").finish(),
            Self::Blur { radius } => f.debug_struct("Blur").field("radius", radius).finish(),
            Self::BackdropBlur { radius } => f.debug_struct("BackdropBlur").field("radius", radius).finish(),
            Self::TextFieldVisual { variant, .. } => f.debug_struct("TextFieldVisual").field("variant", variant).finish(),
            Self::TextFieldOffsetMapping { .. } => f.write_str("TextFieldOffsetMapping"),
            Self::SharedTransition { scope_id, key, kind, transform, path, z_index, enter, exit, render_in_overlay, scene: _ } => f
                .debug_struct("SharedTransition")
                .field("scope", scope_id)
                .field("key", key)
                .field("kind", kind)
                .field("transform", transform)
                .field("path", path)
                .field("z_index", z_index)
                .field("has_enter", &enter.is_some())
                .field("has_exit", &exit.is_some())
                .field("render_in_overlay", render_in_overlay)
                .finish(),
            Self::SharedScopeOverlay { scope_id, z_index } => f
                .debug_struct("SharedScopeOverlay")
                .field("scope", scope_id)
                .field("z_index", z_index)
                .finish(),
            Self::SceneTag { id } => f.debug_struct("SceneTag").field("id", id).finish(),
        }
    }
}

