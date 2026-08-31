# winia render.rs / modifier.rs 深度分析报告

- 分析对象：`winia/src/render.rs`（1926 行）、`winia/src/modifier.rs`（2839 行），及调用侧 `winia/src/app.rs`、`skiwin` 后端
- 结论：这是一个 **Compose 语义的 UI 渲染管线**：不可变 enum 链式 Modifier → 单阶段深度遍历 LayoutNode 树绘制到 Skia Canvas，后端为 skiwin Vulkan（含 GL/CPU/D3D 备选）

---

## render.rs — Skia 渲染管线

### 1. 总入口与 app.rs 调用关系

```rust
// render.rs:20-22
pub fn render(nodes: &[LayoutNode], root_idx: usize, canvas: &Canvas) {
    render_pass1(nodes, root_idx, root_idx, canvas, 0.0, 0.0);
}
```

- 单函数入口，转发到深度优先 `render_pass1`（render.rs:597）。
- 调用侧（app.rs，三处）：
  - **主循环** `recompose_layout_render`（app.rs:328）：`RedrawRequested` 内 compose 循环 → layout → overlay 同步 → 绘制（app.rs:403-413）：
    ```rust
    sw.draw(|surface| {
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
        canvas.save();
        canvas.scale((sf, sf));            // HiDPI：逻辑→物理
        render::render(nodes, root_idx, canvas);
        canvas.restore();
        render_overlays(&self.overlays, canvas, sf, (self.width, self.height));
        after_draw(nodes, root_idx, surface);
    });
    ```
  - **首帧**（app.rs:1388-1391，open_window）；**调试/单测路径**（app.rs:2049）。
  - Density 注入：`crate::unit::with_density(Density::from_density(sf))`（app.rs:337/1363），覆盖 compose+layout+draw 全程，使 `Dimension::Px/TextUnit::Px` 使用窗口 scale_factor。
- **决策+原因**：render 只拿 `&Canvas`，surface 生命周期由后端 `sw.draw(|surface|…)` 管理——渲染器与后端/窗口系统完全解耦。

### 2. 坐标累加（render.rs:597-611）

```rust
fn render_pass1(nodes, root_idx, idx, canvas, parent_x, parent_y) {
    let node = &nodes[idx];
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;
    if w <= 0.0 || h <= 0.0 { return; }
    let rect = Rect::new(x, y, x + w, y + h);
```
- 子节点以 `render_pass1(nodes, root_idx, child, canvas, x, y)` 递归（render.rs:1020-1022）——`node.position` 为父相对坐标，绝对坐标靠参数逐层累加；零尺寸节点整支剪枝。

### 3. Scroll clip + translate（render.rs:975-1028）

- 仅在 vertical/horizontal scroll offset 存在时进入（render.rs:976-1017）：
  ```rust
  let clip_rect = Rect::new(x, y, x + cw, y + ch);
  let dx = -scroll_offset_h.unwrap_or(0.0);
  let dy = -scroll_offset_v.unwrap_or(0.0);
  canvas.save();
  canvas.clip_rect(clip_rect, None, Some(false));
  canvas.translate((dx, dy));
  scrolled = true;
  ...
  if scrolled { canvas.restore(); }
  ```
- 可视尺寸 `cw/ch` 来源（render.rs:978-1009）：先扫 `Size{Fixed}` / `FillMax*` modifier；**再以测量期回写的 `node.scroll_viewport_height/width` 覆盖**——因 `measured_size` 是内容全高，直接用它会把裁剪矩形拉到内容末端，滚动内容越界绘制到后续兄弟之上（如 Scaffold bottom bar）。**决策+原因**：viewport 是布局期唯一事实来源。
- reverseLayout（LazyColumn 反向，render.rs:697-712）：`off = (content - viewport - offset).max(0.0)`——内容坐标从底向上排布。

### 4. Skia Canvas / Surface 生命周期

- 主 Surface 完全由后端提供（skiwin `SkiaWindowTrait::draw(&mut self, impl FnOnce(&mut Surface))`，skiwin/src/lib.rs:25）。render 只消费。
- 离屏 surface 仅两处自建：阴影层 `surfaces::raster_n32_premul`（render.rs:204）、测试用 raster（render.rs:1601 等）。
- backdropBlur 用 `unsafe { canvas.surface() }` 读主画布 snapshot（render.rs:1224、1589）——唯一 unsafe。
- save/restore 严格配对，顺序：gl_saved → scrolled → clipped → blur → (渲染期栈序逆推，render.rs:1027-1041)。

### 5. Modifier 绘制顺序（render_pass1 主循环，render.rs:675-796）

单元素扫描顺序（元素链序决定层级）：
1. **Shadow 垫底**（render.rs:675-679 预扫描循环，在背景绘制之前）：
   ```rust
   for el in node.modifier.elements() {
       if let ModifierElement::Shadow { params, shape, .. } = el {
           draw_shadow_layer(canvas, rect, shape, params);
       }
   }
   ```
   **决策+原因**：Compose shadow 是 graphicsLayer 独立层（垫底）；旧实现误放主循环后 → 阴影画在背景之上 → 卡片被压暗（绿卡 ×(1-0.43)≈0.6，实测 bug）。
2. 收集 `blur_radius` / `clip_shape` / scroll offsets（render.rs:684-712）。
3. `TextFieldVisual`：M3 Filled/Outlined 容器 + supporting 文本 + Outlined label 缺口 cutout（render.rs:713-758；cutout 定位用节点绝对坐标而非 parent_x——用错缺口画偏）。
4. `CustomDraw` 闭包 `f(canvas, rect)`（render.rs:760-762）。
5. `DrawIcon` / `ImageContent` 画在 **padding 内缩的内容区域**（render.rs:763-780）。
6. `render_modifier_element`（render.rs:289-330）：Background/Border/BorderDynamic/TextContent。
   - **Background**：`color_fn()` 渲染期求值；记录 `*last_background = Some((color, shape))`。
   - **Border**：`if *last_background != Some((*color, shape))` 才画描边——**决策+原因**：边框色与形状等于容器时合并为纯填充（M3 drawBox 语义），半透明同色叠 stroke 会双重混合、边框明显深于内部。

### 6. background / border / clip 绘制原语

- `draw_background`（render.rs:1311-1328）：Color4f + anti_alias，按 Shape 分派。
- `draw_border`（render.rs:1455-1478）：stroke 居中（`inset = width/2`），圆角 `corner_radius - inset`；Pill 路径圆角 `w.min(h)/2 - inset`（**不重复减**，否则外缘圆角处内缩 1px）。
- clip：`canvas.save()` + clip_rect/rrect/oval + 内容 + restore（render.rs:806-824）。
- `draw_focus`（render.rs:1482-1542）：焦点环宽 3、完全位于组件外部（gap=2）、淡入 1.15× 缩放收敛。

### 7. 阴影（三套）

| 系统 | 入口 | 实现 |
|---|---|---|
| drop_shadow/ShadowParams | `draw_shadow_layer` render.rs:186-286 | 离屏 surface=**内容尺寸**；白色形状（spread 外圈 stroke，Fill+Stroke 两遍）→ `SrcIn` 染色（色×alpha）→ 主画布 `draw_image` + `ImageFilter::blur` **绘制时应用**（避开离屏 clamp）+ CropRect 限模糊输入范围（内容 ±pad）。`sigma = radius×0.57735`（/√3）；`pad = radius*2+spread*2` |
| shadow(elevation) | modifier.rs:849-875 | 展开 **ambient+spot 两层**：ambient 无偏移、blur=e、alpha 0.18×strength；spot 偏移(0,e×0.5)、blur=e×0.25、alpha 0.30。`strength=(elevation/12).min(1.0)`；`elevation<=0 && !clip` 早退（Compose early-return） |
| graphicsLayer.shadowElevation | `draw_elevation_shadow` render.rs:131-157 | `SkShadowUtils::draw_shadow` 真 ambient+spot；`light_pos=(270,0,600)`、`light_radius=800`；elevation 即 Compose Z 语义，不再手工换算 blur |

### 8. GraphicsLayer（render.rs:621-653）

```rust
let gl_params = node.modifier.graphics_layer_params();
let gl_saved = if let Some(gl) = gl_params {
    if gl.alpha < 1.0 { canvas.save_layer_alpha_f(None, gl.alpha); } else { canvas.save(); }
    if gl.shadow_elevation > 0.0 {
        canvas.save(); apply_gl_transform(...); draw_elevation_shadow(...); canvas.restore();  // 阴影随 3D 形变、不参与内容 clip
    }
    if gl.clip { canvas.clip_rect(rect, None, true); }   // 变换前裁剪
    apply_gl_transform(canvas, &gl, x, y, w, h);
    true
} else { false };
```
- **clip 在变换前**（render.rs:645-650）：Compose 语义先裁到图层 bounds 再变换；变换后裁剪会随内容一起转、clip 变无效操作。
- `apply_gl_transform`（render.rs:50-74）序列：`translate(translation) → translate(pivot) → [3D 或 scale+rotate] → translate(-pivot)`；**pivot = 节点绝对位置 + transformOrigin 分数偏移**（只平移 (px,py) 会让旋转中心落在相对窗口原点）。
- 3D 矩阵 `build_gl_3d_matrix`（render.rs:80-126）：仅 rotationX/Y 非零才走 3D，否则 2D（scale+rotate）。顺序 `P·Rx·Ry·Rz·S`。**决策+原因**：相机透视必须**左乘**——直接 `set_rc(3,2)` 只改 w 行 z 列、2D 点 z=0 → 透视永远不参与（相机距离无效根因）。**决策+原因**：有效相机距离 `max(camera_distance, max(w,h), 1.0)`——默认 8 对 170dp 卡片太小、近边 z' 超相机 → w 变负 → 卡片"飞走"；半尺寸在 Rx+Ry 大角度仍可能越界，用 max(w,h) 保证任意角度 w>0（⚠ 按中心枢轴估算，自定义 transformOrigin 时边缘距离不同）。
- alpha 用 `save_layer_alpha_f`（GPU 层），非逐元素改色。

### 9. BackdropBlur — 即时 snapshot 实现（render.rs:1168-1296）

调用点 render.rs:617-619：**在节点自身任何内容绘制之前**执行——snapshot 只含"位于其下"的已画内容（祖先+前面兄弟），对齐 Compose 语义。四步：
1. **物理像素快照区**：`canvas.local_to_device_as_3x3()` 映射节点四角（含全局 HiDPI scale + 祖先 scroll/gl/overlay 平移）→ AABB + `margin = radius×3`（3σ 采样，不足则 Clamp 重复像素产生边缘条纹），floor/ceil 后 clamp 到 surface（render.rs:1203-1228）。**决策+原因**：跨顶/左边缘时以相交原点锚定，防内容偏移——旧两阶段实现 `.max(0.0)` 语义回归点（render.rs:1219-1223）。
2. `surface_snapshot`（render.rs:1588）= clone surface + `image_snapshot_with_bounds`。
3. `ImageFilter::blur((r,r), Clamp, CropRect=快照尺寸)`（render.rs:1243-1249）——CropRect 限定滤镜输出，防越界采样。
4. 画回（render.rs:1251-1273）：逆矩阵把快照原点（物理像素）映射回逻辑坐标 + `scale(1/矩阵缩放)`——**快照像素与物理像素严格 1:1、无采样缩放**（移动不闪烁）；clip 到节点矩形，边缘带不参与合成（无白雾晕开）。
- **已知限制**（render.rs:1186-1188）：blur 画在 graphicsLayer 变换/alpha 之前——节点自身 alpha/scale/rotation 不作用于模糊层；旋转祖先下画回仅 translate+scale 近似；未在 demo 覆盖。

### 10. HiDPI

- app.rs: `canvas.scale((sf, sf))` 后调 `render::render`（app.rs:406-408、1390）；`sf = winit scale_factor`（app.rs:1341、1356），`ScaleFactorChanged` 更新 + request_redraw（app.rs:653-657）。
- Density 随 `with_density` 注入（app.rs:337）。render.rs 全程逻辑坐标；仅 backdropBlur 用 `local_to_device_as_3x3` 映射物理像素。
- `Dimension::Px` 转换读 `current_density()`（modifier.rs:132、1443 等）。

### 11. winit / Vulkan-GL-CPU 后端对接

- 后端 = `skiwin::vulkan::VulkanSkiaWindow`（app.rs:32、1350）；skiwin 提供 cpu/d3d/gl/vulkan 多后端（skiwin/src/{cpu,d3d,gl,vulkan}.rs），vulkan feature = ash + vulkano + skia-safe/vulkan+gpu。
- render.rs 不感知后端——统一收 `&Canvas`。`frame_interval` 由显示器刷新率对齐（app.rs:1342-1349）。

### 12. render-only 事件：redraw 调度 / pending 帧合并

- `RedrawRequested`（app.rs:922-1024）：`frame_tick()`（动画时钟）→ 尺寸自愈 → `update_animations()` → 消费 focus 请求 → `recompose_layout_render`。
- **帧节流**（app.rs:978-989）：距上次渲染 <16ms 且非 force_redraw 则跳过，但**不丢弃更新**——置 `force_redraw=true` + `ControlFlow::WaitUntil`（渲染欠账，下个可用帧由 new_events 补发 request_redraw）——原实现直接跳过会让一次性状态变更卡到外部事件才刷新。
- **compose 合并**（app.rs:343-381）：循环 recompose 直到无 pending state——并发 tokio task 的 notify 不丢失；`any_composed` 标志防 overlay 误删。
- **崩溃边界**（app.rs:979-1016）：`catch_unwind` 包住整帧；`consecutive_panics` 计数，超阈值 `render_disabled` 停更保画面。渲染成功后 `force_redraw=false`。
- 动画自驱动：`is_animating() → request_redraw`（app.rs:1022-1024）——winit `wake_up` 在 Windows 上偶发丢失（已知竞态），显式 request 兜底。
- request_redraw 节流 `should_request_redraw`（app.rs:2849）。

---

## modifier.rs — 链式 Modifier 系统

### 1. 核心类型

- **Dimension**（modifier.rs:22-34）：`Fixed(f32)/Dp/Px/Fill/Auto`；`to_logical_px` 中 Px 需 `current_density()` 转换（本项目 1dp==1 逻辑像素）。
- **SizeValue**（modifier.rs:42-45）：`Static(Dimension) | Dynamic(Arc<dyn Fn()->f32>)`；`From` 覆盖 f32/Dp/Px/`State<f32>`/`DerivedValue`/闭包（modifier.rs:47-98）——布局属性动画直接传 State。
- **Shape**（modifier.rs:159-170）：`RoundedRect{corner_radius}/Pill/Circle/Rectangle`；Pill = 短边一半（Compose CornerFull，M3 Button 默认）。
- **Color**（modifier.rs:186-205）：r/g/b/a u8 + `overlay()`（modifier.rs:211-220）= M3 状态层叠加（hover 8%/press-focus 12%/drag 16% 近似）。
- **BlendMode** 29 值（modifier.rs:325-331，与 skia 同源）、**ColorFilter**（Tint/Matrix/Lighting，modifier.rs:334-342）、**FilterQuality**（None/Low/Medium/High，modifier.rs:345-355）。
- **ModifierElement** 枚举（modifier.rs:367-551），注释明确"enum 而非 trait object，便于 Layout 系统分类提取"——Layout / Draw / Content / Input 四类。

### 2. 链式组合：builder 模式（非 type-state）

```rust
// modifier.rs:559-562
pub struct Modifier { elements: Vec<ModifierElement> }
// modifier.rs:572-576
pub(crate) fn push(mut self, element: ModifierElement) -> Self {
    self.elements.push(element); self
}
// modifier.rs:589-592  Compose Modifier.then
pub fn then(mut self, other: Modifier) -> Self {
    self.elements.extend(other.elements); self
}
```
- 不可变：每次调用消耗 self、追加元素、返回新链（clone-on-write 式，clone 廉价）。左到右=外到内（modifier.rs:5）。
- **决策+原因**：选 enum+Vec（构建期无类型安全约束，靠运行时 match 分派）而非 type-state——支持动态值（State/闭包）与 GraphicsLayer 合并折叠，代价是非法顺序只能运行时暴露。

### 3. Layout Modifier（modifier.rs:608-892）

- `size/width/height`：`width(w)` 用 `Auto` 占位未约束轴（modifier.rs:618-623）。
- padding 全家（modifier.rs:636-725）：`padding/padding_horizontal/padding_vertical/padding_start/end/top/bottom/padding_sides`，每个都 push 一个 `PaddingSides`，`get_padding_sides`（modifier.rs:1438-1458）**累加所有元素**（可叠加）。
- `fill_max_width/height/size`、`min_width/min_height`（Compose widthIn 语义，仅提升 min 约束）。
- `offset`（RTL x 镜像）/`absolute_offset`（RTL 不镜像）。
- `align_self`、`layout_weight`（Row 分宽/Column 分高）、`aspect_ratio`（assert ratio>0，Compose 前置校验）、`required_size/width/height`（忽略 incoming 收缩、允许溢出父约束 = enforceIncoming=false）。
- `test_tag`、`layout_direction`（**节点级覆盖，不向子树继承**——子树用 `WiniaTheme::with_theme_and_direction` CompositionLocal，modifier.rs:410-413）。
- 滚动：`vertical_scroll/horizontal_scroll`（绑定 ScrollState，构造时 `offset.get()` 注册依赖）、`lazy_scroll`/`is_lazy_scroll_reverse`/`lazy_scroll_reverse`（reverseLayout 标记）、`nested_scroll`（connection 在 descendant pre/post 阶段被调度）。

### 4. Draw Modifier（modifier.rs:896-1055）

- `background(color|closure, shape)`：经 `BackgroundColor` 包装（modifier.rs:2080-2105）统一静态 Color 与闭包；**动画原则**（modifier.rs:913-915）：闭包内用 `State::peek()` 读取动画值（`get()` 会注册依赖触发重组），配合 `set_visual`（不 notify）每帧 request_redraw 实现零重组。
- `border`、`border_dynamic`（动态色闭包）。
- `clip(shape)`、`blur(radius)`（GPU saveLayer 零回读）、`backdrop_blur(radius)`（毛玻璃，单 snapshot 多节点共享）。
- `text_content/text_content_full`（Text/TextField 共用 SSOT）、`text_field_visual`、`text_field_offset_mapping`。
- 阴影：`shadow(elevation,…)`（展开 ambient+spot 两层，见 render 章节）、`drop_shadow(shape, params)`（完全自定义单层，可叠加 = Compose vararg 语义）。

### 5. GraphicsLayer Modifier（modifier.rs:1240-1381）

- `graphics_layer(Into<GraphicsLayerSpec>)`：静态 params 或动画闭包（modifier.rs:2107-2121）。
- 便捷包装：`alpha`（**clip=true 隐式**，Compose 语义）、`rotate/scale`（绕中心）、`rotation_x/rotation_y`（3D+透视）、`camera_distance`、`shadow_elevation`、`shadow_shape`、`ambient_shadow_color/spot_shadow_color`——全部走 `merge_graphics_layer`（modifier.rs:1352-1381）：**已有 GraphicsLayer 元素时合并（包一层 params_fn）而非叠加嵌套层**；实现用 `mem::take(&mut self.elements)` 避开借用冲突。
- 读取时多元素折叠：`graphics_layer_params`（modifier.rs:1752-1764）+ `merge_graphics_params`（modifier.rs:2630-2658）：scale/alpha 相乘、translation/rotation 相加、camera distance/origin/shadow 仅在非默认时覆盖、clip 或合并。
- `GraphicsLayerParams` 字段（modifier.rs:1954-1999）：scale_x/y、alpha、translation、rotation_z、transform_origin（默认 CENTER 0.5,0.5）、clip、rotation_x/y、camera_distance（默认 8）、shadow_elevation/shape/ambient(0x20)/spot(0x50) 色。
- **决策+原因**（modifier.rs:1950-1953）：GL 只影响绘制外观，不参与布局与命中测试——命中区域始终是布局 bounds；命中唯一考虑的位移是 scroll（布局层）。

### 6. Interaction Modifier（modifier.rs:1059-1232）

- `clickable` / `clickable_with_source`（后者**内部组合 push Focusable + Hoverable**，对标 Compose clickable(interactionSource)）。
- 手势：`on_tap`（up 且未超 slop，本地坐标）、`on_double_tap`（<300ms & <50px）、`on_long_press`（>500ms 未移动，**当前在 up 时判定**——与 Compose 即时触发有差异，注释标注）、`on_press`（down 立即）、`on_drag_start/on_drag/on_drag_end/on_drag_cancel`。
- `focusable/focusable_with_source`、`hoverable`。
- 键盘/指针：`on_key_event/on_pre_key_event`（KbEvent；**pre=outer→inner 预拦截，普通=inner→outer 冒泡**）、`on_pointer_event/on_pre_pointer_event`（PointerEvent 同语义）。
- 其它：`focus_requester`（FocusRequesterId）、`draw`（CustomDraw Canvas 原语）、`no_focus_ring`（组件自绘焦点环）、`draw_icon`、`image_content`。
- 路由查询：`has_hoverable/has_gesture/has_drag_gesture/has_double_tap`（app.rs 手势/焦点路由用）；`has_double_tap` 决定 onTap 是否延迟到双击窗口结束（Compose detectTapGestures 语义）。

### 7. Ripple 水波纹

- API（modifier.rs:1141-1169）：`ripple(source, color, bounded)`；`ripple_with_shape`（显式裁剪形状——Outlined/Text 无背景元素时**必须**显式传 shape，否则裁剪回退矩形）。`shape=None` 时渲染期从链上最近 Background/Border 推断（render.rs:1062-1071）。
- 渲染（render.rs:1044-1166）两层模型（旧版 D:\winia ripple.rs）：
  - 背景状态层：节点中心圆（半径=对角线/2），alpha = `hover_opacity + focus_opacity`。
  - 前景按压层：按压点实心圆（半径=对角线×progress，直径=2×对角线保证角落全覆盖）；`layer.center` 为节点本地坐标（press 时已从场景坐标换算）——祖先 scroll translate 与自身 gl 变换都已作用在画布，波纹视觉自动跟随节点。
  - 裁剪：bounded → 节点形状（Circle 分支修正——此前落入 `_ => clip_rect` 被裁成矩形，IconButton 圆形容器波纹呈矩形）；unbounded → 背景圆。
  - 动画值（progress/opacity）由 MutableInteractionSource 提供，渲染期按时间计算、无额外动画状态。

### 8. Modifier 在测量/布局/渲染三阶段介入

- **测量/布局**（layout 引擎消费，本文件只提供查询）：`get_padding_sides`、`resolved_size`（modifier.rs:1490-1514，**合并所有 Size 元素**——⚠ 只取第一个会丢 `width(300).height(dyn)` 的动态高度）、`min_size_constraint`、`is_fill_max_width/height`、`get_layout_weight`、`aspect_ratio_constraint`、`required_size_constraint`、`get_align_self`、`get_offset/get_absolute_offset`、`image_intrinsic_size`、scroll states、`nested_scroll_connection`。**动态 SizeValue 在测量期求值——`State::get()` 注册布局依赖 → 动画更新 State → 节点重测（不重组）**。
- **组合期**：`register_state_deps`（modifier.rs:1837-1848）注册 scroll offset 的 State→Slot 依赖（新含 State 的变体必须同步加分支）。
- **渲染期**：render.rs 直接 `elements()` 匹配绘制；`graphics_layer_params`、`backdrop_blur_radius`、`get_padding_sides`、`focusable_interaction`（焦点环 alpha）。

### 9. skip_modifier 优化（param_eq）

- `Modifier::param_eq`（modifier.rs:2524-2543）+ `element_param_eq`（modifier.rs:2545-2628）+ `size_value_eq`（modifier.rs:2660-2667），供 `start_restartable_group` 的 Skip 判定。
- 规则：**数值/枚举/字符串/颜色参数精确比较**；**闭包类元素视为相同**——背景色/点击回调/GL 动态参数/滚动状态/富文本样式（每次 build 重建的闭包无法比较，精确比会破坏 Skip——列表每行永不 Skip）。
- 语义：数值参数变化 → 不等 → Enter（content 重跑）；闭包参数变化 → 相同 → Skip（保持）。动态尺寸（SizeValue::Dynamic）视为相同——布局期 layout_dep 每帧重测已覆盖。交互源身份参与判定（换源需重入捕获新源，modifier.rs:2598）。

---

## 核心总结 + 当前已知风险/待办

**核心总结**：winia 是一套对标 Jetpack Compose 的声明式 UI 管线——不可变 enum 链式 Modifier（builder 模式，非 type-state）承载布局/绘制/输入三类元素，单阶段深度遍历 LayoutNode 树完成绘制；渲染器与后端（skiwin Vulkan/GL/CPU/D3D）通过 `&Canvas` 完全解耦，HiDPI 由 app.rs 侧 `canvas.scale(sf)` + Density 注入统一处理；阴影（drop_shadow 离屏+绘制时 blur、shadow(elevation) 双图层、GL shadowElevation SkShadowUtils 三套）、backdropBlur 即时 snapshot、GraphicsLayer 2D/3D 透视变换均已对齐 Compose 语义。动画体系依赖"渲染期求值 + 零重组"（`peek()`/闭包）与"测量期 `get()` 注册 layout_dep"双轨。

**已知风险/待办**：
1. backdropBlur 的已知限制（render.rs:1186-1188）：自身 alpha/scale/rotation 不作用于模糊层、旋转祖先下画回仅近似，demo 未覆盖。
2. `on_long_press` 在 up 时判定，与 Compose 即时触发语义有差异（modifier.rs:490-491）。
3. `aspect_ratio` 等用 `assert!`（modifier.rs:802）——发布构建会 panic，非优雅错误。
4. 相机距离钳制按中心枢轴估算（render.rs:95），自定义 transformOrigin 时边缘距离不可靠。
5. `render.rs` 主循环是线性匹配所有元素，长链节点（TextFieldVisual/last_background 合并等）分支密集，性能优化空间（预扫描+主循环双遍历）待评估。
6. GraphicsLayer 命中测试不考虑变换（modifier.rs:1950-1953）——视觉位置与可点击区域在 scale/translate 后可分离，属设计决策但需在文档/测试中明确。
