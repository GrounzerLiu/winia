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
    .leading_icon(|ctx| { Icon::svg_path(…).build(ctx); }) // M3 leadingIcon（24dp 盒）
    .trailing_icon(|ctx| { Text::new("Ctrl+C").build(ctx); }) // M3 trailingIcon
    .on_click(…)
    .build(ctx);
```

输入框下拉（`ExposedDropdownMenuBox`，M3 同名组件）：

```rust
let expanded = ctx.remember(|| false);
let value = ctx.remember(|| TextFieldValue::new(""));
ExposedDropdownMenuBox::new(expanded.clone())
    .on_expanded_change({
        let expanded = expanded.clone();
        move |open| expanded.set(open)
    })
    .anchor_type(ExposedDropdownMenuAnchorType::PrimaryNotEditable) // 只读字段：点击切换
    .match_anchor_width(true)                                       // 菜单宽度 = 输入框宽度（M3 默认）
    .build(ctx,
        |ctx| {  // 锚点：输入框（点击它即切换展开态）
            let value = value.clone();
            let arrow = expanded.clone();
            TextField::new(value).outlined().read_only(true)
                .trailing_icon(move |ctx| {
                    ExposedDropdownMenuDefaults::trailing_icon(ctx, arrow.clone(), Modifier::new())
                })
                .build(ctx);
        },
        |ctx| {  // 项：用 16dp 水平内边距（M3 ExposedDropdownMenuItemHorizontalPadding）
            let field = value.clone();
            let closer = expanded.clone();
            DropdownMenuItem::new("选项 A")
                .content_padding(ExposedDropdownMenuDefaults::ITEM_HORIZONTAL_PADDING, 0.0)
                .on_click(move || {
                    // The caller's half of an exposed dropdown — material3's samples write the field's
                    // value here too, because the box does not own it: the field shows what was picked.
                    field.set(TextFieldValue::new("选项 A"));
                    closer.set(false);
                })
                .build(ctx);
        });
```

注意两处**调用方职责**（M3 同样如此）：选中项后把标签写回输入框的值（`TextFieldValue::new` 会把光标放到末尾并清掉 IME 组合范围）；`trailing_icon` 收的是 `State<bool>` 而非 bool（原因见 §4.8）。

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
| `exposed_dropdown_menu_matches_its_anchor_width` | 200dp 字段：项宽 = 字段宽（实测 `(16,367,200,48)` vs `(16,303,200,56)`）、菜单挂在字段下方、标签距项左 16dp |
| `exposed_dropdown_dismisses_and_reports_it` | 外部点击关闭并回写 `onExpandedChange`，popup 条目随后消失（等退场动画） |
| `exposed_dropdown_primary_editable_anchor_does_not_toggle` | `PrimaryEditable` 的锚点点击不打开菜单 |
| `exposed_dropdown_trailing_icon_is_inside_the_field_and_rotates` | 尾随箭头画在字段内（扫描 5/25 点着色），且展开时翻转（扫描 3/25 点变化）——修复前分别是 0/25 与 0/25 |

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

阶段 2b 当时用手写的 `MenuColumnPolicy` 补了两遍测量（自然宽 → 紧约束重测）。**该策略已删除**：框架层现在有了 intrinsic 协议（`IntrinsicSize` + `MeasurePolicy` 四个默认方法 + 修饰符链反演，见 `docs/intrinsic-size.md` §8），菜单于是就是 M3 的原文——`Column(modifier.padding(vertical = 8.0).width(IntrinsicSize.Max).vertical_scroll(...))`：
1. **自然宽**由列自己的 `width(IntrinsicSize.Max)` 得出：每项的固有宽 = 其内容夹在项自身的 `sizeIn(112, 280)` 里（项的 `fillMaxWidth()` 刻意不计入固有宽，与 Compose 的 `FillNode` 保持默认近似一致）；
2. 列把该宽度**紧约束**传给子节点（管线第 1.5 步），项再靠自己的 `fillMaxWidth()` 撑满，加权标签在行内的余量分配于是与 M3 完全一致。

实测（fixture 图标菜单）：`lead=112 trail=112 menu=112` ✓ 等宽、菜单 = 项宽、且 < 280（最宽项自然宽）✓；尾随图标 x=92 = 项右缘 − 12 ✓。demo 里"复制 / Ctrl+C"那种菜单宽度为 **136**（此前是 280 ✗）。

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

**已修**（本轮核对时确认）：这一行记录的 off-by-padding 已被后续的滚动末端夹取修复消除——`winia/src/layout/node.rs` 的 `fling_limit` 与 `winia/src/app.rs` 的 `apply_scroll_delta_inner` 现在都用容器**自身的** `measured_size`（而非 padding 扣减后的视口）算上限，16px 的差没有了；`dropdown_menu_a_long_menu_fits_the_window_and_scrolls_to_its_end` 断言 `offset == 20*48 + 16 - 容器高`，注释里写着 "used to work from the padding-DEDUCTED viewport instead, which let the content scroll 16px too far"。所以这一条不再是偏差，原记录保留作历史。

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

### 4.6 本轮之前遗留的其他偏差（阶段 2b 一并处理）

| 项 | M3 真身 | winia 现状 |
|---|---|---|
| 定位候选的后两档 | `centerToAnchorTop` + 按锚点半边选贴顶/贴底边 | 只有 下→上→贴边 三档（见 §4.9） |
| 项等宽 / 菜单取最宽项自然宽 | 菜单列 `width(IntrinsicSize.Max)` | 同（框架 intrinsic 协议，§4.2；手写的 `MenuColumnPolicy` 已删除） |

### 4.7 阶段 5 对齐：ExposedDropdownMenuBox（输入框下拉）

M3 的契约（`ExposedDropdownMenu.kt` + `androidMain/ExposedDropdownMenu.android.kt`）：

| M3 | 内容 | winia |
|---|---|---|
| `ExposedDropdownMenuBox(expanded, onExpandedChange, modifier, content)` | 盒子持有展开态，`menuAnchor` 记录输入框 bounds | `ExposedDropdownMenuBox::new(expanded)` + `.on_expanded_change(cb)` + `.build(ctx, anchor, menu)`（锚点/菜单两个闭包，同 `DropdownMenu`） |
| `Modifier.menuAnchor(type, enabled)` | 点击策略、焦点、键盘 | `.anchor_type(...)` / `.enabled(...)`：`PrimaryNotEditable`、`SecondaryEditable` 点击切换；`PrimaryEditable` **不切换**（点击归光标） |
| `ExposedDropdownMenu(matchAnchorWidth = true, …)` | 菜单宽度**强制**等于输入框宽（`exposedDropdownSize`：`minWidth = maxWidth = menuWidth`） | `.match_anchor_width(true)`（默认 true）+ 框架侧 `OverlayDesc::match_anchor_width`：测量期解析锚点矩形，用 `min=max=锚点宽` 约束；内容侧 `fill_max_width`（因为 winia 的 flex 会在交叉轴把 min 归零，强制宽度到不了子节点，见 `layout/flex.rs`） |
| `ExposedDropdownMenuItemHorizontalPadding = 16dp` | 输入框下拉的项水平内边距是 **16dp**（普通菜单 12dp） | `ExposedDropdownMenuDefaults::item_content_padding()`（= `(16, 0)`） |
| `TrailingIcon(expanded)` = `Icons.Filled.ArrowDropDown` + `rotate(if (expanded) 180f else 0f)` | 静态旋转（这一版无动画） | `ExposedDropdownMenuDefaults::trailing_icon(ctx, expanded)` |

**实测**（fixture：200dp 宽的字段）：锚点 `(16,303,200,56)`、项 `(16,367,200,48)` —— 菜单与字段等宽 ✓、挂在字段下方 ✓、标签距项左 16dp ✓。注意这只有在 `fill_max_width` 之后才成立：没有它时策略收到的是 `min_w=0, max_w=200`（flex 抹掉了 min），算出 112 宽 ✗。

另外修了一处框架语义：`DropdownMenu` 原先用 `composer_slot_key()` 取锚点，而那是**锚点闭包里最后组合的节点** ✗——对单节点闭包无害，但输入框的最后一个子节点是 24×24 的尾随图标 ✗（实测锚点被解析成 `(180,391.5,24,24)`）。现在用该 group 自己的 key（= 包装容器 ✓），与"popup 锚在父布局节点"的 M3 语义一致。

### 4.8 已修：`TextField` 的尾随槽被放到字段外（阶段 5 的遗留）

**症状**：输入框下拉的尾随箭头画在了**字段下方 32px** ✗（实测槽 `(260, 391, 13, 19)`、字段 `(16, 303, 280, 56)`）。与 `read_only`、取值、调用方设定宽度、有无 label 都无关；槽确实组合了（换成文本 `▼` 树里能看到 ✓），但扫字段自身那一行全是背景 ✗ —— 所以问题在 `TextField`，不在盒子。

**根因**（读代码 + 数字吻合）：`text_field_content_height` 在父容器给出有限高度时**直接返回该高度** ✗：

```rust
if constraints.max_height < 1.0e9 { constraints.max_height }   // 父给多少就用多少
```

槽的 y = `container_center - slot_h/2`，于是它在"父容器给的可用高度"里居中 ✗，而不是在**字段自己的盒子**里 ✓。实测自洽：`content_h = 195 → container_center = 97.5 → y = 88` ✓（= 实测槽的偏移 ✓）。这也解释了为什么 demo 里看不出来：那些字段在**滚动列**内，父给的是无界高度 ✓ 走的是正确分支 ✓。

**修法**：让它返回字段自身的盒子高 `constraints.constrain_height(input_height + supporting_h)` ✓——`constrain_height` 本身会尊重紧约束，所以固定高字段（40dp pill）依旧在自己的 40 里居中 ✓。

**验证**：`exposed_dropdown_trailing_icon_is_inside_the_field_and_rotates` 从 `#[ignore]` 转为通过 ✓，数字：槽内着色扫描点 **5/25**（修复前 0/25 ✗）、展开后变化 **3/25**（箭头翻转 ✓）。

**顺带修的一条 API 语义**：`ExposedDropdownMenuDefaults::trailing_icon` 收 **`State<bool>`** 而不是 `bool`（M3 收 bool）。原因：Compose 会比较参数、参数变了就重跑，而 winia 的组不比较参数 ✗ —— 用 bool 时图标只组合一次、之后被 skip ✗，箭头永远停在同一朝向（实测：展开前后 16 个探测点全无变化 ✗）。在图标**自己的组合里**读状态才会注册依赖 ✓，这是 winia 表达"变了要重画"的方式。签名按 M3 保留 `modifier` 参数（`TrailingIcon(expanded, modifier)`）✓，调用方可以挂 tag 或调尺寸。

### 4.9 已修：只读输入框不该画光标

M3 的真身（`foundation/text/CoreTextField.kt:449`）：

```kotlin
val showCursor = enabled && !readOnly && windowInfo.isWindowFocused && !state.hasHighlight()
```

**只读字段不画光标** ✓。winia 的渲染条件只有 `focused && !has_selection`（`render.rs`）✗ —— 只要能聚焦就画 ✗，于是输入框下拉那块**不能输入的**只读字段会闪一根光标 ✗（上一轮 demo 截图里 `选项 A|` 那道竖线就是它 ✓）。

**修法**：把 `read_only` 挂到容器元素 `TextFieldVisual` 上（M3 也是从字段状态取的 ✓），渲染端沿父链读它（与取光标色 `text_field_visual_color` 同一条父链 ✓），条件变成 `enabled && !read_only` ✓；裸字段（无 variant ✓）没有该元素，默认仍画光标 ✓（与既有行为一致 ✓）。

**验证**：单测 `a_read_only_field_draws_no_caret`（去掉 `read_only` 项即红 ✓，且第二个用例要求可编辑字段仍画光标 ✓，防止"永远返回 false"式的假通过 ✓）；demo 截图里聚焦的只读字段已无光标 ✓。

### 4.10 定位候选补全（本轮）

原先只实现了 M3 候选序列的前两档（下→上）+ 一个 `clamp` 兜底 ✗。现在按 `MenuPosition.kt` 的工厂逐个照搬，抽成纯函数 `overlay::dropdown_menu_position`（便于逐个候选做确定性单测 ✓）：

| 轴 | 候选（按顺序取第一个"放得进窗口边距"的） | M3 出处 |
|---|---|---|
| 纵向 | 锚点下方 → 锚点上方 → **居中于锚点顶边**（`ay - menuH/2`） → **按锚点中心所在半窗贴顶/贴底**（`topToWindowTop`/`bottomToWindowBottom`，margin 48） | `MenuPosition.centerToAnchorTop` / `WindowAlignmentMarginPosition.Vertical` |
| 横向 | 起始对齐 → 末端对齐 → **按锚点中心所在半窗贴左/贴右**（margin 0） | `WindowAlignmentMarginPosition.Horizontal`（M3 的 `leftToWindowLeft(margin = 0)`） |
| 超出窗口时 | 菜单比边距带还高/还宽 → **在该轴居中**，而不是被推出窗口 | `WindowAlignmentMarginPosition`："If this is not possible, i.e. the menu is too tall, then it is centered" |

**验证**：单测 `dropdown_menu_position_follows_the_material3_candidates` 覆盖全部 5 个纵向候选（含"过高→居中"）+ 3 个横向候选（含两侧贴边）✓；现有 UI 测试的取值不变 ✓（长菜单仍是 424 高、`(520-424)/2 = 48` 正好等于 48dp 边距 ✓）。

### 4.11 原待对齐项（现已实现）

| 项 | M3 真身 | winia 现状 |
|---|---|---|
| 框架层 intrinsic 测量 | `IntrinsicSize.Max/Min` | **已实现**（公开 `IntrinsicSize` + `MeasurePolicy` 四默认方法 + 管线第 1.5 步，见 `docs/intrinsic-size.md` §8；菜单已改用 M3 原文链，`MenuColumnPolicy` 已删除） |
| `PrimaryEditable` 的可编辑锚点语义 | 指针三种锚点类型**都切换**；`PrimaryEditable` 的弹层**不带焦点**打开（保住光标与 IME），Tab/ArrowUp/ArrowDown 才把键盘交出去；空格不展开；Enter 视为点击 | **已实现**，见 §4.13。唯一保留的近似是 `SecondaryEditable`（winia 没有逐元素 `menuAnchor`，见 §4.13 与 §4.12） |

### 4.12 有意保留的偏差

- **锚点由调用方显式给出**（`build(ctx, anchor, menu)`）。M3 的 `DropdownMenu` 没有 anchor 参数，因为 popup 以“父布局节点”的 bounds 为锚（用法是把菜单与触发器放进同一个 `Box`）。winia 没有等价的隐式父锚点，故把锚点内容作为参数；语义等价（锚点即那块 `Box`），但形状不同 —— 记录而非隐藏。
- **`SecondaryEditable` 的逐元素锚点已实现，但两条附带条件仍是偏差**（§4.14）：① 弹层焦点性在 M3 里取决于"是否开启无障碍服务"，winia 没有该信号，故实现的是**非可访问分支**（不带焦点打开，字段保住光标与 IME）；② M3 给两个 primary 锚点发布的 `role = DropdownList` 在 winia 的 `SemanticsRole` 里还没有变体（只有 Button/Checkbox/Switch/RadioButton/Tab/Image/ProgressBar/Dialog），secondary 锚点的 `Button` + expanded 已发布并断言。

### 4.13 Editable anchors (`PrimaryEditable`): what was implemented

material3's anchor type decides whether the POPUP takes focus, not whether a click counts:
`Modifier.expandable` observes the pointer in the Initial pass and calls `onExpandedChange` on the up
event for every type (`ExposedDropdownMenu.kt:1430-1433`), while the type selects the popup's focus
behaviour through `popupPropertiesForAnchorType(anchorType, alwaysFocusable)` (`:354`). winia previously
read `PrimaryEditable` as "the click belongs to the text cursor", so its menu could not be opened with a
pointer at all, and the keyboard path did not exist.

| rule | material3 | winia |
|---|---|---|
| the pointer toggles, for every type | `:1430-1433` | `ExposedDropdownMenuBox` puts `.clickable(toggler)` on the wrapper for every type |
| the popup's focus follows the type | `:354`, plus `DefaultMenuProperties = PopupProperties(focusable = true)` (`androidMain/AndroidMenu.android.kt:194`) | `DropdownMenu::focus_scope(bool)` (new; default `true`); the box passes `false` for an editable anchor |
| Enter is a click | `isEnterMinusSpacebar` (`:1479-1489`) | the box's `.on_pre_key_event` toggles on Enter and consumes it |
| the spacebar must not expand an editable anchor | `:1444` "Primary editable shouldn't expand menu via spacebar" | the handler returns `false` for it, so the space belongs to the field |
| Tab / ArrowUp / ArrowDown hand the keyboard over while expanded | `alwaysFocusable = true` (`:1449-1457`) | the handler sets the remembered `keyboard` state, the popup becomes a focus scope again, and the framework claims the keyboard (`app.rs::claim_keyboard_for_overlay`) |
| Escape closes it | `BackHandler(enabled = expanded)` (`:247`), which exists because a NON-focusable popup sees no back events | already covered: `PerWindow::escape_key` closes the topmost overlay regardless of who owns the keyboard |

Mechanism notes worth keeping:

- The hand-over lives in a `State` created with `ctx.remember`, not a fresh `State::new`. A state built in
  the composable body is recreated on every recomposition, so the hand-over never outlived the frame that
  made it — measured: `exposed_dropdown_editable_anchor_hands_the_keyboard_over_on_a_reach_key` stayed red
  until this was fixed.
- The handler runs in the PREVIEW pass (`on_pre_key_event`) so it sees the key before the focused field
  does. winia's focused-node activation path (`app.rs`, "聚焦组件的键盘激活（对标 Compose clickable）")
  only fires the FOCUSED node's own `clickable`, which a text field has none of — a wrapper-level
  clickable alone could not have implemented the material3 rule.
- material3's `isClick` is the key UP event while winia activates on key DOWN, so the toggle is written
  against `KeyDown` + `!repeat`. A debug `k` command only produces key downs anyway.
- The spacebar arrives as `Key::Character(" ")` (winit 0.31 has no `NamedKey::Space`), the spelling
  winia's own activation path checks (`app.rs:4552`). `parse_debug_key` learned the name `Space` so the
  rule is testable.

Deviations kept, and why:

- `SecondaryEditable` behaves like `PrimaryNotEditable` (material3's accessible branch). In material3 that
  anchor lives on a separate element INSIDE the field (`Modifier.menuAnchor(...)` per element, which is
  also what consumes the pointer down so a click does not move the caret, `:1427-1429`). winia's
  `build(ctx, anchor, menu)` takes the anchor as one closure and has no per-element anchor modifier, so
  neither the down-consume nor the accessibility branch has a place to live.
- The anchor semantics (`role = DropdownList`, or `Button` + `stateDescription`/`contentDescription` for
  `SecondaryEditable`, `:1462-1477`) is not published by the box. The state is visible in the tree; the
  role mapping belongs to the semantics work.

API surface published with this round (all reachable as `winia::ui::…` now, previously only by full path):
`MenuDefaults`, `MenuItemColors`, `ExposedDropdownMenuBox`, `ExposedDropdownMenuAnchorType`,
`ExposedDropdownMenuDefaults`. `MenuDefaults` mirrors `Menu.kt:181-260` — TonalElevation, ShadowElevation,
shape, containerColor, itemColors, DropdownMenuItemContentPadding — and both components resolve their
unset fields THROUGH it, so the published value and the drawn one cannot drift.

Tests: `exposed_dropdown_editable_anchor_opens_without_taking_the_caret` (click opens, the caret stays,
the field keeps receiving characters, the spacebar does not toggle, Enter closes),
`exposed_dropdown_non_editable_anchor_opens_with_focus` (Tab lands on the first item immediately, no
hand-over step), `exposed_dropdown_editable_anchor_hands_the_keyboard_over_on_a_reach_key` (ArrowDown
hands the keyboard over, then Tab reaches the first item) in `winia/tests/ui_test.rs`;
`menu_defaults_are_material3s_tokens` and `menu_defaults_are_what_the_components_resolve_to` in
`winia/src/ui/overlay.rs`.

"Turn it off" proofs, each measured by reverting one term and re-running
`exposed_dropdown_editable_anchor_opens_without_taking_the_caret`:

- clickable excluded for an editable anchor → the menu never opened ("popup entries never showed
  `可编辑项`");
- `focus_scope(true)` for every anchor type → "an editable anchor's menu opens WITHOUT taking focus, so
  the field must still hold the caret";
- the spacebar guard removed → "the spacebar belongs to the text: material3's editable anchor must not
  toggle on it" (`left == right` failed).

### 4.14 Secondary anchors: the element inside the field

material3 hangs `menuAnchor(SecondaryEditable)` on an element INSIDE the field — a trailing icon — and that
element, not the field, owns the toggle (`ExposedDropdownMenu.kt:449-482`). Three things come with it: the
element consumes the pointer DOWN so the click does not move the caret (`:1427-1429`), the popup's
focusability is conditional on accessibility services being enabled (`:475-482`), and the semantics is a
`Button` that reports whether the menu is expanded (`:1462-1477`).

winia's shape for it is `ExposedDropdownMenuBox::build_with_anchor_modifier(ctx, |ctx, modifier| …, menu)`:
the closure receives the modifier to put on the inner element, and the box's own wrapper stays inert — no
`clickable` and, importantly, no press — so the click cannot be counted twice.

- **The press registration IS winia's `downEvent.consume()`.** winia has no consume flag in the pointer
  path; the equivalent is "be the innermost node on the hit path that declares a gesture", because a press
  is dispatched to exactly one node (`winia/src/app.rs::press_gesture_target`). A `Clickable` alone does
  NOT qualify — `Modifier::has_gesture` lists the tap and drag callbacks only — so an anchor element that
  carried `clickable` and nothing else would still let the field's own `on_press` (the caret placement,
  `text_field.rs`) fire. Measured three ways: the unit tests in `app::press_target_tests`, and the UI test
  `exposed_dropdown_secondary_anchor_toggles_from_its_icon_without_taking_focus`, which goes red with
  "the element owns the press, so the field must not take focus from a click on the icon" the moment the
  press registration is removed.
- **Focus policy per anchor type** now reads directly off `popupPropertiesForAnchorType`: a non-editable
  primary anchor opens WITH focus; an editable one opens without it and the reach keys hand it over; a
  secondary one is focusable only when accessibility services are on. winia has no accessibility-services
  signal (the model exists, the OS bridge does not — `docs/semantics-gap.md`), so the NON-ACCESSIBLE branch
  is the one implemented: the menu opens without focus and the field that shares its IME keeps the caret
  and keeps receiving characters. Recorded rather than faked — the conditional branch becomes reachable
  the day a bridge exists.
- **Semantics published**: `role = Button` plus the expanded state on the anchor element. Asserted through
  the published semantics snapshot (`secondary_anchor_semantics` in `ui_test.rs`), not just declared.

Test-harness lesson, worth keeping because it cost a round: `UiTest::click` sends the synthetic debug `c`
command, which focuses the deepest focusable node on the hit path BY DESIGN (so a following `k <char>` has
a target). A test about "this click must not steal focus" therefore has to drive the REAL pointer path —
`UiTest::tap` (a `d` + `u` pair) — or it measures the harness instead of the framework. Measured: the
focus assertion failed under `click` and passed under `tap` with no change to the component.

Tests: `exposed_dropdown_secondary_anchor_toggles_from_its_icon_without_taking_focus` (one toggle per
click, in both directions; the field does not take focus; the element reports button + expanded, then
button + collapsed) and `exposed_dropdown_secondary_anchor_keeps_the_caret_in_a_focused_field` (with the
field focused and holding text, clicking the icon opens the menu, keeps the keyboard in the field and the
caret usable). Library tests: `app::press_target_tests` pins the three press-target rules.

### 4.15 Planned: `PopupPosition` moves to Start/Center/End

`PopupPosition` (`winia/src/ui/overlay.rs:162`) names **absolute corners**: `TopLeft`, `TopCenter`,
`TopRight`, `Center`, `BottomLeft`, `BottomCenter`, `BottomRight`. Neither the enum nor the anchored
placement branch consults the layout direction — `app.rs:3667` resolves `BottomLeft` to `(ax, ay + ah)`,
pure geometry, and the unanchored branch to `(0.0, h - size.1)`. So an anchored popup aligns to the
anchor's geometric left edge under RTL exactly as it does under LTR.

That is wrong for anything that should follow the anchor's *start* edge. material3's `Popup` and
`DropdownMenu` take `Alignment`, which is direction-resolved (`Alignment.TopStart` is the left edge in LTR
and the right edge in RTL), so "below my anchor, aligned to its start" is expressible there and not here.
Measured on the docked date picker: the demo passes `BottomLeft` and looks right only because its
`TextField` does not mirror either — see `docs/date-picker.md` §RTL.

**The plan is to add Start/End variants and migrate to them**, not to make the existing corners mirror —
mirroring a name that says "Left" would be a lie, and would silently change every current caller. Shape:

- Add `TopStart` / `BottomStart` alongside the existing seven; resolve them against
  `WiniaTheme::direction()` at the placement site, which is the same point `Row` reads it
  (`ui/layout_components.rs:110`).
- Migrate the callers that mean "start": the docked date picker's popup, `ExposedDropdownMenuBox`, and the
  `DropdownMenu` trigger path. Callers that genuinely mean an absolute corner (`SearchBar`'s full-screen
  and collapsed overlays, `Tooltip`'s `TopCenter`) stay as they are.
- `fit_around_anchor`'s horizontal candidate sequence (§4.3) is the same question one level up: it walks
  "align to the anchor's start, then its end, then the window edge", which is direction-resolved in M3
  (`internal/MenuPosition.kt`) and geometric in winia today. It wants the same start/end treatment.

Nothing is implemented yet. This is the parking note so the gap is not rediscovered as a bug.
