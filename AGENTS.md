# ADB 工作台

遵守上一层项目约定。用户确认各功能单位后再做本地 Git 提交。

- 后端：Rust / Axum / Tokio / SeaORM / SQLite；前端：SvelteKit 静态网页。
- 修改工作台前端时，使用项目级 [前端开发规范](.agents/skills/adb-studio-frontend/SKILL.md)。
- 任务仅通过工作流编辑器创建或导入，不恢复旧内置任务、旧动作或裸本体入口。使用 `scripts/studio.py` 管理工作流；接口见 `docs/workflows.md`。每项只保留一份当前内容，保存后直接运行，样本检查为可选诊断。
- 当前应用只在条件判断／等待条件明确配置应用条件时读取；不在点击、滑动或暂停恢复中增加隐式应用检查。
- 所有手机命令经 `src/adb.rs` 核对自身编号。不要绕过同机任务互斥或恢复检查。
- `data/` 保存运行状态、令牌、截图及日志，不提交；不得记录模型密钥。
- 清理旧任务前备份数据库，保留编辑器任务、草稿、手机和设置。
- 修改执行行为时运行 `cargo test` 与模拟设备集成检查。网页修改运行 `npm run check` 和构建。
- 以真实失败证据调整规则；保持用户说明、接口文档与实际行为一致。
- Windows 安装与自启见 `docs/deployment.md`；Mac 使用 launchd，Linux 使用 systemd，平台分支不得混用。Windows 模拟检查通过 `scripts/check_support.py` 调用 Python 模拟设备，不操作真机。

## 文档入口

- 功能、部署范围与待办：[当前状态](docs/project-status.md)。
- 变更与验收证据：[变更记录](CHANGELOG.md)。
- 标注、单步与孤立节点：[节点编辑](docs/workflow-node-editing-plan.md)。
- 网页交互与通用组件：[组件说明](docs/ui-components.md)。
- 模型优先、故障与恢复：[识别规则](docs/recognition-fallback-plan.md)。
