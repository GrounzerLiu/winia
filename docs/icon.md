# Icon 组件：API、实现细节与未实现项

> 设计目标：多来源、可动画、可控镜像的图标组件。参考 Compose `Icon`，
> 但不照搬——默认不内置任何图标，支持 SVG path / 完整 SVG / 图片文件 /
> 可变字体四种来源。

## 1. 设计原则

- **默认无图标**：不开 feature 时没有内置图标集（二进制里不含字体）。
- **不自研 SVG 解析**：所有 SVG 渲染统一走 Skia 内置 `svg::Dom`。
  裸 path 数据（fonts.google.com/icons 复制）会被包成
  `<svg viewBox="0 0 24 24"><path d="..."/></svg>` 再交给 Dom。
- **tint 分来源**：`Tint::Auto`（默认）= 单色源（SvgPath/Svg/Symbol）
  染主题 `on_surface`；File 保留原色。`.tint(color)` 强制染，`.tint(None)` 强制不染。
- **autoMirror 是显式属性**：`.auto_mirror(true)` 后，图标在 RTL 布局方向下
  绕中心水平镜像；默认 false。
- **可变轴可动画**：`fill/grad/opsz/wght` 接受静态值或 `&State<f32>`/闭包，
  渲染期求值——动画只重绘不重组。

## 2. API

```rust
// 1) 裸 SVG path（Google Fonts 复制）
Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
    .tint(theme.primary)          // 默认 Auto：单色源染 on_surface
    .size(32.0)                   // 默认 24×24（SvgPath/Symbol）或源固有尺寸
    .build(ctx);

// 2) 完整 SVG 字符串
Icon::svg("<svg viewBox='0 0 24 24'><path d='...'/></svg>").build(ctx);

// 3) 图片文件（png/jpg/webp/bmp/svg）
Icon::file("assets/logo.svg").size(48.0).build(ctx);

// 4) 可变字体（--features material-symbols-outlined）
Icon::symbol(winia::icon::Outlined::FAVORITE)
    .fill(&fill_state)            // FILL 0..1 动画
    .wght(500.0)                  // 100..700
    .grad(25.0)                   // -50..200
    .opsz(32.0)                   // 20..48
    .build(ctx);

// RTL 镜像
Icon::svg_path(ARROW_BACK).auto_mirror(true).build(ctx);
```

常用链式方法：`tint` / `auto_mirror` / `size` / `content_description`
（当前存储，semantics 实现后接入）/ `modifier` / 四个轴方法（feature 启用时）。

## 3. 实现细节

### 3.1 渲染管线

| 来源 | 解码 | 绘制 |
|---|---|---|
| `SvgPath` | 包成最小 SVG 文档 → `svg::Dom` | 离屏 raster → tint（SrcIn）→ Fit 居中 |
| `Svg` | `svg::Dom`（viewBox 为固有尺寸） | 同上 |
| `File` | 按扩展名：svg → Dom；png/jpg/webp/bmp → `Image` | Dom 同上；位图 `draw_image_rect` + tint |
| `Symbol` | `TextBlob` + `FontArguments` 四轴 | `draw_text_blob`，Paint 着色 |

- `ModifierElement::DrawIcon` 承载 `IconSpec{source, tint, auto_mirror, axes}`，
  走现有 Skip/param_eq（动态轴视为相等，动画不触发重组）。
- 全局缓存：path 解码/文件解码按来源键缓存；Symbol 字形 blob 按
  `(symbol, size, fill, grad, opsz, wght)` 位级键缓存。
- 尺寸：显式 `.size()` > 源固有尺寸（SVG viewBox / 位图像素 / Symbol 24）
  > 回退 24×24。无 viewBox 的 SVG 会按 24×24 缩放（可能拉伸失真），
  建议 SVG 文档/文件都带 viewBox。

### 3.2 可变字体轴（官方范围）

| 轴 | 范围 | 说明 |
|---|---|---|
| FILL | 0..1 | 0 描边 / 1 填充 |
| GRAD | -50..200 | 低强调 -25，高强调 200 |
| opsz | 20..48 | 光学尺寸，建议跟随图标大小 |
| wght | 100..700 | 描边粗细 |

渲染前按上述范围钳制（常量 `FILL_RANGE` 等）。

### 3.3 Features

```toml
material-symbols-outlined = []   # 内嵌 Outlined 字体（~10MB）+ 4207 码点
material-symbols-rounded  = []   # Rounded（~14MB）
material-symbols-sharp    = []   # Sharp（~8.6MB）
```

默认全部关闭。码点常量形如 `winia::icon::Outlined::ADD` /
`Rounded::ARROW_BACK` / `Sharp::SEARCH`（4207 个/主题）。
字体与码点表来自 google/material-design-icons（Apache-2.0，见
`winia/src/icon/NOTICE`）。

## 4. 与 Compose 的差异 / 未实现

- 无内置默认图标；Compose 默认带 `Icons.Filled/Outlined/...`。
- 来源不同：Compose 是 `ImageVector`；本实现是 SVG 文档/文件 + 字体符号。
- `autoMirror` 是全局属性而非 AutoMirrored 变体（按需开启）。
- `contentDescription` 仅存储，semantics 树未实现（全框架缺口）。
- 可变轴动画逐帧生成新 TextBlob（有缓存但连续动画会持续 miss）；
  后续可做离屏栅格化/位图缓存。
- 文件缓存不监听 mtime 变化（进程内同一路径复用首次解码结果）。

## 6. IconButton（已实现）

对标 material3 `IconButton` 一族，见 `winia/src/ui/icon_button.rs`：

- 变体：`IconButton::new()`（标准）/ `filled()` / `filled_tonal()` /
  `outlined()`（默认 1px outline 边框）。
- 默认：48×48 圆形容器（M3 为 40dp 容器 + 48dp 触摸目标，本框架直接 48）、
  点击波纹、焦点环（主题 primary、圆形跟随）、禁用不响应。
- 颜色：`IconButtonColors`（container/content/disabled 变体），
  `IconButtonDefaults` 提供四套默认色（标准透明底 on_surface；
  Filled primary/on_primary；Tonal secondary_container；Outlined 透明底
  on_surface_variant）。
- **内容色下传**：新增 `WiniaTheme::content_color()`（对标
  `LocalContentColor`）——IconButton 用 `with_content_color` 包裹内容，
  `Icon` 的 `Tint::Auto` 会取容器提供的内容色（如 Filled 里图标自动
  on_primary）。
- 差异：内容色默认取主题 on_surface（Compose 默认 LocalContentColor）；
  disabled 容器 = onSurface 12%、disabled 内容 = onSurface 38%（M3 token）；
  hover/press 视觉反馈由波纹指示提供（M3 的状态层同样属于 indication，
  不在 IconButtonColors 四色模型内）。
- 已知边界：`with_content_color` 是 CompositionLocal，变化不注册依赖——
  若外层内容色变化而 IconButton 自身参数未变（Skip），内部 Icon 不会
  重解析 tint；改变颜色请通过 IconButton 的 `colors()` 参数触发。

## 5. Demo

```powershell
cargo run -p winia --example icon_demo
cargo run -p winia --example icon_demo --features material-symbols-outlined
```
