# 仓库工作约定

## 代码审查流程

- 用户要求“检查/审查未提交的更改”或要求审查某分支/某次改动时，**默认派生子 agent
  执行独立审查**，提示词使用 [docs/review-agent-template.md](docs/review-agent-template.md)
  中的模板（填入审查对象、范围、背景后发送给 `spawn_agent`）。
- 审查 agent **严格只读**：不运行构建/测试、不修改代码、不提交——只通过阅读
  代码/文档/git diff 审查；编译验证由主 agent 另行执行。
- 主 agent 收到审查报告后：在最终回复中给出摘要，并说明每条问题的采纳/不采纳理由；
  需要修复的问题修复后重新跑全量测试。
- 涉及大改动（跨模块、新子系统）时，即使没有明确要求，也建议先用审查子 agent
  过一遍再合并。

## 其他约定

- 新功能工作从 `text-field`（或当前主线）开分支，分支名按需求；本仓库历史分支
  不使用 codex/ 前缀时遵循现有命名习惯。
- 分支清单/差距分析见 `docs/component-gap-analysis.md`（component-polish 分支）
  与 `docs/handover.md`（真实架构与已知问题）。
