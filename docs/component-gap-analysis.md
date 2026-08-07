# 组件属性与 Modifier 差距分析（对标 Jetpack Compose）

> 分支：`component-polish`（从 text-field 分出）
> 目标：补全已有组件的属性与 Modifier，对齐 Compose 常用 API 语义
> 原则：机制统一不旁路；低成本先做；向量/枚举语义对标 Compose

## 一、现状盘点

### Modifier 已有（winia/src/modifier.rs）
- 布局：`size`/`width`/`height`（SizeValue 静态/动态）、`padding`(+h/v)、`fill_max_*`、`offset`（动态值 + 单轴 + RTL 镜像）、`align_self`、`layout_weight`、`aspect_ratio`、`required_size`/`required_width`/`required_height`
- 绘制：`background`（Color/闭包 + Shape）、`border`、`clip`、`blur`、`backdrop_blur`、`shadow`（elevation/shape/颜色）
- 交互：`clickable`/`clickable_with_source`、`focusable`/`focusable_with_source`、`hoverable`、`on_key_event`/`on_pre_key_event`、`on_pointer_event`/`on_pre_pointer_event`、`focus_requester`、手势 `on_tap`/`on_double_tap`/`on_long_press`/`on_drag_start`/`on_drag`/`on_drag_end`/`on_drag_cancel`
- 视觉：`alpha`/`rotate`/`scale`/`rotation_x`/`rotation_y`/`camera_distance`/`shadow_elevation` 便捷包装、`graphics_layer`（scale_x/y、alpha、translation_x/y、rotation_z、**transformOrigin + clip + 3D 旋转透视 + 阴影已补**）、`test_tag`
- 指示：`ripple(source, color, bounded)`——水波纹（对标 Compose indication/ripple；Button 自动附带）
- 滚动：`vertical_scroll`/`horizontal_scroll`（ScrollState）

### 组件已有
Text / TextField / Button / Column / Row / Stack / RichText / SelectionContainer / AnimatedVisibility / AnimatedContent / AnimatedSize / Crossfade / Window / **Popup / Dialog / DropdownMenu / DropdownMenuItem**

## 二、Modifier 差距（按优先级）——**已对照 Compose 1.11.4 源码核实**

### P0 — 常用便捷（低成本，纯包装/新元素）
1. ✅ **`alpha(a)` / `rotate(deg)` / `scale(sx, sy)`**——graphics_layer 直接包装，**对标源码语义**：
   - `alpha(a)`：`a != 1.0f` 时 `graphicsLayer(alpha = a, clip = true)`（**alpha<1 隐式裁剪到 bounds**）；范围 0..1
   - `rotate(degrees)`：`degrees != 0` 时 `graphicsLayer(rotationZ = degrees)`（**绕中心**）
   - `scale(sx, sy)` / `scale(s)`：非 1 时 `graphicsLayer(scaleX, scaleY)`（**绕中心**）
   - ✅ transformOrigin（P1-7）已补——变换**默认绕中心**（transformOrigin=Center），语义与 Compose 一致
2. ✅ **`shadow(elevation, shape = RectangleShape, clip = elevation > 0.dp, ambientColor = Black, spotColor = Black)`**——`elevation > 0 || clip` 才应用（否则返回 this）——对标 DropShadowPainter
3. ✅ **`aspect_ratio(ratio, match_height_constraints_first = false)`**——ratio 必须 > 0（前置校验）——按 cross 轴推导 main 轴
4. ✅ **`required_size(size)`**——SizeElement(min=max=size, **enforceIncoming=false**——incoming constraints 不强制，子内容可溢出）；另有 requiredWidth/requiredHeight
5. ✅ **`test_tag(tag)`**——对标 testTag（Compose 基于 semantics——本项目无 semantics 系统：实现为独立 modifier 元素 + 调试树暴露）
6. ✅ **`offset(x, y)`**——**布局期**（已核实：Compose Modifier.offset 是 LayoutModifierNode 布局期位移——项目现有实现一致）；已补动态值 + 单轴 + **RTL 镜像语义**（absoluteOffset 待 RTL 便捷 API 时补）

### P1 — GraphicsLayer 补属性（渲染层能力）
7. ✅ **transformOrigin（pivotFractionX/Y，默认 Center (0.5, 0.5)）**——**关键**：Compose 变换默认绕中心，项目当前绕左上——语义偏差，P0 便捷包装依赖它（已实现并修正）
8. ✅ **shadowElevation / ambientShadowColor / spotShadowColor**（graphicsLayer shadowElevation + shadow_shape——复用 Modifier.shadow 的 ambient/spot 实现）
9. clip ✅（graphics_layer 内裁剪到节点 bounds）；**shape 裁剪仍缺**（GraphicsLayerParams 无 shape 字段）
10. ✅ **rotationX/Y / cameraDistance**（3D 透视——M44 + Canvas::concat_44，相机透视 w'=1-z/d 近似）

### P2 — 状态与交互
11. ✅ **enabled 语义**（组件禁用——clickable 不响应 + 视觉降透明度）——Button/TextField 已独立实现；统一语义见 12
12. ✅ **InteractionSource / ComponentState**（`MutableInteractionSource`：press/focus/hover/drag 四态 + hoist + 派生状态读取；Button 状态取色/阴影、TextField focus/isError 已接入；Modifier.clickable/focusable/hoverable 自动发射）
13. **hoverable ✅（`Modifier.hoverable` + clickable 内部自动 hover）；draggable / pointer_input 仍远期**（tap/double-tap/long-press/drag 已实现）

## 三、组件属性差距——**已对照 material3 1.4.0 / foundation 1.11.4 源码核实**

### Text（对标 foundation BasicText + ui-text TextStyle）
| Compose 签名 | 现状 | 缺口 |
|---|---|---|
| `BasicText(text, style, overflow=Clip, softWrap=true, maxLines=MAX, minLines=1, color)` | 部分 | minLines（小） |
| TextStyle: fontSize/weight/style/family/color | 有 | — |
| TextStyle: **letterSpacing** / **lineHeight** | ✅ 已加（TextContent 元素全链路） | — |
| TextStyle: textDecoration / textShadow / background | RichText Style 有 | Text 便捷缺（中优先） |
| textAlign | 有 | — |

### Button（对标 material3 `Button(onClick, modifier, enabled=true, shape, colors, elevation, border, contentPadding, interactionSource)`）
| 缺口 | 说明 |
|---|---|
| **enabled** | ✅ 已实现：禁用：容器色/内容色切换（colors.containerColor(enabled)）+ 不响应点击 |
| **colors**（ButtonColors: container/content + disabled 变体） | ✅ 已实现（默认从主题按 style 生成） |
| **elevation**（ButtonElevation: shadowElevation 随 enabled/interaction 变化） | ✅ 已实现（`Button::elevation` + `ButtonElevation::for_state`——press/hover/focus/disabled 阴影；`ButtonElevation::elevated()` 近似 ElevatedButton） |
| **border / contentPadding** | border 可经 modifier；contentPadding 固定 | 
| **interactionSource** | P2-12 后接 |

### TextField（对标 material3 `TextField(value, onValueChange, enabled=true, readOnly=false, label, placeholder, leadingIcon, trailingIcon, prefix, suffix, supportingText, isError, visualTransformation, keyboardOptions, singleLine=false, maxLines=MAX, minLines=1, colors)`）
| 缺口 | 说明 |
|---|---|
| **enabled / readOnly** | ✅ 已实现：禁用不响应；只读不可编辑仍可选 |
| **label / placeholder** | **@Composable (() -> Unit)**（非 String）——placeholder 空值时显示 |
| **isError** | ✅ 已实现（错误文字色；`is_error` 状态可驱动边框色——需提升 interactionSource） |
| **多行（singleLine=false 默认、maxLines、minLines）** | ✅ 已实现：换行测量 + 光标移动支持（基础已有） |
| leadingIcon/trailingIcon/prefix/suffix/supportingText | 中优先（inline 内容） |

### Column/Row/Stack（对标 foundation-layout Column/Row/Box）
| Compose 签名 | 结论 |
|---|---|
| `Column(modifier, verticalArrangement, horizontalAlignment)` | **无 contentPadding 参数**（核实：只有 modifier/arrangement/alignment）——**文档此前想当然，删除**；contentPadding 是 Material 组件层（Button 等）的概念 |
| `Row(modifier, horizontalArrangement, verticalAlignment)` | 同上 |
| `Box(modifier, contentAlignment)` | 项目 Stack 等价 ✓ |

## 四、实施顺序建议

**✅ 已完成（component-polish 分支）**：
1. ✅ transformOrigin（默认绕中心——Compose 语义修正）+ GraphicsLayerParams.clip
2. ✅ Modifier.alpha/rotate/scale 便捷包装（合并语义 + alpha 隐式 clip）
3. ✅ Modifier.aspect_ratio（约束 max 推导 + match_height_first）+ required_size/width/height（enforceIncoming=false）
4. ✅ Modifier.test_tag（调试树 tag 字段）
5. ✅ Modifier.shadow（elevation/shape/clip/颜色——模糊垫底）
6. ✅ Button enabled + ButtonColors（container/content/disabled 变体）
7. ✅ TextField enabled/readOnly/placeholder
8. ✅ Text letterSpacing/lineHeight（TextContent 元素全链路）
9. ✅ 多行 TextField（singleLine/maxLines/minLines）
10. ✅ frame_clock 并行 flaky 双根因修复（旧帧丢弃同步化 + 首次 0ns 兜底）
11. ✅ RTL 布局方向支持 + 非对称/动态 padding（组合期捕获方向到 desc——修复 RTL 切换后 offset/padding 镜像失效）
12. ✅ Modifier.offset 对标 Compose——动态值 + 单轴 + RTL 镜像语义
13. ✅ 手势识别层——tap/double-tap/long-press/drag（对标 Compose detectTapGestures/detectDragGestures；Modifier.on_tap/on_double_tap/on_long_press/on_drag_*）
14. ✅ 顶层弹出组件——Popup / Dialog / DropdownMenu（模态遮罩、点击外部 dismiss、按 id 保留 State）
15. ✅ overlay 渲染 HiDPI 修复——内容按 scale 绘制，可见位置与命中测试对齐
16. ✅ Popup 锚定到调用位置的上一个兄弟节点（对标 Compose Popup 定位；无兄弟回退窗口对齐）
17. ✅ InteractionSource/ComponentState（press/focus/hover/drag + hoist）+ Button 状态取色/ButtonElevation + TextField focus/isError + Modifier.hoverable（对照 Compose foundation 1.11.4 源码）
18. ✅ 水波纹 indication（ripple）——分层设计（每次按压一层 RippleLayer，扩散 500ms EaseOutCubic + 释放淡出 300ms 后清理；参考旧版 D:\winia ripple.rs 的时长/透明度/分层思路）+ hover/focus 状态层（8%/12%，500ms 平滑过渡）；Button 自动附带
19. ✅ GraphicsLayer 3D：rotationX/rotationY + cameraDistance（M44 透视）+ shadowElevation/shadow_shape（复用 Modifier.shadow ambient/spot）+ 便捷 Modifier 方法

**剩余**：
- TextField label（Composable 浮动——需动画支持，P2）
- 手势层 draggable/pointer_input（远期；tap/double-tap/long-press/drag、hoverable 已完成）
- TextField 内置容器/边框视觉（Outlined/Filled 变体——当前视觉由用户 modifier 提供，isError/focus 状态可驱动）
