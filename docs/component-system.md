# 组件系统架构与 TextField API 重设计划

## 一、当前 Builder 模式

所有组件采用 **`self` move 链式 builder**，每次调用消费 `self` 并返回新 `Self`，链尾以 `build(ctx)` 或 `build(ctx, |ctx| {})` 终结。

### 叶子组件（无子节点）
```rust
// Text / TextField
Text::new("hello")
    .font_size(24.0)
    .color(Color::RED)
    .build(ctx);

TextField::new(state, |v| {})
    .modifier(Modifier::new().size(300.0, 36.0))
    .build(ctx);
```
内部调用 `ctx.start_leaf(key, modifier)` → `ctx.end_node()`。

### 容器组件（有子节点）
```rust
// Button / Column / Row / Stack
Button::new()
    .on_click(|| println!("clicked"))
    .modifier(Modifier::new().size(200.0, 48.0))
    .build(ctx, |ctx| {
        Text::new("Click me").build(ctx);
    });

Column::new()
    .spacing(8.0)
    .build(ctx, |ctx| {
        Text::new("Row 1").build(ctx);
        Text::new("Row 2").build(ctx);
    });
```
内部调用 `ctx.start_restartable_group(key, modifier, policy)` → 匹配 `GroupStatus::Enter`/`Skip` → `ctx.end_restartable_group()`。

## 二、状态管理

- `ctx.remember(|| init)` — 持久化状态，重组时返回同一 `State<T>` 实例
- `State::new(v)` — 独立状态（可在外部创建）
- `.get()` / `.set(v)` / `.update(|v| *v += 1)` — 读写
- 机制：`ComposeCtx::next_remember_key()` 生成 `(group_key << 32) | counter` 定位 slot

## 三、主题颜色

```rust
let theme = crate::ui::theme::WiniaTheme::colors();
// theme.primary, theme.on_surface, theme.secondary_container, ...
// theme.surface_variant, theme.outline, etc.
```

## 四、Modifier 系统

- `Modifier` 是不可变链式元素列表
- `self.modifier = self.modifier.then(modifier)` 追加（外 → 内）
- `build()` 中手动追加内部 modifier 元素（如 Text 的 `TextContent`、Button 的主题背景）
- 支持 `custom(node: impl ModifierNode)` 扩展
- 元素分类：`Layout`, `Draw`, `Input`, `Content` 四类

## 五、TextField 当前状态

```rust
pub struct TextField {
    value: State<TextFieldValue>,
    on_value_change: Box<dyn Fn(TextFieldValue) + Send + Sync>,
    modifier: Modifier,
}
```

当前是**叶子组件**，无 label/placeholder 支持。

### 已实现功能
- 点击定位光标（grapheme 边界）
- 方向键（← →）按 grapheme 边界移动
- Shift+方向键扩展选区
- 文字输入、退格、Delete（含选区处理）
- IME 中文输入 + 预输入下划线
- 光标闪烁（tokio 500ms）
- Tab 切换焦点、Home/End 跳转
- 选区高亮渲染

### 内部通信
- `ctx.set_current_node_cursor_and_callback()` — 直接通过 `node_stack.last()` 设光标
- `ctx.sync_composing_range()` — 同步 IME 组合范围到节点
- `ctx.sync_selection_range()` — 同步选区范围到节点
- IME Preedit 通过独立 `ime_callback` 直通（不经过 KbEvent）

## 六、TextField API 重设计划

### 目标 API（对标 Jetpack Compose）

```rust
// 推荐方式
let state = remember_text_field_state("");

OutlinedTextField::new(state, |v| { /* 外部同步 */ })
    .label(|ctx| { Text::new("用户名").font_size(12.0).build(ctx); })
    .placeholder(|ctx| { Text::new("请输入用户名").build(ctx); })
    .single_line()
    .modifier(Modifier::new().fill_max_width())
    .build(ctx);
```

### 变化清单

| 方面 | 当前 | 目标 |
|------|------|------|
| 组件类型 | 叶子 | 容器（基于 `start_restartable_group`） |
| 布局 | 单节点 | 内部 Column：[label, 输入区, placeholder 叠加] |
| 参数 | value + callback + modifier | + label 闭包 + placeholder 闭包 + 行限制参数 |
| 行限制 | 无 | `single_line()` → `max_lines(1)` |
| 辅助函数 | 无 | `remember_text_field_state(ctx, initial)` |
| floating label | 无 | 聚焦时 label 上浮（动画/颜色变化） |

### 需要读的文件
- `winia/src/ui/text.rs` — Text 渲染模式
- `winia/src/ui/button.rs` — 容器 builder + 主题色
- `winia/src/ui/layout_components.rs` — Column/Row 布局
- `winia/src/core/composer.rs` — ComposeCtx 方法
- `winia/src/modifier.rs` — Modifier 链
- `winia/src/render.rs` — 文本+光标+IME 渲染

### 注意事项
- label/placeholder 是闭包而非字符串 → 启动画/floating 更方便
- 输入区需复用现有叶子逻辑（TextContent + 光标 + 选区 + IME）
- placeholder 在输入框有内容或聚焦时隐藏
- OutlinedTextField 的边框/标签位置需额外 layout 层

## 七、动画系统（待实现）

尚未开始。需实现后方能支持 floating label 动画。

## 八、下一阶段

1. ✅ TextField 核心功能（输入/光标/选区/IME）
2. ⏸️ AP 重设计（等待指令）
3. ❌ 动画系统（等待指令）
