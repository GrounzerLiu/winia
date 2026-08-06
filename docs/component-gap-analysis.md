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

## 二、Modifier 差距（按优先级）

### P0 — 常用便捷（低成本，纯包装/新元素）
1. **`alpha(a)` / `scale(sx, sy)` / `rotate(deg)` / `offset(x, y)`**——graphics_layer 的直接包装（对标 Compose Modifier.alpha/scale/rotate/offset）
   - ⚠ offset 与现有布局 `offset()` 冲突（现有是布局期位移）——Compose 的 `offset()` 是绘制期变换——**新增 `graphics_offset`？**——不——**直接包装 graphics_layer 的翻译**（布局 offset 保留）
2. **`shadow(elevation, shape, color)`**——对标 Compose Modifier.shadow（GraphicsLayer 阴影 + 裁剪——skia 实现：画模糊矩形底 + 内容）
3. **`aspect_ratio(ratio)`**——宽高比约束（布局期：按 cross 轴推导 main 轴）
4. **`required_size(w, h)`**——强制尺寸（无视 constraints 约束，clip 溢出）
5. **`test_tag(tag)`**——测试标记（UI 测试定位——debug 树暴露 tag 字段）
6. **`semantics`/`content_description`**——无障碍（基础版：标记 + 树暴露）

### P1 — GraphicsLayer 补属性（渲染层能力）
7. **pivot / transformOrigin**（变换原点——默认左上 vs Compose 中心）
8. **shadow_elevation / ambient / spot 颜色**（与 Modifier.shadow 合并实现）
9. **clip + shape**（graphics_layer 内裁剪）
10. **rotation_x/y / camera_distance**（3D 透视——skia 支持有限，可降级）

### P2 — 状态与交互
11. **enabled 语义**（组件禁用——clickable 不响应 + 视觉降透明度）——Button/TextField 先接
12. **InteractionSource / ComponentState**（hover/press/focus 状态聚合——Button 波纹/颜色变化的依据——**大工程，先做最小版**：`MutableInteractionSource` + 状态读取）
13. **hoverable / draggable / pointer_input**（手势层——**远期**，需 pointer 事件管道增强）

## 三、组件属性差距

### Text（winia/src/ui/text.rs）
| Compose | 现状 | 缺口 |
|---|---|---|
| maxLines / overflow / softWrap | 有（text_content 参数） | — |
| textAlign | 有（TextAlign） | — |
| fontSize / fontWeight / fontStyle / fontFamily | 有 | — |
| letterSpacing | 无 | 加（skia 支持） |
| lineHeight | 无 | 加（Paragraph 支持） |
| textDecoration / textShadow | RichText Style 有 | Text 便捷缺 |
| color | 有 | — |

### Button（winia/src/ui/button.rs）
| Compose | 现状 | 缺口 |
|---|---|---|
| enabled | 无 | **加**（禁用：不响应点击 + alpha 0.5 + 无波纹） |
| onClick | 有（modifier clickable） | — |
| colors（container/content） | 硬编码主题色 | **加**（ButtonColors 覆盖） |
| shape / border / elevation | shape 可经 modifier | 便捷缺 |
| contentPadding | 固定 | **加** |

### TextField（winia/src/ui/text_field.rs）
| Compose | 现状 | 缺口 |
|---|---|---|
| value / onValueChange | 有（State<String> + 回调） | — |
| label / placeholder | 无 | **加**（placeholder 文字显示在空值时） |
| enabled / readOnly | 无 | **加** |
| singleLine / maxLines | 单行 | **加** multiLine 支持（换行高度已有基础） |
| isError / colors | 无 | **加**（错误边框/文字色） |
| leadingIcon / trailingIcon | 无 | 远期（inline 内容） |

### Column/Row/Stack（winia/src/ui/layout_components.rs）
| Compose | 现状 | 缺口 |
|---|---|---|
| horizontalArrangement / verticalAlignment | 有（Arrangement/Alignment） | — |
| contentPadding | 无 | **加**（内部 padding，避免包一层） |
| reverseLayout | 无 | 远期 |
| Stack: alignment | 有 | — |

### 通用
- **disabled 组件统一处理**（alpha + 事件屏蔽）

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
