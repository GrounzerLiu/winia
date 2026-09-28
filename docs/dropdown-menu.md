# DropdownMenu（对齐 Compose material3）

`winia/src/ui/overlay.rs` 的 `DropdownMenu` / `DropdownMenuItem`，以及本文记录的对齐进度与偏差。

## 0. 运行

```bash
cargo run -p winia --example dropdown_menu_demo --features debug-server
```

demo 里按顺序演示：① 材质默认（单面板 + 禁用项）② 30 项长菜单（封顶 / 贴边 / 可滚动）③ 样式参数（`offset` / `shape` / `shadow_elevation` / 项 `colors` / `content_padding`）④ 贴近窗口底部的触发器（菜单**向上翻**——M3 候选序列的第二个）。

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
| 项颜色 | `MenuItemColors`：文本 `OnSurface`、图标 `OnSurfaceVariant`、禁用 = 同角色 @38%（`ListItemDisabled*Opacity`） | `MenuItemColors` 六个字段同名同义 + `defaults()`；禁用走 `0.38` alpha（与 navigation 组件同一常量值） |
| `contentPadding` | `PaddingValues(horizontal = 12dp, vertical = 0)` | `content_padding(h, v)`，默认 `(12, 0)` |

几何有测试钉住（树断言，不是像素）：`dropdown_menu_geometry_matches_the_material3_metrics` —— 项宽必须落在 `[112, 280]`（对 4 个汉字的标签即证明 minWidth 钳制生效：内容只有 ~80px）、项高 ≥48、容器高 = 3 项 + 上下 8dp（≥160 且 <200）。旧几何（写死 160×36）会因项高 36 与容器高 108 两条断言失败。

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

### 4.4 待对齐（本轮后续阶段）

| 项 | M3 真身 | winia 现状 | 阶段 |
|---|---|---|---|
| 键盘 | Esc 关闭、上下键移动、Enter 激活 | 未测（`DropdownMenu` 未标 focus_scope） | 4 |
| 输入框下拉 | `ExposedDropdownMenuBox` | 无 | 5 |
| 前导/尾随图标槽 | `leadingIcon` / `trailingIcon`，最小 24dp（`ListTokens.ListItemLeading/TrailingIconSize`），文本区在有图标的一侧补 12dp | 无（`MenuItemColors` 已预留这两个颜色字段） | 2b |

### 4.5 有意保留的偏差

- **锚点由调用方显式给出**（`build(ctx, anchor, menu)`）。M3 的 `DropdownMenu` 没有 anchor 参数，因为 popup 以“父布局节点”的 bounds 为锚（用法是把菜单与触发器放进同一个 `Box`）。winia 没有等价的隐式父锚点，故把锚点内容作为参数；语义等价（锚点即那块 `Box`），但形状不同 —— 记录而非隐藏。
- **项目前自带背景与固定尺寸**（§4.2 待改），所以现在三项叠在一起看起来像三个白块，而不是 Compose 的单块菜单面板。
