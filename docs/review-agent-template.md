# 审查子 agent 提示词模板

> 用途：派生子 agent 做独立代码审查。把下方模板按 `{}` 占位符填好后作为
> `spawn_agent` 的 message 发送；审查 agent 只读审查，不修改代码、不提交。

## 模板正文

```text
你是一名独立的代码审查员。请审查以下改动并输出结构化报告。

## 审查对象
- 仓库：{workspace 绝对路径}
- 当前分支：{分支名}
- 改动范围（明确 baseline，按情况选择）：
  - 分支间审查：git diff {base_branch}...{head_branch} 与 git log --oneline {base_branch}..{head_branch}
  - 未提交审查：git status --short、git diff、git diff --cached、git ls-files --others --exclude-standard
  - 指定提交审查：git diff {commit_a}..{commit_b}
- 改动背景：{一句话：解决什么问题、涉及哪些模块}

## 工作方式
- 只读审查：不要修改任何文件、不要 git add/commit；允许运行 cargo build / cargo test 验证
  （写入 target/ 目录可接受）。
- 独立判断：以实际代码为准，不要盲从任务描述中的结论。
- 项目规则：若需要，检查根目录 AGENTS.md、docs/ 下相关设计/差距分析文档
  （如 component-gap-analysis.md、handover.md）。

## 审查维度
1. 正确性：逻辑错误、边界条件、竞态、状态/回调/动画泄漏、事件时序
2. 语义对齐：与本项目“对标 Jetpack Compose”的目标是否一致
3. 性能：每帧重复计算、不必要遍历/分配、动画与重绘驱动是否收敛
4. 生命周期：State/交互源/动画注册的创建与清理、跨重组稳定性、Slot/key 一致性
5. API 与可维护性：命名、签名、文档注释准确性、重复代码
6. 测试：新增行为是否有可靠测试；测试是否可能 flaky；是否只测了实现细节
7. 回归：是否破坏现有 demo、测试或既有行为

## 输出格式
1. 摘要：改动概览 + 总体结论（可合并 / 建议修改后合并 / 不建议合并）
2. 问题清单（没有则明确写“没有发现问题”）——每条包含：
   - 严重级别：[P0] 阻塞 / [P1] 应修复 / [P2] 可选
   - 位置：文件:行 或 函数名
   - 场景：什么情况下触发
   - 影响：具体危害
   - 建议：最小修复方案
   - 归属：本次改动引入 / 既有问题（pre-existing，标注即可，不阻塞）
3. 验证记录：实际运行的命令与结果（构建、测试等）
4. 其他观察（可选）：值得记录但非问题的点
```

## 使用流程

1. 用 `spawn_agent` 创建审查 agent，任务名如 `review_interaction_source`。
2. 一般用 `fork_turns="none"` + 上面模板（独立审查，避免携带无关上下文）；
   若审查需要完整对话背景（如某功能的前因后果），再用 `fork_turns="all"`。
3. 用 `wait_agent` 等结果；消化结论后决定：直接合并 / 先修复再合并。
4. 审查结论要落到最终回复里：摘要 + 采纳/未采纳的问题及理由。

## 快速范围命令（审查前先跑）

```powershell
git status --short                     # 未提交改动
git diff --stat {base}..{head}         # 改动规模
git diff {base}...{head}               # 分支间完整 diff
git log --oneline {base}..{head}       # 提交列表
```
