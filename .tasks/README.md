# .tasks/ — 任务管理目录

此目录由 **task-keeper** MCP 工具自动管理。

## ⚠️ 重要提醒

**请不要手动创建、编辑或删除此目录下的任何文件和文件夹。**
手动修改可能导致：
- 元数据与文档不一致
- 进度丢失或状态错误
- 工具无法识别任务

## 📁 文件夹命名规则

每个任务对应一个独立文件夹，命名格式如下：

```
[P<重要程度>]<任务标题>_<状态>[+<未完成后续事项数>]
```

示例：

| 文件夹名 | 含义 |
|---|---|
| `[P0]实现登录_done` | P0 任务，已完成，无后续事项 |
| `[P0]实现登录_done+2` | P0 任务，已完成，有 2 个待处理的后续事项 |
| `[P1]优化队列_in-progress` | P1 任务，进行中 |
| `[P2]重构CLI_pending` | P2 任务，待开始 |
| `[P3]旧功能_cancelled` | P3 任务，已取消 |

**状态含义：** `done`（已完成）→ `in-progress`（进行中）→ `pending`（待开始）→ `cancelled`（已取消）

## 📄 元数据字段说明

每个任务文件夹包含 `.task-meta.json`，存储结构化元数据：

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 任务唯一标识 |
| `title` | string | 任务标题 |
| `importance` | string | 重要程度：P0 最紧急，P3 最可缓 |
| `progressType` | string | 进度类型：sequential（顺序执行）或 parallel（独立推进） |
| `status` | string | 状态：pending / in_progress / completed / cancelled |
| `tags` | string[] | 自定义标签 |
| `subtasks` | object[] | 子步骤列表，含 index / title / status / note |
| `followups` | object[] | 后续事项列表，含 id / title / status（open/linked/completed/closed）/ note |
| `linkedInfo` | object | 关联信息（此任务是某后续事项的跟踪任务时）：parentTaskId + followupId |
| `_version` | int | 版本号，用于并发冲突检测 |

## 📖 task.md 文档说明

每个任务文件夹下有一个 `task.md` 文件，包含该任务的详细文档。

**你可以：**
- ✅ 自由阅读 `task.md` 了解任务详情
- ✅ 用你平台的读写工具编辑 `task.md` 中除进度表格和后续事项表格外的任何内容

**请注意：**
- `## 进度` 章节由 **task-keeper** 通过 `task_progress` 工具自动管理
- `## 后续事项` 章节由 task-keeper 自动管理，状态随关联任务自动同步
- 手动编辑这两个表格会被工具覆盖
- task.md 路径会随任务状态变化（文件夹名中的状态标记会更新），每次通过 `task_query` 获取最新路径

## 🔗 关联任务机制

当任务中发现不影响当前进度的优化点时，可记录为后续事项（followups）。如果需要专门跟踪，可以创建关联任务：

1. 在主任务中记录后续事项（`followups`）
2. 创建子任务时指定 `linkedParent` + `linkedFollowupId`
3. 子任务完成时，主任务的后续事项自动变为 `completed`
4. 文件夹名的 `+N` 后缀自动更新

## 推荐操作方式

1. 调用 `task_query` 获取元数据和 `taskMdPath`
2. 用你平台的读写工具按需读取或编辑 `task.md`
3. 用 `task_progress` 推进进度（自动更新进度表格和文件夹名）
