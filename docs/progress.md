# Winia v2 — 进度总结

> 更新日期：2026-07-19  
> 基于 v1 重构的声明式跨平台 GUI 框架，对标 Jetpack Compose

---

## 一、已完成 ✅

### 1.1 State 响应式系统 (`core/state.rs`)
- `State<T>` — 可观察值容器，`get()` / `set()` / `update()`
- `PartialEq` 去重，避免无效重组
- `SubscriberId` + `retain` 精确取消订阅
- **增量重组依赖桥接**：`register_dependency` → `record_dep` → `notify_state_changed` → `set_global_dirty`
- 全局脏标志 + `take_global_dirty()`
- 测试：4 个

### 1.2 Composer 组合引擎 (`core/composer.rs`)
- `ComposeCtx` — composable 函数入口，`remember()` / `next_key()` / `start_leaf` / `start_container`
- `Composer` — 管理 SlotTable + LayoutNode 树，`compose()` / `recompose()` / `layout()`
- 树形 SlotTable（path 栈 + 子节点列表），支持重组复用
- 全局 key 计数器每次 compose 重置
- **增量重组（slot 级脏追踪）**：
  - Slot 加 `dirty: bool` 字段
  - `SlotTable::dirty_keys` — 被 State 变化标记的 slot key 集合
  - `start_slot` 检查 dirty → clean slot 跳过 composable 执行
  - `slot_deps: HashMap<u32, Vec<u64>>` — state_id → slot_key 映射
  - thread_local `ACTIVE_SLOT_KEY` 桥接 compose 和 dependency registrar
- 测试：4 个

### 1.3 Modifier 链 (`modifier.rs`)
- `Modifier` — 不可变链式 API，17 种 `ModifierElement`
- Layout 类：`size` / `width` / `height` / `padding` / `padding_horizontal` / `padding_vertical` / `margin` / `fill_max_width` / `fill_max_height` / `fill_max_size`
- Draw 类：`background` / `border` / `clip`
- Content 类：`TextContent`
- Input 类：`clickable` / `focusable` / `scrollable` / `FocusRequesterId`
- `FocusRequester` — 代码请求焦点（`request_focus()`）
- `Dimension` / `Shape` / `Color` / `ScrollDirection` 辅助类型
- 测试：7 个

### 1.4 Layout 系统 (`layout.rs` + `layout/`)
- `Constraints` — 约束模型（min/max width/height, tighten, offset, loosen）
- `LayoutNode` — 布局节点（modifier + measured_size + position + children + measure_policy + focused）
- `MeasurePolicy` trait — `measure()` 返回 `(Size, Vec<Placement>)` + `place()`
- `ColumnLayout` / `RowLayout` / `BoxLayout`
- `measure_node` — 递归测量引擎，含 Skia 文字测量（`measure_text_size`）
- 命中测试：`hit_test()` — 深度优先遍历
- 焦点遍历：`focus_next()` / `focus_node()` / `focus_by_id()` / `collect_focusable()` / `get_focus_id()`
- 测试：8 个

### 1.5 UI 组件 (`ui.rs` + `ui/`)
- `Text` — 文本显示组件 Builder
- `Button` — 按钮组件 Builder（含 Clickable 自动注入）
- `Column` / `Row` / `Stack` — 布局 composable
- 测试：5 个

### 1.6 渲染管线 (`render.rs`)
- 递归遍历 LayoutNode 树
- `Background` / `Border` 用 Skia Paint 绘制
- `TextContent` 用 Skia Paragraph API 渲染文字（thread_local FontCollection 缓存）
- 焦点环：蓝色 2px 描边

### 1.7 应用壳 (`app.rs`)
- `run_app(content, width, height)` 入口
- winit 0.31 事件循环 + `ApplicationHandler`
- 窗口创建（`can_create_surfaces` 内首次 compose+layout+render + `set_visible(true)`）
- 完整渲染循环：compose → layout → render
- PointerButton 事件 → 命中测试 → Clickable 触发 → request_redraw
- KeyboardInput Tab 键 → 焦点切换 + request_redraw
- SurfaceResized 物理→逻辑坐标，忽略 winit bug #2094 的中间态
- ScaleFactor 处理 + `canvas.scale(sf, sf)` HiDPI 渲染
- `ControlFlow::Wait` 空闲省 CPU，pending 时 `Poll`
- EventLoopProxy 唤醒回调

### 1.8 动画系统 (`animation.rs`)
- `Easing` — 5 种缓动（Linear / EaseIn / EaseOut / EaseInOut / BounceOut）
- `animate_as_state(initial, target, duration, easing)` → `State<f32>`
- `animate_to(state, target, duration, easing)` — 改变目标值
- `animate_to_cb(..., on_finish)` — 完成回调
- 全局动画列表 + `tick()` 每帧推进
- 有活跃动画时自动 request_redraw

### 1.9 焦点系统
- **FocusRequester** — `FocusRequester::new()` + `request_focus()`，支持 `&fr` 不消耗所有权
- **Modifier.focus_requester(fr)** — 关联 FocusRequester
- **Tab 键切换** — `focus_next()` 深度优先遍历
- **点击聚焦** — PointerButton 命中时自动聚焦
- **焦点持久化** — `AppState.focused_id` 跨 compose 保持，compose 后自动恢复
- **焦点环渲染** — 蓝色 2px 描边
- **get_focus_id()** — 从当前焦点节点提取 FocusRequesterId
- 文档：`docs/focus-system.md`

### 1.10 DevTools (`debug.rs`，可选 feature)
- 本地 HTTP 服务器（`http://localhost:9999`）
- `/screenshot` — BMP 截图（按需 GPU readback）
- `/tree` — 组件树 JSON（pos / size / modifier / focused / children）
- `/click?x=...&y=...` — 逻辑坐标模拟点击
- `/shutdown` — 优雅退出
- `/event?type=key|scroll|focus_next` — 通用事件
- `debug-server` feature flag，默认不启用
- EventLoopProxy 唤醒 + ControlFlow::Poll 响应

### 1.11 基础设施
- **树形 SlotTable** — path 栈 + 子节点列表，重组 key 匹配
- **HiDPI** — scale_factor 正确应用于坐标转换和渲染
- **文字测量修复** — `measure_text_size` 先 layout 到大宽度再读 intrinsic width，避免 padding 约束导致错误换行
- **winit bug #2094 修复** — 忽略启动时超大 SurfaceResized（如 1265×725 → 400×300）

### 1.12 测试覆盖
| 模块 | 测试数 |
|------|--------|
| core/state + composer | 4 |
| modifier | 7 |
| layout | 8 |
| ui | 5 |
| input/focus | 3 |
| **总计** | **31** |

### 1.13 workspace 结构
```
D:\Projects\winia\
├── winia/                     # UI 框架核心
│   ├── src/
│   │   ├── core.rs + core/    # State + Composer (+ 增量重组)
│   │   ├── modifier.rs        # Modifier 链 + FocusRequester
│   │   ├── layout.rs + layout/# 布局引擎 + hit_test + focus 遍历
│   │   ├── ui.rs + ui/        # 组件层
│   │   ├── render.rs          # 渲染管线
│   │   ├── animation.rs       # 动画系统
│   │   ├── debug.rs           # DevTools（可选）
│   │   └── app.rs             # 应用入口
│   └── examples/counter.rs
├── docs/                      # 文档
│   ├── architecture.md
│   ├── progress.md
│   └── focus-system.md
├── skiwin/                    # Skia 渲染后端 (Vulkan/GL/CPU)
├── proc-macro/                # 过程宏（v1 遗留）
└── material_color_utilities/  # Material 颜色工具（v1 遗留）
```

---

## 二、待完善 🟡

### 2.1 Composer
| 问题 | 影响 |
|------|------|
| `request_recomposition` 只设 flag，未接入 RedrawRequested | State 变化不自动触发重组（靠手动 request_redraw） |
| 增量重组跳过 clean slot 但**未复用 LayoutNode** | clean 子树仍重建 LayoutNode |
| `recompose()` 方法未被使用 | — |

### 2.2 Text 渲染
| 问题 | 影响 |
|------|------|
| `TextAlign` 枚举定义了但 `draw_text` 没使用 | 右对齐/居中不生效 |
| `TextOverflow` / `max_lines` 没实现 | 长文本无法截断 |
| 字体测量和渲染各自创建 Paragraph | 重复的排版计算 |

### 2.3 Layout
| 问题 | 影响 |
|------|------|
| LazyColumn 未实现 | 长列表无法虚拟滚动 |
| ~~ScrollArea~~ 已完成 | `vertical_scroll(ScrollState)` 修饰符 |
| 没有 `Spacer` / `Divider` composable | — |

### 2.4 输入事件
| 问题 | 影响 |
|------|------|
| 仅处理 Pressed 事件 | 无 Released / Moved / Drag |
| 无滚轮事件 | 无法滚动 |
| 无 IME 输入 | TextField 做不了 |
| 无手势识别器 | 双击/长按/滑动不支持 |

### 2.5 焦点系统
| 问题 | 影响 |
|------|------|
| 无 Shift+Tab 反向遍历 | — |
| ~~request_focus 依赖 debug~~ | 已修复：APP_PROXY 唤醒 + focused_id 同步 |
| 焦点环样式不可配 | 硬编码蓝色 |
| 无 `onFocusChanged` 回调 | — |

### 2.6 Render
| 问题 | 影响 |
|------|------|
| 每次全量遍历绘制 | 无脏区域优化 |
| `Clip` modifier 未实现渲染 | — |
| 无图片渲染 | Image 组件无法实现 |

### 2.7 平台
| 问题 | 影响 |
|------|------|
| 仅测试了 Windows + Vulkan | macOS/Linux/GL/CPU 后端未验证 |

---

## 三、未完成 🔴

### 3.1 组件
- [ ] `TextField` — 文本输入框（需 IME + 键盘事件 + 光标）
- [ ] `Checkbox` / `Switch`
- [ ] `Slider` / `ProgressIndicator`
- [ ] `Image` / `Scaffold` / `Dialog`

### 3.2 布局
- [ ] `LazyColumn` / `LazyRow` — 虚拟滚动（有 ScrollState 基础）
- [x] `ScrollArea` — `vertical_scroll(ScrollState)` 修饰符

### 3.3 主题系统
- [ ] 颜色方案（继承 `material_color_utilities`）
- [ ] 字体排版 / 形状系统 / 暗色模式

### 3.4 动画
- [ ] `AnimatedVisibility` — 进入/退出过渡
- [ ] 颜色动画 / Spring 动画 / 循环动画

### 3.5 输入
- [ ] IME 支持 / 手势识别器框架 / 拖拽 / 右键菜单

### 3.6 开发体验
- [ ] 更多示例 / API 文档 / 错误处理 / 性能基准
