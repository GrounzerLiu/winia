# NavigationRail / WideNavigationRail 组件（material3 对齐）

> 对标：androidx-main `NavigationRail.kt`（785 行，Collapsed 基线）+
> `WideNavigationRail.kt`（1422 行，M3 Expressive 宽轨）+ `NavigationItem.kt`
> 的 Start 布局数学
> M3 规格：https://m3.material.io/components/navigation-rail/specs
> 源码：`winia/src/components/navigation_rail.rs`；示例：`examples/navigation_rail_demo.rs`

## 1. 变体总览

| 组件 | 对应 M3 | 宽度 | item 图标位置 | label |
|---|---|---|---|---|
| `NavigationRail` | Collapsed 基线 | 80dp | 上（Top） | 可选，alwaysShowLabel 控制 |
| `WideNavigationRail` | Expressive 宽轨 | **96 ↔ 220..360dp 动画**（见 §5） | 收起上/展开左（变形） | **必选恒显示** |
| `ModalWideNavigationRail` | Expressive 模态宽轨 | 收起同宽轨；展开抬 scrim 层 | 同上 | 同上 |

## 2. NavigationRail API

```rust
// 容器：Surface 底色、撑满高、垂直 padding 4、item spacedBy(4)、水平居中
NavigationRail::new(|ctx| { /* NavigationRailItem ×3~7 */ })
    .container_color(c)          // 默认 theme.surface（注意：非 surfaceContainer）
    .header(|ctx| { /* FAB / logo */ })   // 之后自动插入 8dp Spacer
    .window_insets(WindowInsets::new(l, t, r, b))  // 桌面默认全零（见 §8）
    .modifier(m)
    .build(ctx);

// item：图标在上、label 在下；胶囊 56×32 由图标推导
NavigationRailItem::new(selected, |ctx| { Icon... })
    .label(|ctx| Text::new("Home").build(ctx))
    .on_click(|| {})
    .enabled(true)
    .always_show_label(true)     // false 时未选中隐藏标签且图标居中插值
    .colors(rail_item_colors(&theme))
    .interaction_source(source)
    .modifier(m)
    .build(ctx);
```

- item **拥抱内容、最小宽 80dp**（androidx `widthIn(min=80)` 语义——不横向填满，
  此前误用导航栏等分语义导致兄弟节点被挤出窗口，见回归测试
  `bounded_item_hugs_content_instead_of_filling_width`）。
- item 高基准 **56dp**（= ActiveIndicatorWidth），非导航栏的 80。
- 颜色 token 与导航栏完全一致 → `NavigationRailItemColors` 为
  `NavigationBarItemColors` 的类型别名（`rail_item_colors(&theme)` 构造）。

## 3. 指示器几何（有/无 label 分支）

| 项 | 有 label | 无 label |
|---|---|---|
| 纵向内边距 | (32−24)/2 = **4** | (56−24)/2 = **16** |
| 胶囊尺寸 | 56×32 | **56×56 圆形**（CornerFull） |
| 内容排布 | 图标上/标签下，间距 4+4 | 全部居中（placeIcon） |

- 胶囊宽 = iconW + 2×16；选中展开动画由 sizeProgress（stiffness 400）驱动。
- 未选中 alwaysShowLabel=false 时图标居中插值（offset = iconDistance × (1−p)）。

## 4. 悬浮状态层 / ripple（对齐 androidx IndicatorRipple 分离设计）

- 子节点顺序 `[indicator, icon, label?, ripple]`——ripple **最后放置（z 最上层）**，
  状态层覆盖在彩色胶囊与内容之上（选中/未选中悬浮均有状态层）。
- ripple 节点**恒定全尺寸**：未选中时彩色胶囊收拢为 0 宽，但悬浮热区仍是完整
  胶囊矩形，且跟随指示器的动画位置（与可见图标对齐）。
- hover enter/exit 经交互源驱动 hover_opacity（500ms tween）；渲染期裁剪为 Pill。
- 无障碍/交互链路：`clickable_with_source` 自带 Hoverable——无需额外接线。

## 5. WideNavigationRail（Expressive 宽轨）

```rust
let state = WideNavigationRailState::new(ctx);   // 默认收起
state.expand(); state.collapse(); state.toggle(); // 目标切换（同步简化版）

WideNavigationRail::new(state.clone(), |ctx| { /* items */ })
    .header(|ctx| { /* 菜单/FAB */ })
    .container_color(c)
    .build(ctx);

// item：label 必选且恒显示；progress 绑定容器展开进度（0=Top 布局 1=Start 布局）
WideNavigationRailItem::new(selected, |ctx| { Icon... }, |ctx| { Text... })
    .progress(progress_state)
    .on_click(|| {})
    .build(ctx);
```

### 5.1 ModalWideNavigationRail（模态宽轨）

折叠时就是一条普通折叠宽轨、留在布局流内；展开时走 `open_overlay(modal: true)`
抬起一层带 scrim 的浮层。上游用 `ModalWideNavigationRailDialog` 做同一件事。

```rust
let state = WideNavigationRailState::new(ctx);
ModalWideNavigationRail::new(state.clone(), |ctx| { /* items */ })
    .header(|ctx| { /* 菜单/FAB */ })
    .hide_on_collapse(false)        // true = dismissible 变体，折叠时完全不画
    .collapsed_shape(Shape::Rectangle)
    .expanded_shape(modal_rail_expanded_shape())   // CornerLarge 16dp
    .container_color(c)             // 折叠态，默认 surface
    .modal_container_color(c)       // 展开态，默认 surfaceContainer
    .expanded_header_top_padding(64.0)   // 见下方"对齐"
    .build(ctx);
```

- 内容闭包是 `Fn` 不是 `FnOnce`——overlay 每帧重新组合它的内容。

**折叠态与展开态的 item 对齐**（`expandedHeaderTopPadding`，`WideNavigationRail.kt:527`、
`:666`）：上游对这个参数的说明是"通常用于**让展开动画的内容与折叠态对齐**"
（`:508-510`）。两种状态的纵向节奏本来就不同：

| | 收起 | 展开 |
|---|---|---|
| item 高 | 64 | 56 |
| item 间距 | 4 | 0 |
| **节距** | **68** | **56** |

所以**逐项一一对应在数学上不可能**，两端每项差 12dp（实测：首项 delta 0，
第 2/3/4 项 12/24/36）。能对齐的是**首项**，`expanded_header_top_padding` 就是干这个的——
它加在展开面板的 header 槽上（因此经 `headerOffset = header高度 + HeaderSpaceMinimum(40)`
把首项推下去）。上游两处用法都传 `64.dp`（`NavigationRailSamples.kt:206`、
`NavigationSuiteScaffoldDemo.kt:164`），值是"面板要跨过的顶栏高度"，所以文档说它取决于布局。

两个前提才能对上：

1. **轨道必须在窗口前导边**——面板永远开在窗口前导边（`TopStart`），轨道放在别处就不可能
   被它盖住。上游 sample 把模态轨放在 `Row` 的第一个子节点。
2. **展开面板的 header 槽即使调用方没给 header 也存在**——上游的调用点是
   `header = { Box(Modifier.padding(top = ...)) { header?.invoke() } }`，lambda 恒非 null，
   所以布局总会量到一个 header placeable，padding 才带得动 item。winia 同构。

实测（900dp 窗口，Scaffold 顶栏 64dp）：首项 window y 两边都是 204，delta = 0。
- 展开容器的宽度走与宽轨相同的测量策略（220 起、内容驱动、360 封顶），
  但动画 token 换成 `FastSpatial`。
- scrim 由 `modal: true` 渲染，不自己画；点击遮罩与 Escape 都经 overlay 的
  dismiss 通道落到 `on_dismiss`，收起同一份 state。
- 位置是 `PopupPosition::TopStart`（不是 `TopLeft`，也不是 `Center`）：
  `TopStart` 会按方向镜像，`Center` 会按自身尺寸居中——模态轨要贴前导边。

- 容器宽在 **96 ↔ 展开宽度** 之间按 progress 插值，measure 期读进度注册
  layout_deps——动画帧只重测不重组。
- **展开宽度不是固定值，也不是"窗口多宽就多宽"**。androidx 把
  `ExpandedRailMinWidth`(220) 与 `ExpandedRailMaxWidth`(360) 作为两个动画值，
  再由测量落定（`WideNavigationRail.kt:270-305`、`:387-405`）：

  ```
  width = actualMinWidth                       // 220
  if (expanded && widestElement > max(220, 实际展开宽))
      width = widthFullRange 向着 360 收窄
  ```

  即：**默认展开就是 220**，只有内容（item 内容 + `WNRItemHorizontalPadding`×1，
  或 header 自身宽度）比 220 宽时才往上长，360 封顶，且不超过窗口宽。
  上游自己的测试钉死了两端：`rail_expanded_size`（三个 item）断言
  `ContainerWidthMinimum`，`rail_expanded_maxSize`（400dp header spacer）断言
  `ContainerWidthMaximum`。winia 对应
  `wide_rail_width_endpoints_match_tokens` +
  `wide_rail_expands_past_the_minimum_for_a_wide_header`。
- item 几何在 Top/Start 两套端点间按进度线性插值：指示器从 56×32（图标后）
  过渡到包裹 [icon+gap4+label] 的横向大胶囊（高 max(iconH,labelH)+16）；
  图标/标签位置随之滑移。
- **简化注明**：androidx 用动态 PaddingValues + 分段 label 公式（p>0.5 切换 x
  公式并淡出标签）；本实现为连续 lerp，端点视觉一致。
- 容器顶部 padding 44dp（TopSpace）；展开态 item 填满轨宽（全宽命中目标），
  内容起始对齐（前导缩进 16，对齐 androidx FullWidthLeadingSpace）。
- item 同样拥抱内容、最小宽 96（CollapsedTokens.ContainerWidth）。
- **宽轨 item 的纵向节奏随展开变化**，两端各是一组不同的 token：

  | | 收起（Top） | 展开（Start） |
  |---|---|---|
  | item 最小高 | `TopIconItemMinHeight` = BaselineItem.ContainerHeight = **64** | `minimumA11ySize` = **48** |
  | item 之间的间距 | ItemVerticalSpace = **4** | **0** |
  | 胶囊高 | VerticalItem.ActiveIndicatorHeight = **32** | HorizontalItem.ActiveIndicatorHeight = **56** |
  | 图标-标签间距 | VerticalItem.IconLabelSpace = **4** | HorizontalItem.IconLabelSpace = **8** |
  | 内容前导缩进 | 居中 | WNRItemHorizontalPadding **20** + FullWidthLeadingSpace **16** |

  前两项上游是 `animateDpAsState` 动画值（`WideNavigationRail.kt:279-289`），
  winia 按展开进度线性插值。注意宽轨 item 的 64 与普通轨的 56 是**不同的 token**，
  不是同一个值的两种写法。
- 宽轨 header 之后的间距 = **40dp**（`HeaderSpaceMinimum`），与普通轨的 16dp
  不是同一个 token：普通轨是 8dp Spacer 落在 `spacedBy(4)` 的 Column 里
  （4+8+4=16），宽轨是 `headerPlaceable.height + WNRHeaderPadding`。
- 宽轨 header 在 **RTL 下镜像到右边缘**（`WIDE_INDICATOR_H_PADDING` 是 leading
  值，不是 left 值）；item 位于 `(width - w)/2` 是对称的，镜像后位置不变。
  方向在 compose 期捕获并传入 policy——placement 跑在组合之后，
  `direction::current()` 那时已不是真相。

## 6. Token 对照表

| Token | 值 | winia 常量 |
|---|---|---|
| NarrowContainerWidth | 80 | `NAVIGATION_RAIL_WIDTH` |
| VerticalItem.ActiveIndicatorWidth / Height | 56 / 32 | `NAVIGATION_RAIL_INDICATOR_WIDTH` / `_HEIGHT` |
| BaselineItem.IconSize | 24 | `NAVIGATION_RAIL_ICON_SIZE` |
| ItemHeight（=ActiveIndicatorWidth） | 56 | `NAVIGATION_RAIL_ITEM_HEIGHT` |
| CollapsedTokens.ContainerWidth | 96 | `WIDE_RAIL_COLLAPSED_WIDTH` |
| ExpandedTokens.ContainerWidthMinimum | 220 | `WIDE_RAIL_EXPANDED_MIN_WIDTH` |
| ExpandedTokens.ContainerWidthMaximum | 360 | `WIDE_RAIL_EXPANDED_MAX_WIDTH` |
| ExpandedTokens.ModalContainerShape（CornerLarge） | 16 | `modal_rail_expanded_shape()` |
| CollapsedTokens.TopSpace | 44 | `WIDE_RAIL_TOP_PADDING` |
| WNRItemHorizontalPadding | 20 | `WIDE_RAIL_ITEM_H_PADDING` |
| BaselineItem.HeaderSpaceMinimum（宽轨 header 间距） | 40 | `WIDE_RAIL_HEADER_PADDING` |
| ItemVerticalPadding / RailVerticalPadding | 4 / 4 | 私有常量 |
| BaselineItem.ContainerHeight（宽轨 item 收起态最小高） | 64 | `WIDE_RAIL_ITEM_MIN_HEIGHT` |
| minimumA11ySize（宽轨 item 展开态最小高） | 48 | `WIDE_RAIL_ITEM_MIN_HEIGHT_EXPANDED` |
| HorizontalItem.ActiveIndicatorHeight（Start 胶囊高） | 56 | 由 `WIDE_START_INDICATOR_V_PADDING` 推出 |
| HorizontalItem.IconLabelSpace（Start 图标-标签间距） | 8 | `WIDE_START_ICON_LABEL_GAP` |
| ItemVerticalSpace（展开态 item 间距） | 0 | `WIDE_RAIL_ITEM_GAP_EXPANDED` |
| NavigationRailHeaderPadding（普通轨 header 间距，含两侧 spacing） | 4+8+4=16 | 私有常量 `HEADER_SPACER` |
| ItemActiveIndicatorIconLabelSpace | 4 | 私有常量 `ITEM_ICON_LABEL_GAP` |
| 颜色 7 token | 同导航栏 | `rail_item_colors(&theme)` |

### 动效 token（`MotionScheme`，随主题 standard/expressive 切换）

| 用途 | Token | standard 值 | winia |
|---|---|---|---|
| 普通轨 item 淡入 | `DefaultEffects` | 1.0 / 1600 | `rail_alpha_spec()` |
| 普通轨胶囊尺寸 | `FastSpatial` | 0.9 / 1400 | `rail_item_size_spec()` |
| 宽轨容器展开 | `DefaultSpatial` | 0.9 / 700 | `rail_container_spec()` |
| **模态**宽轨展开 | `FastSpatial` | 0.9 / 1400 | `modal_rail_container_spec()` |
| 宽轨 item 变形 | `DefaultSpatial` | 0.9 / 700 | `wide_rail_item_morph_spec()` |

注意最后两行与直觉相反：宽轨 item 的变形用的是**较慢**的 `DefaultSpatial`
（`NavigationItem.kt:486-491`、`:1232-1237`），而模态轨容器用 `FastSpatial`
（`WideNavigationRail.kt:269`）。`the_rail_springs_are_the_tokens_its_sources_name`
逐个断言。

## 7. 平台差异与有意简化

- **windowInsets**：桌面默认零值。API 已预留（`WindowInsets` 结构体 +
  `.window_insets()`），未来 Android 支持或自定义窗口装饰（标题栏模拟状态栏）
  时由平台层填充真实尺寸。
- **PredictiveBack 缩放**：依赖 Android 返回手势进度输入，桌面无此源。
- **宽轨变形中间态**：连续 lerp 替代 androidx 分段公式（见 §5）。
- **宽轨状态机**：同步简化版（androidx 为 suspend + Animatable + Saver）。

## 9. 测试

| 测试 | 覆盖 |
|---|---|
| `navigation_rail_tokens_match_androidx_main` | token 数值断言 |
| `selected_item_geometry_matches_androidx_place_label_and_icon` | 选中布局数学 |
| `unselected_item_without_always_label_centers_icon` | 居中插值 + ripple 跟随 |
| `no_label_item_gets_circular_full_size_pill` | 56×56 圆形胶囊 |
| `bounded_item_hugs_content_instead_of_filling_width` | 拥抱宽度回归 |
| `disabled_item_has_no_interaction_elements` | 禁用态无交互元素 |
| `unselected_item_hover_shows_state_layer_on_full_pill_rect` | 悬浮状态层像素 |
| `wide_rail_width_endpoints_match_tokens` | 宽度端点 96/220/窗口约束 |
| `wide_rail_expands_past_the_minimum_for_a_wide_header` | 内容驱动增长与 360 封顶 |
| `wide_rail_header_mirrors_under_rtl_and_items_do_not` | RTL header 镜像、item 不镜像 |
| `modal_rail_is_a_plain_rail_until_it_expands` | 模态轨 overlay + TopStart |
| `hide_on_collapse_draws_nothing_until_expanded` | dismissible 变体 |
| `wide_rail_item_morphs_between_top_and_start_layouts` | 变形两布局端点（64/56 高、20+16 前导） |
| `wide_rail_item_gap_and_floor_follow_the_expansion` | item 间距 4→0 与最小高 64→48 |
| `wide_rail_state_toggles_expansion` | 状态机 |
| `wide_rail_item_target_width_expands_when_expanded` | 展开态目标区全宽（M3） |
| `the_rail_springs_are_the_tokens_its_sources_name` | 五个动效 token |