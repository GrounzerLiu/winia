# NavigationRail / WideNavigationRail 组件（material3 对齐）

> 对标：androidx-main `NavigationRail.kt`（785 行，Collapsed 基线）+
> `WideNavigationRail.kt`（1422 行，M3 Expressive 宽轨）+ `NavigationItem.kt`
> 的 Start 布局数学
> M3 规格：https://m3.material.io/components/navigation-rail/specs
> 源码：`winia/src/ui/navigation_rail.rs`；示例：`examples/navigation_rail_demo.rs`

## 1. 变体总览

| 组件 | 对应 M3 | 宽度 | item 图标位置 | label |
|---|---|---|---|---|
| `NavigationRail` | Collapsed 基线 | 80dp | 上（Top） | 可选，alwaysShowLabel 控制 |
| `WideNavigationRail` | Expressive 宽轨 | **96 ↔ 220dp 动画** | 收起上/展开左（变形） | **必选恒显示** |

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

- 容器宽 = lerp(**96**, **220**, progress)，measure 期读进度注册 layout_deps
  ——动画帧只重测不重组。
- item 几何在 Top/Start 两套端点间按进度线性插值：指示器从 56×32（图标后）
  过渡到包裹 [icon+gap4+label] 的横向大胶囊（高 max(iconH,labelH)+16）；
  图标/标签位置随之滑移。
- **简化注明**：androidx 用动态 PaddingValues + 分段 label 公式（p>0.5 切换 x
  公式并淡出标签）；本实现为连续 lerp，端点视觉一致。
- 容器顶部 padding 44dp（TopSpace）；展开态 item 填满轨宽（全宽命中目标），
  内容起始对齐（前导缩进 16，对齐 androidx FullWidthLeadingSpace）。
- item 同样拥抱内容、最小宽 96（CollapsedTokens.ContainerWidth）。

## 6. Token 对照表

| Token | 值 | winia 常量 |
|---|---|---|
| NarrowContainerWidth | 80 | `NAVIGATION_RAIL_WIDTH` |
| VerticalItem.ActiveIndicatorWidth / Height | 56 / 32 | `NAVIGATION_RAIL_INDICATOR_WIDTH` / `_HEIGHT` |
| BaselineItem.IconSize | 24 | `NAVIGATION_RAIL_ICON_SIZE` |
| ItemHeight（=ActiveIndicatorWidth） | 56 | `NAVIGATION_RAIL_ITEM_HEIGHT` |
| CollapsedTokens.ContainerWidth | 96 | `WIDE_RAIL_COLLAPSED_WIDTH` |
| ExpandedTokens.ContainerWidthMinimum | 220 | `WIDE_RAIL_EXPANDED_MIN_WIDTH` |
| CollapsedTokens.TopSpace | 44 | `WIDE_RAIL_TOP_PADDING` |
| ItemVerticalPadding / RailVerticalPadding | 4 / 4 | 私有常量 |
| HeaderPadding | 8 | 私有常量 `HEADER_SPACER` |
| ItemActiveIndicatorIconLabelSpace | 4 | 私有常量 `ITEM_ICON_LABEL_GAP` |
| 颜色 7 token | 同导航栏 | `rail_item_colors(&theme)` |

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
| `wide_rail_width_endpoints_match_tokens` | 宽度端点 96/220 |
| `wide_rail_item_morphs_between_top_and_start_layouts` | 变形两布局端点 |
| `wide_rail_state_toggles_expansion` | 状态机 |
| `wide_rail_item_target_width_expands_when_expanded` | 展开态目标区全宽（M3） |