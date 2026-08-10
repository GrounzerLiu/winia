# Image 组件：API、实现细节与未实现项

> 目标：对标 Compose foundation `Image`，在 winia 上提供等价的声明式 API
> 与布局/绘制语义。
> 本文档随实现演进，改动 API 或行为时同步更新。

## 1. API 总览

### 1.1 构造

| 构造 | 等价 Compose | 说明 |
|---|---|---|
| `Image::new(source: IconSource)` | `Image(bitmap/painter)` | 复用 `IconSource`（统一解码缓存） |
| `Image::file(path)` | `Image(ImageBitmap)` | 图片文件（png/jpg/jpeg/webp/bmp/svg，按扩展名解码） |
| `Image::svg(data)` | `Image(ImageVector)` | SVG 文档字符串 |

### 1.2 链式参数

| 方法 | 对标 | 说明 |
|---|---|---|
| `modifier(Modifier)` | `modifier` | 尺寸覆盖（size/fillMaxWidth 等）；未指定维度按固有尺寸 |
| `content_scale(ContentScale)` | `contentScale` | 缩放模式（默认 Fit） |
| `alignment(ImageAlignment)` | `alignment` | 内容在 bounds 内对齐（默认 Center，Start/End 随 RTL 镜像） |
| `alpha(f32)` | `alpha` | 整体透明度（默认 1.0；越界自动 clamp 0..1） |
| `content_description(String)` | `contentDescription` | a11y 描述（winia 无 semantics 树——预留参数，不影响渲染） |

### 1.3 值类型

- `ContentScale`（对标 Compose `ContentScale`）：
  - `None`：拉伸填满（不保持宽高比）；
  - `Fit`：完整放入（默认）；
  - `Crop`：覆盖 bounds（超出裁剪）；
  - `Inside`：保持比例且不超过 bounds（缩小不放大）；
  - `FillWidth`：宽填满、高按比例（可超出，裁剪）；
  - `FillHeight`：高填满、宽按比例（可超出，裁剪）。
- `ImageAlignment`（对标 Compose `Alignment` 9 值）：
  TopStart / TopCenter / TopEnd / CenterStart / Center / CenterEnd /
  BottomStart / BottomCenter / BottomEnd；Start/End 随布局方向镜像。

## 2. 实现细节

### 2.1 布局语义

- **固有尺寸布局**：叶子节点测量 = 位图固有像素尺寸 / SVG viewBox 尺寸，
  受 incoming 约束钳制（与 Text 测量同构）；modifier 的 size/fillMax 覆盖。
- 布局尺寸不含 padding 内边距（与其他叶子一致，测量回加）。

### 2.2 渲染

- 位图：`content_scale_rect`（缩放 + 对齐，单一事实来源）→ 线性采样
  （`draw_image_rect_with_sampling_options`，三线性防锯齿/摩尔纹）→ alpha。
- **clipToBounds**：内容超出 bounds（Crop/FillWidth/FillHeight 等比放大）
  时按几何判断裁剪（Fit/Inside 不超出则零开销）——对齐 Compose `clipToBounds()`。
- SVG：简化 Fit + Center（`draw_svg_dom` 内部 fit；content_scale/alignment
  暂不生效）；alpha 生效（saveLayer）。
- 绘制于内容区域（padding 内缩——与其他叶子内容一致）。

### 2.3 增量重组

- `ctx.changed(&self.source)`：source 变化 → 重测；content_scale/alignment/
  alpha 仅影响绘制（渲染每帧全量执行），不声明 changed。
- `ImageContent` 参与 `param_eq`（四字段全比较）与 Debug。

## 3. 与 Compose foundation Image 的差距

- **`colorFilter`**：未实现（Compose 支持染色）。
- **`filterQuality`**：未实现（固定线性采样，Compose 默认 Low 可配置）。
- **SVG 来源的 content_scale/alignment**：暂按 Fit + Center（位图完整支持）。
- **`contentDescription`**：winia 无 semantics 树（全框架缺口），参数预留。
- **Painter 抽象**：无（Compose 的 Painter 接口）；winia 用 `IconSource` 表达。

## 4. 维护约定

- 新增参数：默认值放 builder；绘制类参数（scale/alignment/alpha）不声明
  `changed`；同步补 getter、单元测试与本文档。
- 修改缩放/裁剪语义时保持"对齐 Compose `ContentScale`/`clipToBounds` 优先"。
