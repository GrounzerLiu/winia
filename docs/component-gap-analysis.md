# 组件属性与 Modifier 差距分析（对标 Jetpack Compose）

> 分支：`component-polish`（从 text-field 分出）
> 目标：补全已有组件的属性与 Modifier，对齐 Compose 常用 API 语义
> 原则：机制统一不旁路；低成本先做；向量/枚举语义对标 Compose

## 一、现状盘点

### Modifier 已有（winia/src/modifier.rs）
- 布局：`size`/`width`/`height`（SizeValue 静态/动态）、`padding`(+h/v)、`fill_max_*`、`offset`、`align_self`、`layout_weight`
- 绘制：`background`（Color/闭包 + Shape）、`border`、`clip`、`blur`、`backdrop_blur`
- 交互：`clickable`、`focusable`、`on_key_event`/`on_pre_key_event`、`on_pointer_event`/`on_pre_pointer_event`、`focus_requester`
- 视觉：`graphics_layer`（scale_x/y、alpha、translation_x/y、rotation_z——**无 pivot/transformOrigin/shadow/clip**）
- 滚动：`vertical_scroll`/`horizontal_scroll`（ScrollState）

### 组件已有
Text / TextField / Button / Column / Row / Stack / RichText / SelectionContainer / AnimatedVisibility / AnimatedContent / AnimatedSize / Crossfade / Window

## 二、Modifier 差距（按优先级）——**已对照 Compose 1.11.4 源码核实**

### P0 — 常用便捷（低成本，纯包装/新元素）
1. **`alpha(a)` / `rotate(deg)` / `scale(sx, sy)`**——graphics_layer 直接包装，**对标源码语义**：
   - `alpha(a)`：`a != 1.0f` 时 `graphicsLayer(alpha = a, clip = true)`（**alpha<1 隐式裁剪到 bounds**）；范围 0..1
   - `rotate(degrees)`：`degrees != 0` 时 `graphicsLayer(rotationZ = degrees)`（**绕中心**）
   - `scale(sx, sy)` / `scale(s)`：非 1 时 `graphicsLayer(scaleX, scaleY)`（**绕中心**）
   - ⚠ 以上变换**默认绕中心**（transformOrigin=Center）——当前项目 graphics_layer 绕左上——**需要先补 transformOrigin**（P1-7）语义才一致
2. **`shadow(elevation, shape = RectangleShape, clip = elevation > 0.dp, ambientColor = Black, spotColor = Black)`**——`elevation > 0 || clip` 才应用（否则返回 this）——对标 DropShadowPainter
3. **`aspect_ratio(ratio, match_height_constraints_first = false)`**——ratio 必须 > 0（前置校验）——按 cross 轴推导 main 轴
4. **`required_size(size)`**——SizeElement(min=max=size, **enforceIncoming=false**——incoming constraints 不强制，子内容可溢出）；另有 requiredWidth/requiredHeight
5. **`test_tag(tag)`**——对标 testTag（Compose 基于 semantics——本项目无 semantics 系统：实现为独立 modifier 元素 + 调试树暴露）
6. **`offset(x, y)`**——**布局期**（已核实：Compose Modifier.offset 是 LayoutModifierNode 布局期位移——项目现有实现一致，无需改）；absoluteOffset（rtlAware=false）待 RTL 支持时再补

### P1 — GraphicsLayer 补属性（渲染层能力）
7. **transformOrigin（pivotFractionX/Y，默认 Center (0.5, 0.5)）**——**关键**：Compose 变换默认绕中心，项目当前绕左上——语义偏差，P0 便捷包装依赖它
8. **shadowElevation / ambientShadowColor / spotShadowColor**（与 Modifier.shadow 合并实现）
9. **shape + clip**（graphics_layer 内裁剪）
10. **rotationX/Y / cameraDistance**（3D 透视——skia 支持有限，可降级）

### P2 — 状态与交互
11. **enabled 语义**（组件禁用——clickable 不响应 + 视觉降透明度）——Button/TextField 先接
12. **InteractionSource / ComponentState**（hover/press/focus 状态聚合——Button 颜色/阴影变化的依据——**大工程，先做最小版**：`MutableInteractionSource` + 状态读取）
13. **hoverable / draggable / pointer_input**（手势层——**远期**，需 pointer 事件管道增强）

## 三、组件属性差距——**已对照 material3 1.4.0 / foundation 1.11.4 源码核实**

### Text（对标 foundation BasicText + ui-text TextStyle）
| Compose 签名 | 现状 | 缺口 |
|---|---|---|
| `BasicText(text, style, overflow=Clip, softWrap=true, maxLines=MAX, minLines=1, color)` | 部分 | minLines（小） |
| TextStyle: fontSize/weight/style/family/color | 有 | — |
| TextStyle: **letterSpacing** / **lineHeight** | 无 | **加**（skia/Paragraph 原生支持） |
| TextStyle: textDecoration / textShadow / background | RichText Style 有 | Text 便捷缺（中优先） |
| textAlign | 有 | — |

### Button（对标 material3 `Button(onClick, modifier, enabled=true, shape, colors, elevation, border, contentPadding, interactionSource)`）
| 缺口 | 说明 |
|---|---|
| **enabled** | 禁用：容器色/内容色切换（colors.containerColor(enabled)）+ 不响应点击 |
| **colors**（ButtonColors: container/content + disabled 变体） | 现硬编码主题色 |
| **elevation**（ButtonElevation: shadowElevation 随 enabled/interaction 变化） | 无（结合 Modifier.shadow） |
| **border / contentPadding** | border 可经 modifier；contentPadding 固定 | 
| **interactionSource** | P2-12 后接 |

### TextField（对标 material3 `TextField(value, onValueChange, enabled=true, readOnly=false, label, placeholder, leadingIcon, trailingIcon, prefix, suffix, supportingText, isError, visualTransformation, keyboardOptions, singleLine=false, maxLines=MAX, minLines=1, colors)`）
| 缺口 | 说明 |
|---|---|
| **enabled / readOnly** | 禁用不响应；只读不可编辑仍可选 |
| **label / placeholder** | **@Composable (() -> Unit)**（非 String）——placeholder 空值时显示 |
| **isError** | 错误边框/文字色（colors 变体） |
| **多行（singleLine=false 默认、maxLines、minLines）** | 项目现单行——**Compose 默认多行**，需换行测量 + 光标移动支持（基础已有） |
| leadingIcon/trailingIcon/prefix/suffix/supportingText | 中优先（inline 内容） |

### Column/Row/Stack（对标 foundation-layout Column/Row/Box）
| Compose 签名 | 结论 |
|---|---|
| `Column(modifier, verticalArrangement, horizontalAlignment)` | **无 contentPadding 参数**（核实：只有 modifier/arrangement/alignment）——**文档此前想当然，删除**；contentPadding 是 Material 组件层（Button 等）的概念 |
| `Row(modifier, horizontalArrangement, verticalAlignment)` | 同上 |
| `Box(modifier, contentAlignment)` | 项目 Stack 等价 ✓ |

## 四、实施顺序建议

1. P0-1 Modifier 便捷包装（alpha/scale/rotate）——半天
2. P0-3 aspect_ratio + P0-4 required_size——布局层新约束
3. P0-5 test_tag——UI 测试增强
4. P0-2 shadow——渲染层
5. 组件：Button enabled+colors、TextField label/placeholder/enabled/readOnly、Text letterSpacing/lineHeight、布局 contentPadding
6. P2-11 enabled 语义统一
7. P2-12 InteractionSource 最小版
8. P1 GraphicsLayer 补属性（pivot/shadow/clip）
9. P2-13 手势层（远期）
