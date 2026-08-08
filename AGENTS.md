# 仓库工作约定

## 代码审查流程

- 用户要求“检查/审查未提交的更改”或要求审查某分支/某次改动时，**默认使用
  `reasonix review` 执行独立 AI 审查**：未提交改动直接 `reasonix review`；
  分支间改动用 `reasonix review --base <分支>`；需要指定模型时加 `--model`。
- **聚焦审查**：`reasonix review` 默认覆盖全部工作区 diff；需要限定重点时用
  `-instructions` 追加指令（如 `reasonix review -instructions "只审查
  winia/src/render.rs 与 winia/src/ui/icon.rs 的 SVG 渲染与缓存改动，忽略
  文档/示例/码点表"`）。`-base`/`-commit` 可把范围限定到指定分支或单个提交。
- ⚠ **Codex 子代理当前不可用**：DeepSeek（非 OpenAI provider）+ MultiAgentV2 下
  `spawn_agent`/`followup_task` 的任务负载经 `encrypted_content` 投递会被
  provider 丢弃（openai/codex#36586），子代理只会回复“未收到任务”。
  上游修复或本地补丁生效前，**不要派生子代理审查**；
  [docs/review-agent-template.md](docs/review-agent-template.md) 模板保留，
  待子代理恢复后按原流程使用。
- 审查（reasonix 或未来的子代理）**严格只读**：不修改代码、不提交；编译验证由
  主 agent 另行执行。reasonix 审查若 shell 受限（无法跑 `git diff`/`cargo check`）
  或 diff 截断，主 agent 需人工核对它未覆盖的文件并补齐编译验证。
- 主 agent 收到审查报告后：在最终回复中给出摘要，并说明每条问题的采纳/不采纳理由；
  需要修复的问题修复后重新跑全量测试。
- 涉及大改动（跨模块、新子系统）时，即使没有明确要求，也建议先用 `reasonix review`
  过一遍再合并。

## 其他约定

- 新功能工作从 `text-field`（或当前主线）开分支，分支名按需求；本仓库历史分支
  使用描述性短名称（如 `interaction-source`、`text-field`、`component-polish`），
  **不要使用 `codex/` 前缀**。
- 分支清单/差距分析见 `docs/component-gap-analysis.md`（component-polish 分支）
  与 `docs/handover.md`（真实架构与已知问题）。
