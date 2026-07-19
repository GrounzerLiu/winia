# [P1] [scroll] 对齐 Compose — ScrollState + layout偏移 + render只clip

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度


| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. ScrollState 封装 | 🔄 进行中 | ScrollState 包裹 State f32, animateScrollTo, maxValue |
| 2. 偏移移到 layout | ⏳ 待开始 | measure_node 布 scroll 后调整 child.position.y |
| 3. render 只 clip 不移 | ⏳ 待开始 | 移除 canvas.translate，只保留 clip_rect |
| 4. 更新示例 | ⏳ 待开始 | Counter 用 rememberScrollState 替代裸 State |
| 5. 测试+文档 | ⏳ 待开始 | cargo test, update docs/tasks/scroll.md |

- **进度类型**：sequential（顺序）
- **完成进度**：0/5（当前：1/5）

