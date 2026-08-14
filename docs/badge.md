# Badge / BadgedBox 组件（material3 对齐）

> 分支：`badges`（从 v2 分出）
> 对标：material3 1.4.0 `Badge` / `BadgedBox`（Compose androidx-main Badge.kt）
> M3 规格：https://m3.material.io/components/badges/specs

## 1. API

```rust
// 小徽章（无内容——6×6 圆点）
Badge::new().build(ctx);

// 大徽章（数字/短文本）
Badge::new()
    .content(|ctx| { Text::new("3").build(ctx); })   // 对标 content（RowScope）
    .container_color(c)                              // 对标 containerColor（默认 Error）
    .content_color(c)                                // 对标 contentColor（默认 OnError）
    .modifier(Modifier)                              // 外部修饰符
    .build(ctx);

// BadgedBox：badge 挂载到 anchor（图标等）右上角
BadgedBox::new(|ctx| { Badge::new().build(ctx); })   // 对标 badge 参数
    .modifier(Modifier)
    .build(ctx, |ctx| { /* anchor 内容（图标等） */ });
```

- `Badge::content()` 设置后为大徽章（min 16×16、圆角 8、水平 padding 4、
  LabelSmall 文本 11sp/Medium/OnError）；无内容为小圆点（6×6、圆角 3）。
- `BadgedBox` 的 badge 闭包是构造参数（必填），anchor 是 build 内容闭包
  （与其他容器组件一致）。
- 徽章宽 > 6dp 视为有内容（Compose `badgePlaceable.width > BadgeTokens.Size`）
  ——偏移随之切换。

## 2. 默认值（对标 `BadgeTokens` v0_103 / M3 specs）

| 项 | 小徽章 | 大徽章 |
|---|---|---|
| 尺寸 | 6×6（`Size`） | min 16×16（`LargeSize`） |
| 圆角 | 3（`CornerFull`） | 8（`CornerFull`） |
| 水平 padding | — | 4（`BadgeWithContentHorizontalPadding`） |
| 容器色 | Error | Error（`LargeColor`） |
| 内容色 | — | OnError（`LargeLabelTextColor`） |
| 文本样式 | — | LabelSmall（11sp / Medium 500） |
| 最大字符形态 | — | 16×34dp（M3：99+ 等） |

### BadgedBox 定位（对齐 Compose Badge.kt 布局）

| 形态 | 水平偏移 | 垂直偏移 | 语义 |
|---|---|---|---|
| 无内容 | 6dp | 6dp | 徽章左下角距锚点右上角 6×6（`BadgeOffset`） |
| 有内容 | 12dp | 14dp | 左缘距锚点右缘 12、底边距锚点顶 14（`BadgeWithContent*Offset`） |

## 3. 实现细节

- **Badge**：min 尺寸 + `background`（CornerFull 形状）+ 内容水平 padding，
  有内容时 `WiniaTheme::with_content_color` + `ProvideTextStyle`（LabelSmall）
  包裹 Row（垂直居中）——对齐 Compose `ProvideContentColorTextStyle`。
- **BadgedBox**：自定义 `MeasurePolicy`（`BadgedBoxPolicy`）——anchor 全约束
  测量、badge 宽松高度测量（`constraints.loosen()`，文本不占多余高度）；
  尺寸 = anchor 尺寸；badge 定位 `(anchorW - offset_x, -badgeH + offset_y)`
  （徽章可越出锚点顶部/右侧——渲染与命中测试均支持负坐标）。
- 组合结构：`start_container`（自定义 policy）→ 两个 `start_restartable_group`
  （anchor 居中 / badge 居中），各自独立 Skip。
- 徽章底色/内容色为静态色（无动画——徽章不承载状态变化，Compose 同）。
- 像素/结构测试 6 个：默认色、小徽章定位 (18,0)、大徽章定位 (12,-2)、
  小徽章 Error 底色、大徽章底色+文字像素、BadgedBox 渲染级验证。

## 4. 未实现项（对齐 Compose 差距）

- `BadgeTopRuler` / `BadgeEndRuler`（导航栏边界钳制——徽章不超出导航栏）；
  winia 无 NavigationBar 组件，徽章按锚点定位（若后续实现导航栏需补）；
- `defaultMinSize` 语义近似：winia `min_width`/`min_height`（不强制 min，
  内容超宽时徽章自然变宽——M3 最大字符形态 16×34 由内容宽度驱动 ✓）；
- `ProvideContentColorTextStyle` 的 textStyle 下传（LabelSmall）已近似实现；
- 无障碍语义（badge 内容朗读）——winia 暂无 semantics 系统。
