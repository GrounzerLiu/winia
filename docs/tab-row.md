# TabRow / Tab / ScrollableTabRow 组件（material3 对齐）

> 分支：`tab-row`（基于 v2；当前未合并回 v2）
> 对标：androidx-main `TabRow.kt` + `Tab.kt`（Primary/Secondary/Scrollable 变体）
> M3 规格：https://m3.material.io/components/tabs/specs
> 源码：`winia/src/ui/tab_row.rs`；示例：`examples/tab_row_demo.rs`
> （含 RTL/LTR 切换、Primary/Secondary 切换、LeadingIconTab 展示）

## 1. 变体总览

| 变体 | 对应 | 布局 | 指示条 |
|---|---|---|---|
| `TabRow`（默认） | M3 PrimaryTabRow | 固定等分（fillMaxWidth/tabCount） | Primary：跟随内容宽，圆角 3dp |
| `TabRow::secondary()` | M3 SecondaryTabRow | 固定等分 | Secondary：全 tab 宽直角 |
| `ScrollableTabRow` | M3 Primary/SecondaryScrollableTabRow | 内容自然宽（≥90dp）+ 2×52dp 边缘 | 同上（选中居中滚动） |

M3 规范：tab 少用固定等分；tab 多/超宽用可滚动变体。

## 2. API

```rust
// 固定 TabRow（Primary 默认，等分宽、48dp 高）
TabRow::new(selected_index, |ctx| { /* Tab ×N */ })
    .secondary()                          // Secondary 风格（指示条全宽直角）
    .container_color(c)                   // 默认 Surface
    .content_color(c)                     // 默认 Primary / OnSurface
    .divider_color(c)                     // 默认 OutlineVariant，1dp 底部分隔线
    .indicator_color(c)                   // 默认 Primary
    .indicator_shape(shape)               // Primary 默认 RoundedRect(3)
    .modifier(m)
    .build(ctx);

// 可滚动 TabRow（超出视口横向滚动，选中自动居中）
ScrollableTabRow::new(selected_index, |ctx| { /* Tab ×N */ })
    .scroll_state(scroll_state)           // 缺省内部 remember 创建
    .edge_padding(52.0)                   // 起始边缘（默认 52dp）
    .min_tab_width(90.0)                  // 最小 tab 宽（默认 90dp）
    .secondary()
    .build(ctx);

// Tab（默认竖排：icon 上/text 下）
Tab::new(selected, || on_click())
    .text(|ctx| Text::new("Tab A").build(ctx))
    .icon(|ctx| Icon::new(...).size(24.0).build(ctx))
    .content(|ctx| { /* 自定义内容——替代 text/icon 槽 */ })
    .leading_icon()                       // LeadingIconTab：icon 左 + 8dp + text 右
    .enabled(true)
    .selected_content_color(c)
    .unselected_content_color(c)
    .interaction_source(src)              // 注入外部交互源（须 remember 创建）
    .modifier(m)
    .build(ctx);
```

- **固定 TabRow 子节点顺序约定**：`[tab0, ..., tabN-1, divider, indicator]`——尾部两个
  槽由组件内部追加（布局 policy 按 `children[n-2]/[n-1]` 识别）。
- **ScrollableTabRow 相同**：tabs 按内容自然宽排列，`layout_width = 2×edgePadding + ΣtabW`。
- Tab 点击走 `clickable_with_source` + 全节点 ripple（bounded，ripple 色 = 选中内容色）。
- 指示条**居中于 slot**（offset = tab_left + (tabWidth − indicatorWidth)/2），
  非贴边（对齐 M3 规范）。

## 3. 指示条动画（两段式依赖——核心机制）

- build 期创建双 `State<f32>`（offset/width，`remember(|| 0.0)`）+ `initialized` 标记
  （Arc\<AtomicBool\>）。
- **measure 期**（`TabRowLayoutPolicy::measure` / `ScrollableTabRowLayoutPolicy::measure`）：
  1. **最开头** `offset_state.get()` / `width_state.get()` 注册 layout_dep（必须最先——
     递归测量子节点后 ACTIVE_SLOT_KEY 会被改写，依赖会挂到 divider/indicator 叶）。
  2. 计算 target（选中 slot 的物理 left + 居中偏移、宽度）。
  3. `initialized` 未置位 → `set_silent` 直接到位（首帧免动画）；
     已置位 → `push_animatable(state, target, spec)`（全局 pub，可在 layout 期调用）。
  4. `peek()` 读当前动画值做 placement（零注册开销）。
- 动画帧 → 仅重测 TabRow 节点（不重组）→ 指示条位置更新。spec =
  `spring(damping_ratio=0.6, stiffness=700)`（对齐 M3 Expressive DefaultSpatial）。

### Tab 颜色过渡（TabTransition 对齐）

- `animate_color_as_state(selected ? sel : unsel, spring(1.0, 300))` → `State<Color>`；
  text/icon 槽挂 `graphics_layer(color_filter: ColorFilter::Tint { SrcIn })`——渲染期
  peek 零重组，单层动态染色（无需双文本交叉淡化）。layout_dep 由
  `TabLayoutPolicy::measure` 开头 `color_anim.get()` 注册（动画帧重测 Tab → 重绘）。

## 4. RTL——四层镜像（缺一不可）

| 层 | 位置 | 机制 |
|---|---|---|
| 布局镜像 | `ScrollableTabRowLayoutPolicy::measure` | tabs 从右往左排（tab0 最右贴内容末端）；先测全部宽再放置（RTL 需总宽） |
| render 平移镜像 | `ModifierElement::HorizontalScroll.reverse` + render.rs | offset 0 = 内容末端；`off = content_w − viewport_w − offset` |
| 手势/动画 delta 镜像 | app.rs `apply_scroll_delta` + fling | `new = current + dx`（reverse 时）、`fling_vx = −child_velocity.x` |
| hit_test 坐标镜像 | `scroll_offset_for_node` | reverse 时返回 `content_w − viewport_w − offset`（否则点击命中错位） |

- 居中滚动 target 镜像：`offset_rtl = available − centered`（绕 available 翻转），
  clamp [0, available]。
- **LeadingIconTab 内部也需镜像**（`TabLayoutPolicy.direction`）：RTL 时 icon 移右、
  text 移左（Compose Row 自动镜像子节点顺序的物理等价）。
- 固定 TabRow RTL：`x = row_width − (i+1)×tab_width`（TabRowLayoutPolicy）。

### 首帧不居中根因（框架级教训）

- `measure_node` 在 `policy.measure` **之后**才回写 `fling_limit`（node.rs:1535-1551）；
  首帧 policy 读 fling_limit 恒 0 → target 恒 0 不滚动且 `last_selected` 已消费。
- 修复：policy 加 `layout_seen: State<bool>`——首帧只 `set(true)`（notify → 下帧重测）
  不消费 `last_selected`；第二帧 fling_limit 就绪后触发居中滚动（`animate_scroll_to`）。
- **方向切换重居中**：build 记 `last_dir`（0/1），direction 变化时重置
  `last_selected=-1` → 下帧重新居中。

## 5. Tab 内部布局（TabLayoutPolicy）

| 模式 | 布局 | spec 高 |
|---|---|---|
| text-only / icon-only | 水平+垂直居中 | 48（SmallTabHeight） |
| text + icon（竖排） | icon 上 + 20dp + text 下，居中 | 72（LargeTabHeight） |
| leading（`.leading_icon()`） | icon 左 + 8dp + text 右，整组居中 | 48 |

- 内容用 **loose 约束**测量（TabRow 传 tight 会撑爆：72+72+20=164）；再钳制回 incoming。
- leading 自然宽 = icon+8+text 三者之和；text 槽**不加** 16dp 水平 padding
  （否则 icon-text 间隙被撑成 8+16）。
- ripple leaf 最后放置、恒定全尺寸（fill_max_size 覆盖整个 tab slot）。

## 6. Token 对照表

| Token | 值 | winia 常量 |
|---|---|---|
| ContainerHeight / SmallTabHeight | 48 | `SMALL_TAB_HEIGHT`（`TAB_ROW_HEIGHT`） |
| LargeTabHeight | 72 | `LARGE_TAB_HEIGHT` |
| ActiveIndicatorHeight | 3 | `ACTIVE_INDICATOR_HEIGHT` |
| HorizontalTextPadding | 16 | `HORIZONTAL_TEXT_PADDING` |
| 最小指示条宽 | 24 | `MIN_INDICATOR_WIDTH` |
| IconDistanceFromBaseline 近似 | 20 | `ICON_TEXT_SPACING` |
| TextDistanceFromLeadingIcon | 8 | `LEADING_ICON_TEXT_SPACING` |
| ScrollableTabRowMinTabWidth | 90 | `SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH` |
| ScrollableTabRowEdgeStartPadding | 52 | `SCROLLABLE_TAB_ROW_EDGE_START_PADDING` |
| ActiveIndicatorColor / LabelTextColor | Primary / TitleSmall | `TabRowDefaults::*` |
| InactiveLabelTextColor | OnSurfaceVariant | `TabRowDefaults::unselected_content_color` |
| DividerColor | OutlineVariant，1dp | `TabRowDefaults::divider_color` |
| 指示条动画 | spring(0.6, 700) | `indicator_spring()` |

## 7. 已知差距（与 Compose 对照）

- **TabIndicatorScope 自定义指示器 API 未做**：Compose 的 indicator 槽依赖
  SubcomposeLayout（组合期注入用户槽、measure 期喂 tabPositions）。winia 无
  subcompose 设施——positions 是 measure 期数据，组合期无法注入用户闭包，
  需先造新机制（布局期写 State + 用户槽 peek 读）或换轻方案（indicator 用户
  content 槽 + `tab_positions: State<Vec<TabPosition>>` 暴露）。
- 无 TabBaselineLayout 基线精确数学（竖排 text+icon 居中，无 first/lastBaseline 修正）。
- 无 icon-only 独立 API（icon-only 用 `.icon()` 即可，与 text-only 同 48dp）。
- 固定/可滚动变体间无动画过渡（Compose 亦无——用户显式选择）。
- windowInsets 不适用（桌面无系统栏叠加）。

## 8. 测试（`ui::tab_row::tests`，21 个）

| 测试 | 覆盖 |
|---|---|
| `tab_row_tabs_equal_width` / `tab_row_primary_indicator_position` / `tab_row_secondary_indicator_width` | 固定等分 + 指示条几何 |
| `tab_row_rtl_mirror` / `tab_row_rtl_indicator_mirrors` | 固定 RTL 镜像 |
| `tab_row_0_tabs_does_not_panic` / `tab_row_selected_out_of_range_falls_back_to_origin` | 边界 |
| `tab_row_indicator_animates_on_selected_change` | 指示条动画推进 + 收敛 |
| `tab_content_version_ripple_covers_full_tab` | 自定义 content ripple 全尺寸 |
| `tab_leading_icon_lays_out_horizontally` / `tab_leading_icon_rtl_mirrors_icon_to_right` | LeadingIconTab 布局 + RTL 镜像 |
| `tab_accepts_injected_interaction_source` | 交互源注入 |
| `scrollable_tab_row_lays_out_natural_widths_with_padding` | 自然宽 + 边缘 padding |
| `scrollable_tab_row_indicator_centered_on_selected` | 指示条居中 |
| `scrollable_tab_row_scrolls_selected_into_view` / `scrollable_tab_row_centers_initial_selection` | 选中居中滚动 + 首帧延迟回归 |
| `scrollable_tab_row_rtl_mirrors_tab_positions` / `scrollable_tab_row_rtl_indicator_centered_on_selected` / `scrollable_tab_row_rtl_scrolls_selected_into_view` | RTL 布局/指示条/居中滚动 |

另：`layout::node::tests::test_scroll_offset_rtl_reverse_mirrors`——hit_test 坐标
转换镜像回归。
