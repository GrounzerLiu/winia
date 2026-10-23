use super::super::*;

// ── Input Modifier 方法 ──

impl Modifier {
    /// 添加点击行为
    pub fn clickable(self, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::Clickable {
            on_click: Arc::new(on_click),
            interaction: None,
        })
    }

    /// 添加点击行为并绑定交互源——按下/释放/聚焦/悬停自动发射到 `source`
    /// （对标 Compose `clickable(interactionSource)`：内部组合 focusable + hover）
    pub fn clickable_with_source(
        self,
        source: &MutableInteractionSource,
        on_click: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let source = source.clone();
        self.push(ModifierElement::Clickable {
            on_click: Arc::new(on_click),
            interaction: Some(source.clone()),
        })
        .push(ModifierElement::Focusable { interaction: Some(source.clone()) })
        .push(ModifierElement::Hoverable { interaction: source })
    }

    // ── 手势（对标 Compose detectTapGestures / detectDragGestures） ──

    /// 点按回调（up 且未超过 touch slop）——参数为本地坐标
    pub fn on_tap(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnTap { cb: Arc::new(cb) })
    }

    /// 双击回调（两次 tap 间隔 < 300ms 且位置差 < 50px）
    pub fn on_double_tap(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnDoubleTap { cb: Arc::new(cb) })
    }

    /// 长按回调（down 持续 > 500ms 且未移动）——当前在 up 时判定
    pub fn on_long_press(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnLongPress { cb: Arc::new(cb) })
    }

    /// 按下回调（down 立即触发——onPress 语义）
    pub fn on_press(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::TapOnPress { cb: Arc::new(cb) })
    }

    /// 拖拽开始回调（首次超过 touch slop）
    pub fn on_drag_start(self, cb: impl Fn((f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnStart { cb: Arc::new(cb) })
    }

    /// 拖拽移动回调（当前位置, 增量）
    pub fn on_drag(self, cb: impl Fn((f32, f32), (f32, f32)) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnMove { cb: Arc::new(cb) })
    }

    /// 拖拽结束回调
    pub fn on_drag_end(self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnEnd { cb: Arc::new(cb) })
    }

    /// 拖拽取消回调（系统打断）
    pub fn on_drag_cancel(self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::DragOnCancel { cb: Arc::new(cb) })
    }

    /// 标记为可获焦点
    pub fn focusable(self) -> Self {
        self.push(ModifierElement::Focusable { interaction: None })
    }

    /// 标记为可获焦点并绑定交互源——焦点变化自动发射 Focus/Unfocus
    pub fn focusable_with_source(self, source: &MutableInteractionSource) -> Self {
        self.push(ModifierElement::Focusable { interaction: Some(source.clone()) })
    }

    /// Compose Desktop's `Modifier.pointerHoverIcon(icon)`: while the pointer is over this node
    /// (or its descendants), the window's cursor is `icon`. A closer descendant setting its own
    /// icon wins; see [`pointer_hover_icon_override_descendants`](Self::pointer_hover_icon_override_descendants).
    pub fn pointer_hover_icon(self, icon: impl Into<crate::input::PointerIcon>) -> Self {
        self.push(ModifierElement::PointerHoverIcon {
            icon: icon.into(),
            override_descendants: false,
        })
    }

    /// Compose Desktop's `pointerHoverIcon(icon, overrideDescendants = true)`: this node's icon
    /// wins over any `pointer_hover_icon` set by a descendant.
    pub fn pointer_hover_icon_override_descendants(
        self,
        icon: impl Into<crate::input::PointerIcon>,
    ) -> Self {
        self.push(ModifierElement::PointerHoverIcon {
            icon: icon.into(),
            override_descendants: true,
        })
    }

    /// 悬停——指针进入/离开自动发射 Hover Enter/Exit（对标 Compose hoverable）
    pub fn hoverable(self, source: &MutableInteractionSource) -> Self {
        self.push(ModifierElement::Hoverable { interaction: source.clone() })
    }

    /// 水波纹指示（对标 Compose `indication` + Material ripple）。
    /// `bounded=true` 时裁剪到节点形状（默认）；`color` 为波纹颜色
    /// （Material 用 content 色——Button 已自动附加，通用 clickable 可手动加）。
    pub fn ripple(
        self,
        source: &MutableInteractionSource,
        color: impl Into<crate::graphics::Dyn<Color>>,
        bounded: bool,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color: color.into(),
            bounded,
            shape: None,
            state_layer_diameter: None,
        })
    }

    /// Ripple indication whose BACKGROUND state layer has an explicit DIAMETER — Compose's
    /// `ripple(bounded, radius)`, where `radius` is half that. material3 hands it `StateLayerSize / 2`: the
    /// checkbox/radio state layer is 40 dp while the node (the touch target) is 48 dp, so the halo cannot be
    /// derived from the node. The spreading foreground layer is unaffected and still grows to the node's
    /// diagonal.
    pub fn ripple_with_state_layer_size(
        self,
        source: &MutableInteractionSource,
        color: impl Into<crate::graphics::Dyn<Color>>,
        bounded: bool,
        diameter: impl Into<crate::graphics::Dyn<f32>>,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color: color.into(),
            bounded,
            shape: None,
            state_layer_diameter: Some(diameter.into()),
        })
    }

    /// 水波纹指示——显式指定裁剪形状（对标 Compose Surface indication：
    /// 容器 shape 同时决定波纹裁剪；Outlined/Text 等无背景元素的按钮
    /// 必须显式传 shape，否则裁剪回退为矩形）
    pub fn ripple_with_shape(
        self,
        source: &MutableInteractionSource,
        color: impl Into<crate::graphics::Dyn<Color>>,
        bounded: bool,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
    ) -> Self {
        self.push(ModifierElement::Ripple {
            source: source.clone(),
            color: color.into(),
            bounded,
            shape: Some(shape.into()),
            state_layer_diameter: None,
        })
    }

    /// 绘制图标（Icon 组件内部使用）——tint/autoMirror/可变轴在渲染期求值
    /// 自定义绘制：渲染期以节点 rect 调用闭包（Canvas 原语——轨道/刻度/
/// thumb 等动态绘制；对齐 Compose Canvas/drawWithCache 的轻量替代）。
/// 闭包在渲染期调用——读 State::peek() 不触发重组（值变化由外部 notify 驱动重绘）。
pub fn draw(self, f: impl Fn(&skia_safe::Canvas, skia_safe::Rect) + Send + Sync + 'static) -> Self {
    self.push(ModifierElement::CustomDraw { f: Arc::new(f) })
}

/// 禁用框架自动焦点环（组件自绘焦点环时用——如 Slider 焦点环包围 thumb 胶囊，
/// 而非整个组件 rect）
pub fn no_focus_ring(self) -> Self {
    self.push(ModifierElement::NoFocusRing)
}

pub fn draw_icon(self, spec: crate::graphics::IconSpec) -> Self {
        self.push(ModifierElement::DrawIcon { spec })
    }

    /// 图片内容元素（Image 组件用——绘制按 ContentScale/对齐/alpha）
    pub fn image_content(
        self,
        source: crate::graphics::IconSource,
        content_scale: crate::graphics::ContentScale,
        alignment: crate::graphics::ImageAlignment,
        alpha: impl Into<crate::graphics::Dyn<f32>>,
        color_filter: Option<crate::graphics::Dyn<ColorFilter>>,
        filter_quality: FilterQuality,
    ) -> Self {
        self.push(ModifierElement::ImageContent {
            source,
            content_scale,
            alignment,
            alpha: alpha.into(),
            color_filter,
            filter_quality,
        })
    }

    /// 图片固有尺寸（位图像素尺寸 / SVG viewBox）——测量期调用
    pub(crate) fn image_intrinsic_size(&self) -> Option<(f32, f32)> {
        for el in &self.elements {
            if let ModifierElement::ImageContent { source, .. } = el {
                return source.intrinsic_size();
            }
        }
        None
    }

    /// 关联 FocusRequester（不消耗所有权）
    pub fn on_key_event(self, handler: impl Fn(&KbEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::KbEvent { on_key: Some(Arc::new(handler)), on_pre_key: None })
    }

    /// 预拦截按键事件
    pub fn on_pre_key_event(self, handler: impl Fn(&KbEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::KbEvent { on_key: None, on_pre_key: Some(Arc::new(handler)) })
    }

    /// 关联 FocusRequester

    /// 指针事件（inner→outer 冒泡）
    pub fn on_pointer_event(self, handler: impl Fn(&PointerEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::PointerEvent { on_ptr: Some(Arc::new(handler)), on_pre_ptr: None })
    }

    /// 预拦截指针事件（outer→inner）
    pub fn on_pre_pointer_event(self, handler: impl Fn(&PointerEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::PointerEvent { on_ptr: None, on_pre_ptr: Some(Arc::new(handler)) })
    }

    /// 关联 FocusRequester
    pub fn focus_requester(self, fr: impl Into<FocusRequester>) -> Self {
        let fr = fr.into();
        self.push(ModifierElement::FocusRequesterId { id: fr.id })
    }

    /// 图形层变换（scale/alpha/rotation/translation）。
    ///
    /// 统一入口：静态参数与动画闭包自动适配（`impl Into<GraphicsLayerSpec>`）。
    ///
    /// **静态用法**：
    /// ```
    /// # use winia::prelude::*;
    /// # use winia::graphics::GraphicsLayerParams;
    /// let _m = Modifier::new().graphics_layer(GraphicsLayerParams { alpha: 0.5, ..Default::default() });
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```ignore
    /// .graphics_layer(|| GraphicsLayerParams { alpha: pulse.peek(), ..Default::default() })
    /// ```
    ///
    /// **原则**：与 `background` 相同——静态传值，动画传闭包（`peek()` 读取，
    /// 配合 `Visual::set` 零重组）。仅影响绘制层，不触发布局。
    pub fn graphics_layer(self, params: impl Into<GraphicsLayerSpec>) -> Self {
        self.push(ModifierElement::GraphicsLayer {
            params: params.into(),
        })
    }

    /// `on_size_changed(f)`（对标 Compose `Modifier.onSizeChanged`）——节点
    /// 测量完成后回调逻辑像素尺寸 `(width, height)`。**尺寸变化才回调**
    /// （去重作用域 = 元素实例：modifier 链每次重组会重建元素并重置去重状态，
    /// 跨帧防重由回调内 `State::set` 的 PartialEq 承担）。窗口 resize/内容变化
    /// 触发重测时回调。回调内可写 State（如动画位移基准）——写相同值会被
    /// 去重，不会引发重组循环。
    pub fn on_size_changed(self, f: impl Fn(f32, f32) + Send + Sync + 'static) -> Self {
        let last: std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>> = Default::default();
        self.push(ModifierElement::OnSizeChanged {
            callback: std::sync::Arc::new(move |w, h| {
                let mut last = last.lock().unwrap();
                if *last != Some((w, h)) {
                    *last = Some((w, h));
                    drop(last);
                    f(w, h);
                }
            }),
        })
    }

    /// 测量完成后调用链上全部 `on_size_changed` 回调（元素内各自去重）
    pub(crate) fn report_measured_size(&self, width: f32, height: f32) {
        for el in &self.elements {
            if let ModifierElement::OnSizeChanged { callback } = el {
                callback(width, height);
            }
        }
    }

    /// Compose's `Modifier.onGloballyPositioned`: the callback receives the node's placed box in
    /// window coordinates (scroll offsets included) whenever that box changes.
    pub fn on_globally_positioned(
        self,
        callback: impl Fn(crate::layout::GlobalPosition) + Send + Sync + 'static,
    ) -> Self {
        self.push(ModifierElement::OnGloballyPositioned {
            callback: std::sync::Arc::new(callback),
        })
    }

    /// Compose's `Modifier.onPlaced`: the same box, delivered before the `on_globally_positioned`
    /// callbacks of the same pass.
    pub fn on_placed(
        self,
        callback: impl Fn(crate::layout::GlobalPosition) + Send + Sync + 'static,
    ) -> Self {
        self.push(ModifierElement::OnPlaced {
            callback: std::sync::Arc::new(callback),
        })
    }

    /// Compose's `Modifier.layoutId`: a tag for this layout node (debug tree and
    /// [`Composer::find_by_layout_id`](crate::runtime::composer::Composer::find_by_layout_id)).
    pub fn layout_id(self, id: impl Into<std::sync::Arc<str>>) -> Self {
        self.push(ModifierElement::LayoutId { id: id.into() })
    }

    /// Compose's `Modifier.bringIntoViewRequester`: attach the handle to this node. A request made
    /// through it scrolls the nearest scrolling ancestors so the node is visible.
    pub fn bring_into_view_requester(
        self,
        requester: &crate::layout::BringIntoViewRequester,
    ) -> Self {
        // Subscribe the enclosing group to the request counter: a request then wakes a frame even
        // when nothing else changed, which is what gets the post-layout pass to serve it.
        requester.subscribe();
        self.push(ModifierElement::BringIntoViewRequester {
            requester: requester.clone(),
        })
    }

    /// Whether this chain carries an `on_globally_positioned` / `on_placed` callback.
    pub(crate) fn has_position_callbacks(&self) -> bool {
        self.elements.iter().any(|el| {
            matches!(
                el,
                ModifierElement::OnGloballyPositioned { .. } | ModifierElement::OnPlaced { .. }
            )
        })
    }

    /// Whether this chain carries a bring-into-view requester (requests may be queued on it).
    pub(crate) fn has_bring_into_view_requester(&self) -> bool {
        self.elements
            .iter()
            .any(|el| matches!(el, ModifierElement::BringIntoViewRequester { .. }))
    }

    /// The chain's `layout_id`, if one was declared.
    pub fn layout_id_value(&self) -> Option<&str> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::LayoutId { id } => Some(&**id),
            _ => None,
        })
    }

    /// `alpha(a)`（对标 Compose `Modifier.alpha`）——透明度便捷包装：
    /// `a != 1.0` 时应用 `graphicsLayer(alpha = a, clip = true)`（**alpha<1
    /// 隐式裁剪到 bounds**——Compose 语义）。已存在 GraphicsLayer 元素时
    /// **合并**（alpha 相乘 + 开 clip）而非叠加嵌套层。
    pub fn alpha(self, alpha: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let alpha = alpha.into();
        if !alpha.is_dynamic() && alpha.eval() == 1.0 {
            return self;
        }
        self.merge_layer_f32(alpha, |p, a| {
            p.alpha *= a;
            p.clip = true;
        })
    }

    /// `rotate(degrees)`（对标 Compose `Modifier.rotate`）——绕**中心**
    /// 顺时针旋转（默认 transformOrigin=Center）。`degrees != 0` 才应用。
    pub fn rotate(self, degrees: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let degrees = degrees.into();
        if !degrees.is_dynamic() && degrees.eval() == 0.0 {
            return self;
        }
        self.merge_layer_f32(degrees, |p, d| p.rotation_z += d)
    }

    /// `scale(sx, sy)`（对标 Compose `Modifier.scale`）——绕**中心**缩放。
    /// 非 1 才应用。
    pub fn scale(
        self,
        sx: impl Into<crate::graphics::Dyn<f32>>,
        sy: impl Into<crate::graphics::Dyn<f32>>,
    ) -> Self {
        let (sx, sy) = (sx.into(), sy.into());
        if !sx.is_dynamic() && !sy.is_dynamic() && sx.eval() == 1.0 && sy.eval() == 1.0 {
            return self;
        }
        if sx.is_dynamic() || sy.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| {
                p.scale_x *= sx.eval();
                p.scale_y *= sy.eval();
            })
        } else {
            let (x, y) = (sx.eval(), sy.eval());
            self.merge_graphics_layer(move |p| {
                p.scale_x *= x;
                p.scale_y *= y;
            })
        }
    }

    /// `rotationX(degrees)`（对标 Compose `Modifier.rotationX`）——绕 X 轴
    /// 3D 旋转（带 cameraDistance 透视；绕 transformOrigin）。`degrees != 0`
    /// 才应用。
    pub fn rotation_x(self, degrees: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let degrees = degrees.into();
        if !degrees.is_dynamic() && degrees.eval() == 0.0 {
            return self;
        }
        self.merge_layer_f32(degrees, |p, d| p.rotation_x += d)
    }

    /// `rotationY(degrees)`（对标 Compose `Modifier.rotationY`）——绕 Y 轴
    /// 3D 旋转（带 cameraDistance 透视；绕 transformOrigin）。
    pub fn rotation_y(self, degrees: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let degrees = degrees.into();
        if !degrees.is_dynamic() && degrees.eval() == 0.0 {
            return self;
        }
        self.merge_layer_f32(degrees, |p, d| p.rotation_y += d)
    }

    /// `cameraDistance(distance)`（对标 Compose `Modifier.cameraDistance`）——
    /// 3D 旋转的相机距离（逻辑 px；越大透视越平，Compose 默认 8.dp）。
    pub fn camera_distance(self, distance: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.merge_layer_f32(distance.into(), |p, d| p.camera_distance = d)
    }

    /// `shadowElevation(elevation)`（对标 Compose `graphicsLayer.shadowElevation`）——
    /// 图层阴影高度（逻辑 px；>0 时在图层内容后画 ambient+spot 阴影）。
    pub fn shadow_elevation(self, elevation: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        let elevation = elevation.into();
        if !elevation.is_dynamic() && elevation.eval() <= 0.0 {
            return self;
        }
        self.merge_layer_f32(elevation, |p, e| p.shadow_elevation = e)
    }

    /// 图层阴影形状（对标 Compose `graphicsLayer.shape`——默认矩形）
    pub fn shadow_shape(self, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        let shape = shape.into();
        if shape.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| p.shadow_shape = Some(shape.eval()))
        } else {
            let value = shape.eval();
            self.merge_graphics_layer(move |p| p.shadow_shape = Some(value))
        }
    }

    /// 图层环境光阴影颜色（对标 Compose `ambientShadowColor`）。
    pub fn ambient_shadow_color(self, color: impl Into<crate::graphics::Dyn<Color>>) -> Self {
        self.merge_layer_color(color.into(), |p, c| p.ambient_shadow_color = c)
    }

    /// 图层投射光阴影颜色（对标 Compose `spotShadowColor`）。
    pub fn spot_shadow_color(self, color: impl Into<crate::graphics::Dyn<Color>>) -> Self {
        self.merge_layer_color(color.into(), |p, c| p.spot_shadow_color = c)
    }

    /// 便捷包装合并：已有 GraphicsLayer 元素 → 在其参数上叠加；否则 push 新元素。
    ///
    /// A static parameter set stays static (the mutator is applied now, at build time); a dynamic one
    /// stays a closure (the mutator runs inside it, per paint). The mutators here only assign captured
    /// values, so this loses no dynamism.
    pub(super) fn merge_graphics_layer(mut self, f: impl Fn(&mut GraphicsLayerParams) + Send + Sync + 'static) -> Self {
        let f = Arc::new(f);
        // mem::take 取出 elements（空 Vec 占位）——局部修改后再放回，
        // 完全避开 self 借用与 push 的冲突
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements
            .iter_mut()
            .find(|e| matches!(e, ModifierElement::GraphicsLayer { .. }))
        {
            if let ModifierElement::GraphicsLayer { params } = el {
                let old = params.clone();
                let f = f.clone();
                *params = match old {
                    crate::graphics::Dyn::Static(mut p) => {
                        f(&mut p);
                        crate::graphics::Dyn::Static(p)
                    }
                    crate::graphics::Dyn::Dynamic(g) => {
                        crate::graphics::Dyn::Dynamic(Arc::new(move || {
                            let mut p = g();
                            f(&mut p);
                            p
                        }))
                    }
                };
            }
        } else {
            let mut p = GraphicsLayerParams::default();
            f(&mut p);
            elements.push(ModifierElement::GraphicsLayer {
                params: crate::graphics::Dyn::Static(p),
            });
        }
        self.elements = elements;
        self
    }

    /// Merge one scalar layer parameter that may be read per paint: a dynamic carrier takes the
    /// per-paint path, a literal is applied now (and keeps the plain, allocation-free value).
    fn merge_layer_f32(
        self,
        value: crate::graphics::Dyn<f32>,
        apply: impl Fn(&mut GraphicsLayerParams, f32) + Send + Sync + 'static,
    ) -> Self {
        if value.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| apply(p, value.eval()))
        } else {
            let v = value.eval();
            self.merge_graphics_layer(move |p| apply(p, v))
        }
    }

    /// The colour twin of [`merge_layer_f32`](Self::merge_layer_f32).
    fn merge_layer_color(
        self,
        value: crate::graphics::Dyn<Color>,
        apply: impl Fn(&mut GraphicsLayerParams, Color) + Send + Sync + 'static,
    ) -> Self {
        if value.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| apply(p, value.eval()))
        } else {
            let v = value.eval();
            self.merge_graphics_layer(move |p| apply(p, v))
        }
    }

    /// Like [`merge_graphics_layer`](Self::merge_graphics_layer), but the mutator runs on EVERY paint:
    /// the element always becomes (or stays) a dynamic layer. Use it when the mutator reads something
    /// that changes per frame (a dynamic shape); a static value keeps the plain form and its no-alloc
    /// path.
    pub(super) fn merge_graphics_layer_per_paint(
        mut self,
        f: impl Fn(&mut GraphicsLayerParams) + Send + Sync + 'static,
    ) -> Self {
        let f = Arc::new(f);
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements
            .iter_mut()
            .find(|e| matches!(e, ModifierElement::GraphicsLayer { .. }))
        {
            if let ModifierElement::GraphicsLayer { params } = el {
                let old = params.clone();
                let f = f.clone();
                *params = match old {
                    crate::graphics::Dyn::Static(base) => {
                        crate::graphics::Dyn::Dynamic(Arc::new(move || {
                            let mut p = base.clone();
                            f(&mut p);
                            p
                        }))
                    }
                    crate::graphics::Dyn::Dynamic(g) => crate::graphics::Dyn::Dynamic(Arc::new(move || {
                        let mut p = g();
                        f(&mut p);
                        p
                    })),
                };
            }
        } else {
            elements.push(ModifierElement::GraphicsLayer {
                params: crate::graphics::Dyn::Dynamic(Arc::new(move || {
                    let mut p = GraphicsLayerParams::default();
                    f(&mut p);
                    p
                })),
            });
        }
        self.elements = elements;
        self
    }

    /// 垂直滚动（绑定 ScrollState）
    pub fn vertical_scroll(self, state: ScrollState) -> Self {
        // 无 builder 期 get 副作用（State Phase4）：依赖由物化期
        // register_modifier_deps_recursive 统一注册到实际 node slot
        //（composer.rs compose 末尾，take_deps 之前）。builder 期 get 会把
        // 依赖注册到调用 scope，与 slot 注册重复。
        self.push(ModifierElement::VerticalScroll { state })
    }

    /// 水平滚动
    /// 标记为 lazy 滚动容器（LazyColumn 内部使用——内容总高 State）
    pub fn lazy_scroll(self, content_height: crate::runtime::state::Backchannel<f32>) -> Self {
        self.push(ModifierElement::LazyScroll { content_height, reverse: false })
    }

    /// lazy 列表是否反向布局（reverseLayout：render 滚动平移镜像）
    pub fn is_lazy_scroll_reverse(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::LazyScroll { reverse: true, .. }))
    }

    /// lazy 列表反向布局标记（reverseLayout：render 侧镜像滚动平移）
    pub fn lazy_scroll_reverse(mut self, reverse: bool) -> Self {
        if let Some(el) = self.elements.last_mut() {
            if let ModifierElement::LazyScroll { reverse: r, .. } = el {
                *r = reverse;
            }
        }
        self
    }

    /// Compose's `LazyItemScope.animateItem()`: place this on the item's root. A lazy list springs
    /// the item from its old slot to the new one when the data reorders (keyed by the item key).
    /// FlowRow/FlowColumn also honor it, but only when the container opts in with
    /// `FlowRow::animate_placement(true)` / `FlowColumn::animate_placement(true)`. Elsewhere the
    /// marker is inert.
    pub fn animate_item(self) -> Self {
        self.push(ModifierElement::AnimateItem)
    }

    /// 绑定嵌套滚动连接。连接会在 descendant scrollable 的 pre/post 阶段被调度。
    /// Compose's `clipScrollableContainer(orientation)`: clip the scroll container on its SCROLL
    /// axis while leaving 30 dp of room on the cross axis, so a shadow drawn there survives.
    /// winia always clips its scroll viewport; this modifier makes the clip cross-axis aware.
    pub fn clip_scrollable_container(self, orientation: crate::overscroll::Orientation) -> Self {
        self.push(ModifierElement::ClipScrollableContainer {
            vertical: orientation == crate::overscroll::Orientation::Vertical,
        })
    }

    /// Compose's inner shadow: a rim inside `shape`, painted above the content. `blur` is the mask's
    /// sigma in logical pixels; `offset` moves the cut-out shape, which is what puts the shadow on
    /// one side.
    pub fn inner_shadow(
        self,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        color: Color,
        blur: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> Self {
        self.push(ModifierElement::InnerShadow {
            shape: shape.into(),
            color,
            blur,
            offset_x,
            offset_y,
        })
    }

    /// Compose's `Modifier.overscroll(effect)`: the delta a scroll container cannot consume (it is
    /// at the start or the end) is pulled into the effect instead of falling through to an ancestor,
    /// which translates the content and returns with a short spring.
    pub fn overscroll(self, effect: &crate::overscroll::OverscrollEffect) -> Self {
        self.push(ModifierElement::Overscroll {
            effect: effect.clone(),
        })
    }

    pub fn nested_scroll(self, connection: impl crate::nested_scroll::NestedScrollConnection + 'static) -> Self {
        self.push(ModifierElement::NestedScroll { connection: Arc::new(connection) })
    }

    /// Attach an already-shared nested-scroll connection - for a component that keeps a handle of its
    /// own (the pager's `page_nested_scroll_connection`) and attaches it on every build.
    pub(crate) fn nested_scroll_arc(
        self,
        connection: Arc<dyn crate::nested_scroll::NestedScrollConnection>,
    ) -> Self {
        self.push(ModifierElement::NestedScroll { connection })
    }

    /// 水平滚动（绑定 ScrollState——对齐 vertical_scroll）
    pub fn horizontal_scroll(self, state: ScrollState) -> Self {
        // 无 builder 期 get 副作用（同 vertical_scroll，State Phase4）。
        self.push(ModifierElement::HorizontalScroll { state, reverse: false })
    }

    /// 水平滚动是否反向（RTL：render 滚动平移镜像——offset 0 显示内容末端）
    pub fn is_horizontal_scroll_reverse(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::HorizontalScroll { reverse: true, .. }))
    }

    /// 水平滚动反向标记（RTL：`scroll_reverse` 语义——render 镜像平移；
    /// 同 lazy_scroll_reverse，但作用于 HorizontalScroll 元素）
    pub fn horizontal_scroll_reverse(mut self, reverse: bool) -> Self {
        if let Some(el) = self.elements.last_mut() {
            if let ModifierElement::HorizontalScroll { reverse: r, .. } = el {
                *r = reverse;
            }
        }
        self
    }
}

// ── 查询方法: 布局参数提取 ──
