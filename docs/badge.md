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
- **内容色**：默认按 `contentColorFor(containerColor)` 从容器色推导（`error → onError`、
  `primary → onPrimary`、`surfaceContainerHigh → onSurface` …），非主题色则回退到
  `LocalContentColor`；`.content_color(c)` 显式覆盖两者。
- **caller modifier 在最外层**：`Badge(modifier = Modifier.padding(10.dp))` 的 padding
  在徽章背景**之外**（Compose 链为 `modifier.defaultMinSize().background().padding()`）。

## 2. 默认值（对标 `BadgeTokens` v0_103 / M3 specs）

| 项 | 小徽章 | 大徽章 |
|---|---|---|
| 尺寸 | 6×6（`Size`） | min 16×16（`LargeSize`） |
| 圆角 | 3（`CornerFull`） | 8（`CornerFull`） |
| 水平 padding | — | 4（`BadgeWithContentHorizontalPadding`） |
| 容器色 | Error | Error（`LargeColor`） |
| 内容色 | — | `contentColorFor(containerColor)`（M3 `LargeLabelTextColor` = OnError 仅为 Error 容器的结果） |
| 文本样式 | — | LabelSmall（11sp / Medium 500） |
| 最大字符形态 | — | 16×34dp（M3：99+ 等） |

> 圆角用固定值 3 / 8 而非百分比 `CornerFull`：`min_height = 16` 保证短边 ≥ 16，
> 单字符 16×16 的短边一半恰为 8、圆点 6×6 恰为 3，与 `CornerFull` 在每个
> 文档尺寸上都重合（多行 16×32、font 60 的 42×16 也仍为 8）。

### BadgedBox 定位（对齐 Compose Badge.kt 布局）

| 形态 | 水平偏移 | 垂直偏移 | 语义 |
|---|---|---|---|
| 无内容 | 6dp | 6dp | 徽章左下角距锚点右上角 6×6（`BadgeOffset`） |
| 有内容 | 12dp | 14dp | 左缘距锚点右缘 12、底边距锚点顶 14（`BadgeWithContent*Offset`） |

## 3. 实现细节

- **Badge**：`default_min_size` 尺寸 + `background`（CornerFull 形状）+ 内容水平 padding，
  有内容时 `WiniaTheme::with_content_color` + `ProvideTextStyle`（LabelSmall）
  包裹 Row（垂直居中）——对齐 Compose `ProvideContentColorTextStyle`。
  内容色变化时用 backchannel 记上帧值并在变动时 `mark_subtree_dirty()`：
  `CompositionLocal` 不会让读者失效，内层 Row 组又只声明 arrangement/alignment/spacing，
  仅靠外层 Enter 到达不了标签（`WiniaTheme::provide_resolved` 对主题切换记录的同一缺陷）。
- **BadgedBox**：自定义 `MeasurePolicy`（`BadgedBoxPolicy`）——anchor 全约束
  测量、badge 宽松高度测量（`constraints.loosen_height()`——Compose 是
  `constraints.copy(minHeight = 0)`，**只放松高度、不放松宽度**，文本不占多余高度）；
  尺寸 = anchor 尺寸；badge 定位 `(anchorW - offset_x, -badgeH + offset_y)`
  （徽章可越出锚点顶部/右侧——渲染与命中测试均支持负坐标）。
- 组合结构：`start_container`（自定义 policy）→ 两个 `start_restartable_group`
  （anchor 居中 / badge 居中），各自独立 Skip。
- 徽章底色/尺寸为静态值（无动画——徽章不承载状态变化，Compose 同）；内容色会随
  容器色与主题色移动（见上）。
- 测试 15 个：结构/定位（小徽章 (18,0)、大徽章 (12,-2)、RTL 镜像）、对齐（内容色推导、
  基线只报 anchor、min 宽度只放松高度）、响应式（容器色运行时移动后标签跟随、显式
  内容色生效）、caller modifier 语义（可缩小到 2dp、无覆盖时用 token 尺寸）、
  像素（小徽章 Error 底色、大徽章底色+文字、BadgedBox 渲染级验证）。

## 4. 未实现项（对齐 Compose 差距）

- `BadgeTopRuler` / `BadgeEndRuler`（导航栏边界钳制——徽章不超出导航栏）；
  winia 无 NavigationBar 组件，徽章按锚点定位（若后续实现导航栏需补）；
- **content 的 `RowScope` 接收者**：Compose 是 `@Composable (RowScope.() -> Unit)?`，
  winia 是 `Box<dyn FnOnce(&mut ComposeCtx)>`——winia 没有 `RowScope` 类型（`Row`
  也没有 `weight`），内容无法使用 Row 作用域能力。这是缺失的整个 scopes 体系，非 badge 局部；
- `ProvideContentColorTextStyle` 的 textStyle 下传（LabelSmall）已近似实现；
- 无障碍语义（badge 内容朗读）——winia 暂无 semantics 系统。
