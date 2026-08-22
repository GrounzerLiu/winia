# NavigationBar / NavigationBarItem 组件（material3 对齐）

> 对标：androidx-main `NavigationBar.kt`（v0_11_0 token）+
> `ShortNavigationBar.kt`（M3 Expressive 水平 item）+ `NavigationItem.kt`
> 布局数学
> M3 规格：https://m3.material.io/components/navigation-bar/specs
> 源码：`winia/src/ui/navigation_bar.rs`；示例：`examples/scaffold_demo.rs`
> （Scaffold 集成）、`examples/navigation_rail_demo.rs`（对照）

## 1. 变体总览

| 变体 | 对应 | item 布局 | 使用场景 |
|---|---|---|---|
| 默认（垂直 item） | M3 基线 | 图标上/标签下 | 紧凑窗口 |
| 水平 item（`.layout(Horizontal)`） | M3 Expressive ShortNavigationBar | 图标左/标签右，胶囊包裹整组 | 中等窗口 |

M3 规范：紧凑窗口用垂直 item；中等窗口用水平 item。

## 2. API

```rust
// 容器：surfaceContainer 底色、固定高 80、item 等分宽度（weight 语义）+
// 8dp 间距、RTL 镜像
NavigationBar::new(|ctx| { /* NavigationBarItem ×3~5 */ })
    .colors(NavigationBarColors { container })   // 默认 surfaceContainer
    .modifier(m)
    .build(ctx);        // Scaffold 场景用 .bottom_bar(|ctx| rail.build(ctx))

// item（垂直，默认）
NavigationBarItem::new(selected, |ctx| { Icon... })
    .label(|ctx| Text::new("Home").build(ctx))
    .on_click(|| {})
    .enabled(true)
    .always_show_label(true)   // false：未选中隐藏标签且图标居中插值
    .colors(NavigationBarItemColors::from_theme(&theme))
    .interaction_source(source)
    .modifier(m)
    .build(ctx);

// item（水平，Expressive）——图标在左、胶囊横向包裹 [icon+gap4+label]
NavigationBarItem::new(selected, |ctx| { Icon... })
    .label(...)
    .layout(NavigationBarItemLayout::Horizontal)
    .build(ctx);
```

- **item 等分填满槽位**（与 rail 的拥抱语义相反——底部栏 tap 目标占满列宽）。
- 容器高固定 80dp（`NAVIGATION_BAR_HEIGHT` = TallContainerHeight）。
- 徽章用法：icon 槽内包 `BadgedBox`（见 examples/badge_demo.rs）；
  BadgedBox 尺寸=锚点尺寸，不影响胶囊推导。**计数徽章的 State 读取必须放在
  content 闭包内**（最近作用域读状态），否则数字不更新。

## 3. 指示器几何（垂直 item）

| 项 | 值 | 来源 |
|---|---|---|
| 胶囊尺寸 | 56×32 | ActiveIndicatorWidth/Height |
| 横向内边距 | (56−24)/2 = 16 | 由图标推导宽 |
| 纵向内边距 | (32−24)/2 = 4 | |
| 图标-标签间距 | 4+4（IndicatorVerticalPadding + IndicatorToLabelPadding） | |
| contentHeight | iconH + 4 + 4 + labelH | placeLabelAndIcon |
| 未选中 !alwaysShowLabel | 图标居中插值 offset = iconDistance × (1−p) | |

- 双进度动画：alphaProgress（stiffness 200——颜色/label 淡出）与
  sizeProgress（stiffness 400——胶囊展开/位置插值），对齐 androidx
  alphaAnimationProgress/sizeAnimationProgress 分离。

### 水平 item 几何（Expressive）

- 胶囊横包 [icon + gap(4) + label] 整组：宽 = iconW+4+labelW+2×16、
  高 = max(iconH,labelH)+2×8；内容组整体居中、全部垂直居中。
- label 恒显示（alwaysShowLabel 淡出/位置插值仅垂直模式）；无 label 时退化为
  垂直模式的 56×32 胶囊（TopIconOrIconOnlyMeasurePolicy）。

## 4. 悬浮状态层 / ripple（IndicatorRipple 分离设计）

- 子节点顺序 `[indicator, icon, label?, ripple]`——ripple 最后放置（z 最上层），
  状态层覆盖彩色胶囊与内容（选中/未选中悬浮均有）。
- ripple 节点恒定全尺寸（未选中胶囊收拢为 0 宽仍有完整热区），跟随指示器
  动画位置。
- hover 经交互源 hover_opacity（500ms tween）；渲染裁剪为 Pill。
- 像素回归：`unselected_item_hover_shows_state_layer_on_full_pill_rect`。

## 5. Scaffold 集成与滚动内容裁剪

- `Scaffold.bottom_bar(|ctx| NavigationRail... )`——bottom slot 高 80dp，
  内容区自动让位（content 高 = 窗口 − top − bottom）。
- **滚动内容越界裁剪**：滚动容器 measured_size 为内容全高（max_offset 依赖），
  渲染期按 `scroll_viewport_height/width` 裁剪、命中测试同口径——否则列表项
  会绘制到 bottom bar 之上（已修复，见 render.rs render_pass1 与
  hit_test_recursive）。

## 6. Token 对照表

| Token | 值 | winia 常量 |
|---|---|---|
| TallContainerHeight | 80 | `NAVIGATION_BAR_HEIGHT` |
| ItemHorizontalPadding（间距） | 8 | `NAVIGATION_BAR_ITEM_SPACING` |
| ActiveIndicatorWidth / Height | 56 / 32 | `NAVIGATION_RAIL_INDICATOR_*` 同值 |
| IconSize | 24 | `NAVIGATION_BAR_ICON_SIZE` |
| ItemActiveIcon / Indicator / LabelText | OnSecondaryContainer / SecondaryContainer / Secondary | `NavigationBarItemColors::from_theme` |
| InactiveIcon / InactiveLabelText | OnSurfaceVariant | 同上 |
| DisabledAlpha | 38% | 私有常量 |
| 水平：LeadingSpace×2 | 16×2 | `H_INDICATOR_HORIZONTAL_PADDING`(私有) |
| 水平：IconLabelSpace | 4 | `START_ICON_TO_LABEL_PADDING`(私有) |

## 7. 平台差异与有意简化

- 无 badge 专用参数（androidx item 内也是 BadgedBox 组合——用法见 §2）。
- 水平/垂直变形过渡（ShortNavigationBar 的 iconPosition 动画）未做——两种布局
  通过 `.layout()` 显式选择（宽轨 WideNavigationRail 有连续变形版本）。
- windowInsets 不适用（桌面无系统栏叠加）。

## 8. 测试

| 测试 | 覆盖 |
|---|---|
| `navigation_bar_tokens_match_androidx_main` | token 断言（含水平 item token） |
| `item_colors_follow_v0_11_0_tokens` | 7 色 token 映射 |
| `bar_splits_width_equally_with_spacing_and_rtl_mirrors` | 等分 + RTL 镜像 |
| `selected_item_geometry_matches_androidx_place_label_and_icon` | 选中布局数学 |
| `unselected_item_without_always_label_centers_icon` | 居中插值 |
| `always_show_label_keeps_positions_stable_when_unselected` | 位置稳定 |
| `enabled_item_indicator_carries_pill_ripple_and_dynamic_background` | 节点职责分离 |
| `disabled_item_has_no_interaction_elements` | 禁用态 |
| `horizontal_item_geometry_matches_androidx_place_label_and_start_icon` | 水平数学 |
| `horizontal_item_without_label_falls_back_to_circular_indicator` | 水平回退 |
| `scaffold_bottom_bar_integrates_navigation_bar` | Scaffold 集成 |
| `unselected_item_hover_shows_state_layer_on_full_pill_rect` | 悬浮状态层像素 |
| visual_matrix `navigation_bar_*` ×2 | 像素回归（胶囊/RTL/水平包裹） |