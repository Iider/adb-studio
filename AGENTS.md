# ADB 工作台

遵守上一层项目约定。用户确认各功能单位后再做本地 Git 提交。

- 后端：Rust / Axum / Tokio / SeaORM / SQLite；前端：SvelteKit 静态网页。
- 修改工作台前端时，使用项目级 [前端开发规范](.agents/skills/adb-studio-frontend/SKILL.md)。
- 使用 `scripts/studio.py` 管理工作流；格式见 `docs/workflows.md`。工作流按编号覆盖保存，每项只保留一份当前内容；保存后直接运行，样本检查为可选诊断。
- 所有手机命令经 `src/adb.rs` 核对自身编号。不要绕过同机任务互斥或恢复检查。
- `data/` 保存运行状态、令牌、截图及日志，不提交；不得记录模型密钥。
- 修改执行行为时运行 `cargo test` 与模拟设备集成检查。网页修改运行 `npm run check` 和构建。
- 以真实失败证据调整规则；保持用户说明、接口文档与实际行为一致。
- Windows 安装与自启见 `docs/deployment.md`；Mac 使用 launchd，Linux 使用 systemd，平台分支不得混用。Windows 模拟检查通过 `scripts/check_support.py` 调用 Python 模拟设备，不操作真机。

## 当前状态与下一步

Windows 本机部署、模拟检查及用户真机 Wi-Fi 连接、截图、投屏验收通过。安装、启动、停止、自启和 ZIP 使用见 `docs/deployment.md`。待完成：Mac 完整实机回归与完整 Linux 后台回归。

工作流副本导入已通过用户验收并部署，新建／导入按钮放在任务页右上角；覆盖导入待开发。当前接口与方案见 `docs/workflow-import-export.md`。
