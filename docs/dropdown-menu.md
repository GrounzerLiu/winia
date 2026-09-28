# DropdownMenu（对齐 Compose material3）

`winia/src/ui/overlay.rs` 的 `DropdownMenu` / `DropdownMenuItem`，以及本文记录的对齐进度与偏差。

## 0. 运行

```bash
cargo run -p winia --example dropdown_menu_demo --features debug-server
```

demo 里按顺序演示：① 材质默认（单面板 + 禁用项）② 30 项长菜单（封顶 / 贴边 / 可滚动）③ 样式参数（`offset` / `shape` / `shadow_elevation` / 项 `colors` / `content_padding`）④ 图标槽（前导图标 + 尾随快捷键提示）⑤ 贴近窗口底部的触发器（菜单**向上翻**——M3 候选序列的第二个）。

## 1. 现状 API

```rust
let expanded = ctx.remember(|| false);
Button::new().on_click(move || expanded.set(true)).build(ctx, |ctx| Text::new("打开").build(ctx));

DropdownMenu::new(expanded.clone())
    .on_dismiss_request(move || expanded.set(false))
    .modifier(Modifier::new().test_tag("dm-container"))   // M3 的 modifier（作用于菜单容器/Surface）
    .offset(0.0, 0.0)            // M3 offset: DpOffset（默认 (0,0)）
    .shape(Shape::RoundedRect { corner_radius: 4.0 })     // 默认即 M3 CornerExtraSmall
    .container_color(colors.surface_container)            // 默认即 M3 surfaceContainer
    .shadow_elevation(3.0)                                // 默认即 M3 Level2
    .border(SurfaceBorder::new(1.0, color))               // 默认无边框
    .build(ctx,
        |ctx| { /* 锚点：菜单定位到这块内容的下方 */ },
        |ctx| { DropdownMenuItem::new("新建文件").on_click(…).build(ctx); });

DropdownMenuItem::new("删除")
    .modifier(Modifier::new().test_tag("dm-item-delete"))  // 调用方 modifier，追加在内部样式外层
    .enabled(false)
    .colors(MenuItemColors::defaults())                    // M3 MenuItemColors（默认按主题角色）
    .content_padding(12.0, 0.0)                            // M3 contentPadding（默认水平 12、垂直 0）
    .on_click(…)
    .build(ctx);
```

走顶层 overlay 机制（独立 Composer）：`anchor_slot` 定位、无进出动画（与 Compose 默认一致）、`modal: false`、`dismiss_on_outside: true`、每次组合记录 `active` 供 sync 删除（同 Popup/Dialog 的契约，见 `docs/key-system-design.md`）。

## 2. 已修：菜单项坍缩成最后一项（本轮发现）

**症状**：一个含 3 项的菜单，树里只有最后一项（`overlay_demo` 与 UI fixture 都能复现）。

**根因**（组合层，不是 overlay 特有）：winia 的组合根是**单个节点**——顶层兄弟节点只有最后一个成为 root（`core/materialize.rs`：“wrap in a container, or keep emitting siblings, which is its own round”）。菜单项若作为顶层兄弟组合，前两项被丢弃。用裸 Composer 实测：3 项 → `arena_len=6`，而 `layout_root_idx()` 是最后一项（size 9×12）；3 个普通 `Column` 作对照同样如此。

**这正是 Compose 把 `DropdownMenu` 的 content 写成 `@Composable ColumnScope.() -> Unit` 的原因**：内容本就该在菜单自己的 Column 里。

**修法**：`DropdownMenu::build` 把内容包进 `Column`（`overlay.rs` 的 `content: Box::new(move |ctx| Column::new().build(ctx, |ctx| menu(ctx)))`）。

**证据**：修复前 fixture 树里 `新建文件` 0 次 / `重命名` 0 次 / `删除` 1 次；修复后三者各 1 次。

## 2b. 已修：项内容没有垂直居中（并暴露一个 flex 布局 bug）

**症状**（用户看 demo 截图指出："上下的padding不对称"）：容器自身的 8dp 上下内边距是对的，但**项里的文字贴在项顶部**——项 48 高、文字 20，于是文字下方空 28、上方空 0，整块看起来下方更空。实测树：文字节点 `pos:[12,0]`。

**M3 真身**：`DropdownMenuItemContent` 的最外层是 `Row(..., verticalAlignment = Alignment.CenterVertically)`——项内容垂直居中。

**直接修法**：项容器加 `Arrangement::Center`（winia 的等价物）。

**但这一改暴露出框架级 bug**（`layout/flex.rs`）：主轴剩余空间的判定写成 `A::main_max(constraints).is_finite()`，而 winia 的"无界"哨兵是 `f32::MAX`，**它 `is_finite()` 为真** → 剩余空间 = `f32::MAX` → `Arrangement::Center` 把子节点放到 `170141173319264429905852091742258462720`（实测值），也就是 1.7e38。

**修法**：先解出容器最终的主轴尺寸，再由它算剩余——既让"带 min 的容器"（48dp 项 + 20px 文字）真正有空间可分配，也把哨兵挡在算术之外；SpaceBetween/SpaceAround/SpaceEvenly 语义不变（它们定义在"可用空间"上，仍用有限 max）。

**证据**：修后文字 `pos:[12,14]` = (48−20)/2 ✓；测试里断言 `上 = 下`，实测 `item=(16,162,48) label=(28,176,20) 上=14 下=14`。

## 3. 测试（`fixture_dropdown_menu` + `ui_test.rs`）

| 测试 | 钉住的行为 |
|---|---|
| `dropdown_menu_opens_and_an_item_pick_closes_it` | 打开后 overlay 条目出现且含各项文本；点项触发回调 `dm-picked` 且菜单关闭（`dm-open: no`），popup 条目被移除（不是留着继续吃点击） |
| `dropdown_menu_a_disabled_item_neither_fires_nor_dismisses` | `enabled(false)` 的项既不触发回调也不关闭菜单（对齐 Compose：该项根本不可点） |
| `dropdown_menu_dismisses_on_an_outside_click` | 点击菜单外 → 经 `on_dismiss_request` 关闭，且没有任何项被选中 |
| `dropdown_menu_geometry_matches_the_material3_metrics` | 项宽 ∈ [112, 280]（对 4 个汉字的标签即证明 minWidth 钳制生效）、项高 ≥ 48、容器高 = 3 项 + 上下各 8dp |
| `dropdown_menu_paints_its_surface` | 容器**确实绘制**：菜单内 8dp 内边距处的像素与页面背景不同（树里看不出颜色，故读帧） |
| `dropdown_menu_a_long_menu_fits_the_window_and_scrolls_to_its_end` | 20 项菜单：容器完全落在窗口内（`cy+ch ≤ 窗口高`），滚轮把 offset 推到上限附近，末项渲染位置落在容器内 |
| `dropdown_menu_animates_in_from_its_anchor` | 进出动画：同一帧内两个探针（pivot 侧顶边 vs 远端角）要求出现"既非页面也非稳定面板"的过渡帧 |
| `dropdown_menu_item_ripples_while_pressed` | 按住菜单项时该处像素变化（实测 242,236,244 → 221,216,223）；去掉波纹即红 |
| `dropdown_menu_takes_the_keyboard_while_open` | 打开期间 Tab 落在菜单项（而不是主树按钮），且页面元素不再持有焦点 |
| `dropdown_menu_a_focused_item_activates_on_enter` | Tab 聚焦首项后 Enter：回调触发且菜单关闭 |
| `dropdown_menu_esc_dismisses_it` | Esc 经 `on_dismiss_request` 关闭，且没有项被选中 |
| `dropdown_menu_returns_focus_to_its_trigger_on_close` | Esc 关闭后焦点回到触发器 `dm-toggle` |
| `dropdown_menu_item_icon_geometry_matches_material3` | 前导图标 24dp 盒在项内缩进 12dp、标签在其后 12dp（实测 16→28→64）；尾随图标收在项右内容边（实测 x=92 且项宽 112）；**所有项等宽且等于菜单宽**（实测 112/112/112），且菜单 < 280（取最宽项自然宽而非上限） |
| `dropdown_menu_highlights_the_focused_item` | 聚焦后项内变暗 25 个单位（状态层），且停止在"完全淡入"而不是第一次波动 |
| `a_menu_item_carries_a_ripple_and_no_focus_ring`（lib 单测） | 项的节点上同时有 `Clickable`、`Ripple`、`NoFocusRing`——环是边界外 ~1px 的带（实测物理 x=23 为 `(197,193,199)`，内侧是 `(217,211,219)`），逻辑坐标探针踩不准，故用结构断言 |

## 4. 与 Compose M3 的差异（依据：本地 androidx 源码）

依据文件：`target/compose-src/commonMain/androidx/compose/material3/Menu.kt`、`androidMain/.../AndroidMenu.android.kt`、`commonMain/.../tokens/MenuTokens.kt`。

### 4.1 本轮之前就已对齐

| 项 | M3 | winia |
|---|---|---|
| 内容为列容器 | content: `ColumnScope.() -> Unit` | `build` 内部包 `Column`（见 §2） |
| 项可点击/禁用 | `clickable(enabled, onClick)`，禁用不可点 | 同（禁用时不挂 clickable） |
| 默认无进出动画 | （platform popup 默认） | `enter_anim/exit_anim = None` |
| 外部点击关闭 | `onDismissRequest` | `dismiss_on_outside: true` + `on_dismiss_request` |

### 4.2 阶段 2 对齐（几何与样式）

依据：`material3/Menu.kt`（`DropdownMenu` / `DropdownMenuItem` / `MenuDefaults` 的数值）、`tokens/MenuTokens.kt`、`tokens/ListTokens.kt`。

| 项 | M3 真身 | winia 现状 |
|---|---|---|
| 容器形状 | `MenuTokens.ContainerShape` = CornerExtraSmall（4dp） | 默认 `RoundedRect { 4.0 }`，可 `shape(...)` |
| 容器色 | `surfaceContainer` | 默认主题 `surface_container`，可 `container_color(...)` |
| 阴影 | shadow `Level2`（3dp）、tonal `Level0` | 默认 3.0 / 0.0，可 `shadow_elevation(...)` / `tonal_elevation(...)` |
| 边框 | `border: BorderStroke? = null` | 默认无，可 `border(SurfaceBorder)` |
| 容器垂直内边距 | `DropdownMenuVerticalPadding = 8dp` | Column 上下各 8dp |
| 偏移 | `offset: DpOffset = DpOffset(0, 0)` | 默认 `(0, 0)`，可 `offset(x, y)`（原为写死 `(0, 4)`） |
| 菜单 modifier | `modifier`（作用于菜单容器） | `modifier(...)`，追加在内部 modifier 外层 |
| 项几何 | `sizeIn(minWidth 112dp, maxWidth 280dp, minHeight 48dp)` + `padding(horizontal 12dp, vertical 0)` | `min_width(112).max_width(280).min_height(48).padding_horizontal(12).padding_vertical(0)` |
| 项背景/圆角 | 无（由容器绘制） | 已去掉原来的写死白底 + 4dp 圆角 |
| 项文本 | `ProvideTextStyle(typography.labelLarge)` | `WiniaTheme::typography().label_large`（14/20/0.1/Medium，与 M3 同值） |
| 项内容垂直居中 | `Row(verticalAlignment = Alignment.CenterVertically)` | `Column::new().arrangement(Arrangement::Center)`（见 §2b） |
| 项波纹 | `clickable(enabled, onClick, interactionSource, indication = ripple(true))` | `clickable_with_source` + `ripple_with_shape(..., bounded = true, Shape::Rectangle)`（同 `Button` 的接法）；`interaction_source(...)` 参数，未设则 `remember` 自持；纯矩形是因为 M3 的项没有 shape——圆角由菜单 Surface 的 `clip(shape)` 负责（`surface.rs`），且 8dp 内边距本来就让首末项不碰圆角 |
| 项颜色 | `MenuItemColors`：文本 `OnSurface`、图标 `OnSurfaceVariant`、禁用 = 同角色 @38%（`ListItemDisabled*Opacity`） | `MenuItemColors` 六个字段同名同义 + `defaults()`；禁用走 `0.38` alpha（与 navigation 组件同一常量值） |
| `contentPadding` | `PaddingValues(horizontal = 12dp, vertical = 0)` | `content_padding(h, v)`，默认 `(12, 0)` |
| 前导图标 | `leadingIcon` → `Box(defaultMinSize(minWidth = ListItemLeadingIconSize /*24dp*/))`，色用 `colors.leadingIconColor(enabled)` | `leading_icon(...)` 槽：24dp 盒 + `WiniaTheme::with_content_color(colors.leading_icon_color(enabled))`（等价 M3 的 `CompositionLocalProvider(LocalContentColor provides …)`） |
| 尾随图标 | `trailingIcon` → 同样 24dp 盒，色用 `colors.trailingIconColor(enabled)` | `trailing_icon(...)` 槽，同上 |
| 图标与文本间距 | 文本盒 `padding(start = 12dp if leading, end = 12dp if trailing)` | 同（实测：项 x=16 → 图标 x=28 宽 24 → 标签 x=64 = 16+12+24+12） |

几何有测试钉住（树断言，不是像素）：`dropdown_menu_geometry_matches_the_material3_metrics` —— 项宽必须落在 `[112, 280]`（对 4 个汉字的标签即证明 minWidth 钳制生效：内容只有 ~80px）、项高 ≥48、容器高 = 3 项 + 上下 8dp（≥160 且 <200）。旧几何（写死 160×36）会因项高 36 与容器高 108 两条断言失败。

**等宽与 intrinsic 宽度（阶段 2b 的补充）**：M3 把文本盒写成 `weight(1f)`，配合菜单列的 `width(IntrinsicSize.Max)` —— 菜单取**最宽项的自然宽**，所有项再撑满它。winia 框架层**没有 intrinsic 测量**（`segmented_button` 为此手写了 MeasurePolicy），而加权子节点会填满**约束**：实测直接加权，菜单宽度立刻从 112 顶到 280 上限 ✗；更糟的是当时我按"有尾随图标才加权"权宜，导致**同一菜单内的项宽度不一致**（112 与 280 并存 ✗），于是**状态层/波纹只覆盖行的一部分**（用户截图就是这条）。

现在由 `MenuColumnPolicy`（菜单列自己的 MeasurePolicy）补齐两遍测量：
1. **自然宽**：测每个项的**内容**（不含其 own padding 后再加回），因为项自身的标签是加权的——给加权子节点无界约束只会把约束原样报回来；
2. 把该宽度**紧约束**施加给每个项，此后加权标签在行内的余量分配就与 M3 完全一致。

实测（fixture 图标菜单）：`lead=112 trail=112 menu=112` ✓ 等宽、菜单 = 项宽、且 < 280（最宽项自然宽）✓；尾随图标 x=92 = 项右缘 − 12 ✓。demo 里"复制 / Ctrl+C"那种菜单宽度为 **136**（此前是 280 ✗）。框架层的 intrinsic 测量仍缺失（列在 §4.6）。

### 4.3 阶段 3 对齐：长菜单（滚动 + 按锚点选位）

M3 的机制由三件事组成（`Menu.kt` 的 `DropdownMenuContent` + `internal/MenuPosition.kt` 的 `DropdownMenuPositionProvider`）：

1. 内容在 `Column` 里，链为 `modifier.padding(vertical = DropdownMenuVerticalPadding).width(IntrinsicSize.Max).verticalScroll(scrollState)` → **8dp 内边距在滚动之外**（内容滚动时它不动），滚动容器由菜单提供；
2. 内容按窗口约束测量，故高度封顶在窗口；
3. **定位**在候选位置里挑：锚点下方 → 锚点上方 → 贴窗口边（横向同理：起始对齐 → 末端对齐 → 贴边）。

winia 对照实现：

| 项 | M3 | winia |
|---|---|---|
| 滚动 | `verticalScroll(scrollState)` | `Modifier::new().padding_vertical(8.0).vertical_scroll(scroll_state)`，`scroll_state(...)` 参数，默认 `ctx.remember(ScrollState::new())` |
| 高度封顶 | 平台 popup 按窗口测量 | winia 滚动容器自身尺寸 = `min(内容, 视口+padding)`（`layout/node.rs`）——天然封顶 |
| 定位 | 候选序列 | `OverlayDesc::fit_around_anchor`（**opt-in**，只有 DropdownMenu 打开）：下方→上方→贴边，x 同样三候选；其余 overlay（Popup/Dialog/BottomSheet/Tooltip）保持原有定位 |
| 滚轮投递 | 平台 popup 自己收 | **winia 的滚轮原先只看主树**——overlay 有独立 arena，菜单的滚动容器收不到滚轮。已修：滚轮先看指针下的 overlay（`hit_overlay`），debug 注入的 `s` 同优先级 |

**改前实测**（20 项菜单、520px 窗口）：容器 `(16,238,112,520)`，底边 758 > 窗口 520——底部 238px 的项够不到。**改后**：容器 `(16,0,112,520)`（贴窗口顶边、完全在窗口内），滚轮后 offset 0 → 472，末项渲染在 y=448 落入容器；测试 `dropdown_menu_a_long_menu_fits_the_window_and_scrolls_to_its_end` 用**滚动偏移**断言（滚动改变渲染平移，节点布局坐标不变，故矩形永远看不出滚动）。

观察到的既有偏差（记录，不在本轮修）：偏移上限 472 vs M3 的 456（内容高含 8dp×2、视口不含，差 16px）。这是**所有带 padding 的滚动容器**共有的 off-by-padding，不是菜单特有。

### 4.4 阶段 4a 对齐：进出动画

M3 真身（`Menu.kt` 的 `DropdownMenuContent`）：`updateTransition(expandedState)` 驱动 `graphicsLayer` 的 `scaleX/scaleY/alpha`，目标值为 `ClosedScaleTarget = 0.8f` → `ExpandedScaleTarget = 1f`、`ClosedAlphaTarget = 0f` → `ExpandedAlphaTarget = 1f`，pivot 用 `transformOrigin = calculateTransformOrigin(anchorBounds, menuBounds)`。

| 项 | M3 | winia |
|---|---|---|
| scale | 0.8 → 1.0 | `OverlayAnimSpec::default_enter/exit`（`scale_from = 0.8`）——同一组目标值 |
| alpha | 0 → 1 | 同 spec 的 `fade` |
| pivot | 锚点交点（`calculateTransformOrigin`） | `OverlayAnimSpec::anchor_pivot` + `overlay_transform_origin()`（逐分支照搬 M3），仅菜单打开；其余 overlay 仍是内容中心 |
| 退出动画 | 反向播放同一 transition | `exit_anim`（`default_exit`，`clamp` 反向） |
| 时长/曲线 | `FastSpatial`（缩放）/ `FastEffects`（淡入），数值在 motion scheme 里 | winia 200ms 单一 ease 曲线（与 Dialog 同规格） |

**偏差（记录，不猜）**：M3 用**两条不同**的运动规格（缩放走 spatial、alpha 走 effects），而 winia 的 `OverlayAnimSpec` 只有一条曲线、scale 与 alpha 共用。后果是实测可见窗口很窄：alpha 达到 0.9 时 scale 已 ≈0.98，因此"肉眼可见的长大"很短暂（淡入明显、放大含蓄）。`FastSpatial`/`FastEffects` 的具体数值不在本地抽取的源码里，所以没有编数值。

**验证**（都是客观测量，不靠眼睛）：
- 动画开/关对照：**关掉 `enter_anim/exit_anim` 后** `dropdown_menu_animates_in_from_its_anchor` 必红（每一帧都是满尺寸面板），打开则绿——测试读的是**一次捕获内的两个点**（同一帧），断言存在"既非页面也非稳定面板"的过渡帧；
- 缩放分量单独验证（临时把 `scale_from` 夸大到 0.2 再 revert）：点击后 60ms 时面板横向只覆盖到 ~110 物理像素（稳定后 192），说明确实按 pivot 从锚点侧长大。

### 4.5 阶段 4b 对齐：键盘与关闭语义

M3 的真身只有两条（`Menu.kt` 里**没有任何键处理**——没有 `onKeyEvent`、没有箭头键导航、没有 focusRequester）：

1. `DefaultMenuProperties = PopupProperties(focusable = true)`（`androidMain/AndroidMenu.android.kt:194`）——菜单的 popup 在打开期间**占有键盘**；
2. 项的激活来自 `clickable` 本身（聚焦时 Enter/Space 触发 onClick，Compose clickable 的既有语义）。

| 行为 | M3 | winia |
|---|---|---|
| 打开期间键盘归谁 | popup `focusable = true` | `focus_scope: true`（原来 false——实测 Tab 会把焦点走到主树按钮 `dm-many-toggle`） |
| Tab | 在菜单内移动 | 焦点落在首个可聚焦项 `dm-item-new`，页面元素不再持有焦点 |
| 项激活 | `clickable` 的聚焦激活 | 复用框架既有的"聚焦节点 Enter/Space 触发 onClick"（`app.rs` 键分发）；实测 Tab→Enter 后 `dm-picked: new` 且菜单关闭 |
| Esc 关闭 | 平台 popup 的 dismiss | 本就可用（本轮补测试钉住），且不依赖 `focus_scope` |
| 关闭后焦点 | popup 还原 | 回到打开它的触发器（实测 Esc 后 `dm-toggle` 持有焦点） |
| 焦点标记 | 状态层（无描边） | 状态层来自**波纹元素**本身（`render.rs:1601`：`hover_opacity + focus_opacity`）；同时用 `no_focus_ring()` 关掉 winia 默认的焦点环——环横跨菜单会像行分隔线。实测聚焦后项内为 `(217,211,219)`（未聚焦 `(242,236,244)`，差 25），环关闭后外侧像素不再变化 |

**不做（因为 M3 里没有）**：箭头键在项间导航、Home/End、首字母跳转——这些在 Compose 属于应用层，`Menu.kt` 没有实现。不把"自造行为"当对齐。

### 4.6 待对齐（本轮后续阶段）

| 项 | M3 真身 | winia 现状 | 阶段 |
|---|---|---|---|
| 输入框下拉 | `ExposedDropdownMenuBox` | 无 | 5 |
| 定位候选的后两档 | `centerToAnchorTop` + 按锚点半边选贴顶/贴底边 | 只有 下→上→贴边 三档 | - |
| 项等宽 / 菜单取最宽项自然宽 | 菜单列 `width(IntrinsicSize.Max)` | 已由 `MenuColumnPolicy` 在菜单内实现（§4.2）；**框架层**仍无 intrinsic 测量，其它组件需自行照此实现 | 框架 |

### 4.7 有意保留的偏差

- **锚点由调用方显式给出**（`build(ctx, anchor, menu)`）。M3 的 `DropdownMenu` 没有 anchor 参数，因为 popup 以“父布局节点”的 bounds 为锚（用法是把菜单与触发器放进同一个 `Box`）。winia 没有等价的隐式父锚点，故把锚点内容作为参数；语义等价（锚点即那块 `Box`），但形状不同 —— 记录而非隐藏。
