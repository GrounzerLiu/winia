# DropdownMenu（对齐 Compose material3）

`winia/src/ui/overlay.rs` 的 `DropdownMenu` / `DropdownMenuItem`，以及本文记录的对齐进度与偏差。

## 1. 现状 API

```rust
let expanded = ctx.remember(|| false);
Button::new().on_click(move || expanded.set(true)).build(ctx, |ctx| Text::new("打开").build(ctx));

DropdownMenu::new(expanded.clone())
    .on_dismiss_request(move || expanded.set(false))
    .build(ctx,
        |ctx| { /* 锚点：菜单定位到这块内容的下方 */ },
        |ctx| { DropdownMenuItem::new("新建文件").on_click(…).build(ctx); });

DropdownMenuItem::new("删除")
    .modifier(Modifier::new().test_tag("dm-item-delete"))  // 调用方 modifier，追加在内部样式外层
    .enabled(false)
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

## 3. 测试（`fixture_dropdown_menu` + `ui_test.rs`）

| 测试 | 钉住的行为 |
|---|---|
| `dropdown_menu_opens_and_an_item_pick_closes_it` | 打开后 overlay 条目出现且含各项文本；点项触发回调 `dm-picked` 且菜单关闭（`dm-open: no`），popup 条目被移除（不是留着继续吃点击） |
| `dropdown_menu_a_disabled_item_neither_fires_nor_dismisses` | `enabled(false)` 的项既不触发回调也不关闭菜单（对齐 Compose：该项根本不可点） |
| `dropdown_menu_dismisses_on_an_outside_click` | 点击菜单外 → 经 `on_dismiss_request` 关闭，且没有任何项被选中 |

## 4. 与 Compose M3 的差异（依据：本地 androidx 源码）

依据文件：`target/compose-src/commonMain/androidx/compose/material3/Menu.kt`、`androidMain/.../AndroidMenu.android.kt`、`commonMain/.../tokens/MenuTokens.kt`。

### 4.1 已对齐

| 项 | M3 | winia |
|---|---|---|
| 内容为列容器 | content: `ColumnScope.() -> Unit` | `build` 内部包 `Column`（见 §2） |
| 项可点击/禁用 | `clickable(enabled, onClick)`，禁用不可点 | 同（禁用时不挂 clickable） |
| 默认无进出动画 | （platform popup 默认） | `enter_anim/exit_anim = None` |
| 外部点击关闭 | `onDismissRequest` | `dismiss_on_outside: true` + `on_dismiss_request` |

### 4.2 待对齐（本轮后续阶段）

| 项 | M3 真身 | winia 现状 | 阶段 |
|---|---|---|---|
| 容器形状 | `MenuTokens.ContainerShape` = CornerExtraSmall（4dp） | 项自带圆角 4（容器无形状） | 2 |
| 容器色 | `surfaceContainer` | 项背景写死 `(255,250,250)`，容器无色 | 2 |
| 阴影/tonal | shadow `Level2`、tonal `Level0` | 无 elevation | 2 |
| 项几何 | `sizeIn(minWidth 112dp, maxWidth 280dp, minHeight 48dp)` + `padding(12dp, 0)` | 写死 `size(160, 36)` + `padding(12)` | 2 |
| 容器垂直内边距 | `8dp`（`DropdownMenuVerticalPadding`） | 无 | 2 |
| 文本样式 | `labelLarge` | 写死 13px + 硬编码颜色 | 2 |
| 图标槽 | `leadingIcon` / `trailingIcon`（前导最小 24dp，文本 `weight(1f)`） | 无 | 2 |
| 参数 | `offset: DpOffset`、`scrollState`、`shape/containerColor/tonalElevation/shadowElevation/border`、`colors`、`contentPadding`、`interactionSource` | 只有 `expanded` / `on_dismiss_request`（项有 `modifier`） | 2 |
| 长菜单滚动 | 平台 popup 把 `scrollState` 接给 `verticalScroll` | **不可滚动**（overlay 按窗口约束测量，内容超出即够不到） | 3 |
| 键盘 | Esc 关闭、上下键移动、Enter 激活 | 未测（`DropdownMenu` 未标 focus_scope） | 4 |
| 输入框下拉 | `ExposedDropdownMenuBox` | 无 | 5 |

### 4.3 有意保留的偏差

- **锚点由调用方显式给出**（`build(ctx, anchor, menu)`）。M3 的 `DropdownMenu` 没有 anchor 参数，因为 popup 以“父布局节点”的 bounds 为锚（用法是把菜单与触发器放进同一个 `Box`）。winia 没有等价的隐式父锚点，故把锚点内容作为参数；语义等价（锚点即那块 `Box`），但形状不同 —— 记录而非隐藏。
- **项目前自带背景与固定尺寸**（§4.2 待改），所以现在三项叠在一起看起来像三个白块，而不是 Compose 的单块菜单面板。
