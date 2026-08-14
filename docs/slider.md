# Slider 组件（material3 对齐）

> 分支：`slider`（从 v2 分出）
> 对标：material3 `Slider`（新版 M3 token v2_3_5——Compose androidx-main Slider.kt）
> M3 规格：https://m3.material.io/components/sliders/specs

## 1. API

```rust
Slider::new(value: f32)                       // 对标 Slider(value, ...)
    .on_value_change(|v: f32| {})             // 对标 onValueChange（拖动/点击/键盘每步）
    .on_value_change_finished(|| {})          // 对标 onValueChangeFinished
    .value_range(min: f32, max: f32)          // 对标 valueRange（默认 0..1）
    .steps(i32)                               // 对标 steps（>0 离散；0 连续）
    .enabled(bool)
    .colors(SliderColors)
    .interaction_source(source)               // hoist
    .modifier(Modifier)
    .build(ctx);
```

- **受控组件**：value 由调用方持有，`on_value_change` 更新（与 Checkbox 同模式）。
- **steps 语义**（对齐 Compose）：steps=4 时值域 0..10 允许 2/4/6/8
  （两端之间 4 个等距值）；拖动/点击自动吸附最近刻度。
- **交互**：点击跳转（tap 位置即值）、拖动连续跟随（绝对位置换算）、
  键盘（聚焦后 ←/→ 1 步 = 1% 值域或 1 档、PageUp/Down 大步、Home/End 端点）。

## 2. 默认值（对标 `SliderTokens` v2_3_5 / M3 specs）

| 项 | 值 |
|---|---|
| 轨道高度 | 16（`ActiveTrackHeight/InactiveTrackHeight`） |
| 轨道形态 | **两段独立胶囊**（active/inactive 与 thumb 不粘连） |
| 拇指-轨道间隙 | 6（`ThumbTrackGapSize = ActiveHandleLeadingSpace`） |
| 轨道圆角 | 外端全圆 8 / 内端 2（`TrackInsideCornerSize`） |
| 拇指尺寸 | 4×44（`HandleWidth×HandleHeight`，CornerFull 胶囊） |
| 交互中拇指宽 | 2（press/drag/focus 时减半——Compose ThumbContent） |
| 刻度点 | 4（`StopIndicatorSize`） |
| 触摸目标 | 48 高（`minimumInteractiveComponentSize` 语义） |
| 键盘默认步进 | 1% 值域（steps=0 时；Compose `actualSteps = 100`） |

### 颜色（`SliderColors::from_theme`——对齐 Compose `defaultSliderColors`）

| 部件 | enabled | disabled |
|---|---|---|
| thumb | Primary | OnSurface @ 38% |
| active track | Primary | OnSurface @ 38% |
| inactive track | SecondaryContainer | OnSurface @ 12% |
| active 区 tick | SecondaryContainer（交叉对比） | OnSurface @ 12% |
| inactive 区 tick | Primary（交叉对比） | OnSurface @ 38% |

注：tick 色与轨道色**交叉**（active 区 tick 用 inactive 轨道色）——保证
浅/深轨道上都可见，与 Compose `defaultSliderColors` 一致。

## 3. 实现细节

- **架构**：winia 首个使用 `Modifier::draw()`（自定义 Canvas 绘制闭包）的组件
  ——track/thumb/tick 全部由绘制闭包完成（对齐 Compose `Canvas`）。
  该能力是新增框架原语（`ModifierElement::CustomDraw`），后续进度条等
  绘制型组件可复用。
- **轨道宽度通道**：交互回调（tap/drag）需要像素↔值换算，但组件无尺寸
  通知——draw 闭包渲染期把 `rect.width()` 写入 `set_silent` 的宽度 State
  （无重组副作用），交互回调读取。
- **点击/拖动**：**按下立即跳转**（`on_press` 即换算位置更新值——用户规范，
  不等 tap/drag）+ `on_tap`（finished）+ `on_drag_start`（拖动起点）+
  `on_drag`（绝对位置跟随，无累积误差）+ `on_drag_end`（finished）；
  与 Compose 的 tap + draggable 组合语义对齐（winia 手势系统已区分 tap/drag）。
- **交互发射**：手势回调显式发射 interaction——`on_press` emit_press（按下
  thumb 减半）、`on_drag_start` emit_drag_start、`on_drag_end/cancel`
  emit_drag_end + emit_release（对齐 Compose ThumbContent 收集 press/drag——
  手势系统本身不自动 emit，2026-08 实测补上）。
- **绘制（M3 轨道两段独立 + 端点对齐 stop）**：
  **轨道视觉占满组件宽度** `[0, w]`；**stop indicator 在轨道端头圆心**
  （`corner = 8dp`，不贴边）；**thumb 端点中心 = stop 中心**——滑动范围
  `[corner, w - corner]` 不占满（handle 正好停在 stop 上）；
  active track `[0, value_pos - end_gap]`（Primary，左端全圆 8dp、
  右端 2dp 小圆角）、inactive track `[value_pos + end_gap, w]`
  （SecondaryContainer，左端 2dp、右端全圆 8dp）；
  `end_gap = thumb宽/2 + 6dp`——**thumb 与轨道保持 6dp 间隙**
  （`ThumbTrackGapSize`），两段轨道不粘连；非对称圆角用
  `RRect::new_rect_radii`（每角独立半径——与旧版 v1 实现 physical_rrect_radii 同方案）；
  **stop indicator**：两段轨道外端头中心 4dp 圆点——**开始端**（active 段左端）
  离散时与同区 tick 同色（secondary_container，与其它 stop 一致）、连续时与
  active track 同色（primary）；**结束端**（inactive 段右端）与 active track 同色
  （primary——离散时恰好 = inactive tick 色，与 tick 一致）；
  **与 thumb 重合的 stop 不绘制**（tick 与端头 stop 都判定：中心距 < (thumb宽+
  stop直径)/2 = 4dp 即跳过——value 落在刻度上/端点时 thumb 不盖 stop）；
  **焦点环包围 thumb 胶囊**（非整个组件）——框架新增 `Modifier::no_focus_ring()`
  （禁用整组件自动环）+ `draw_focus` gap 参数化，slider 在 draw 闭包内渲染期读
  焦点状态/环透明度（`focus_indicator_alpha` 动画）复用 `render::draw_focus` 画环；
  **环带中心线 = 轨道端头**（距 thumb 中心 `end_gap` = 8dp）——环带间距与两个
  轨道端头距离一致（16dp），基准用静止 thumb 尺寸（交互变窄不影响环位）；
  **thumb 中心统一按 corner 内缩**：`corner + (w - 2×corner) × f`（连续/离散
  一致，端点落在 stop 上）；tick 同式内缩；tick 用 4dp 圆点，
  active track 范围内外分色；thumb 用圆角胶囊（交互中宽 2dp）；
  **点击/拖动换算同步**：`value_at_x` 用 `(x - corner) / (w - 2×corner)`
- **键盘**：`on_key_event` + `focusable`——KeyDown 步进、KeyUp 触发 finished
  （对齐 `slideOnKeyEvents`）；按键由焦点节点消费后不会触发框架焦点导航。
- **换算纯函数**（公开可测）：`tick_fractions` / `snap_value` / `value_at_x` /
  `fraction_from_value`。

## 4. 测试

9 个：刻度分数（Compose steps+2 语义）、吸附（最近刻度）、位置↔值换算、
颜色解析（含 tick 交叉与 disabled 透明度）、键盘步进（1%/档/Page/Home/End）、
像素级（value=0.5 时左 primary 右 secondary、**轨道与 thumb 间隙白像素**、
thumb 位置、steps 刻度内缩后双色）、
点击换算（x=100/200 → value=5 实测回调）。

## 5. 未实现项（对齐 Compose 差距）

- **RangeSlider**（双拇指范围滑块）——数据结构（RangeSliderState）与手势
  （双拇指拖动判定）均未实现；
- **M3 Expressive**：value indicator（值标签浮出）、vertical slider、
  centered track、自定义 thumb/track slot（Compose 高级重载）；
- **RTL 镜像**：winia 布局方向已支持，slider 未做 RTL 反转（fraction 恒从左）；
- **语义/无障碍**：`progressSemantics`（winia 暂无 semantics 系统）；
- 拖动 slop 判定由框架手势统一处理（Compose 的 pointerSlop 细节差异）；
- 新版 Compose 的 corner shrinking / inset focus ring（Expressive 轨道角处理）未实现。
