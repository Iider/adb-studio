# 工作流与接口

任务只通过工作流编辑器创建。旧内置点击、滑动、汽水及旧纯视觉任务已移除，不再转换或恢复。工作流内的点击、滑动、按键、等待、条件、循环与定时继续使用。

## 创建和保存

网页“自动化任务”右上角提供“创建新工作流”和“导入工作流文件”。草稿自动保存未完成的编辑；手动“保存”检查连线、标注与参数，生成可运行任务。保存覆盖当前任务，已启动的运行保留启动时的内容。

命令工具通过 `data/service-root` 查找本地服务与访问凭据，不要输出令牌。接口路径在命令中省略 `/api`。

```sh
python scripts/studio.py devices
python scripts/studio.py workflows
python scripts/studio.py request GET /workflow-drafts
```

| 接口 | 用途 |
| --- | --- |
| `GET /api/workflow-drafts` | 列出草稿 |
| `GET /api/workflow-drafts/{UUID}` | 读取草稿 |
| `PUT /api/workflow-drafts/{UUID}` | 保存草稿；新建 revision=0，修改带读取到的 revision |
| `POST /api/workflow-drafts/{UUID}/save` | 请求 `{"revision":当前编号}`，检查并覆盖可运行任务 |
| `DELETE /api/workflow-drafts/{UUID}?revision=N` | 删除未关联已保存任务的草稿；关联任务须从任务列表整体删除 |
| `GET /api/workflows` | 列出已保存任务 |
| `DELETE /api/workflows/{id}` | 删除任务、草稿、定时、全部历史记录与截图 |
| `GET /api/editor/observe/{device_id}` | 取得编辑器使用的当前画面与屏幕信息 |

并发修改草稿返回 409，不自动覆盖另一页面的编辑。`POST /api/workflows` 已关闭；不能直接提交旧任务本体。正在运行、等待、恢复或暂停的任务须先停止再删除。

## 草稿与工作流格式

草稿字段为 `id`（UUID）、`revision`、`name`、`description`、`entry`、`nodes`、`viewport`。每个节点含 `id`、`kind`、`label`、`config`、`targets`、`position`。`viewport` 为 `[横向位置,纵向位置,缩放]`；节点位置为 `[x,y]`。

可用节点：`tap`、`swipe`、`back`、`home`、`recents`、`wait`、`condition`、`wait_condition`、`loop`、`break`、`continue`、`end`。各项设置、手机标注和连线见[编辑器说明](visual-workflow-editor.md)。条件的页面依据可留空；读字失败与陌生页面不会自动成为“不包含”。

`tap`、`swipe` 的 `config.after_wait` 为全部动作完成后的一次等待，单位秒，范围 0 至 86400，支持小数；省略时为 0。`config.interval` 仍只用于两次动作之间。

### 运行变量

在“开始”节点添加变量，填写名称、类型和默认值，可选填“显示别名”，例如变量 `count` 的别名为“点击次数”。运行参数、画布摘要和参数报错优先显示别名；别名为空时显示名称。单位单独设置，例如“点击间隔”配合秒单位显示“点击间隔（秒）”。其他节点的数值字段可切换为“使用变量”；编辑器运行面板、设备页“直接运行”和任务详情的输入框直接显示默认值，均可临时修改，留空使用默认值，`0` 是有效输入。临时值不修改工作流，运行结束后下次仍从默认值开始。

支持整数、可带小数的数字；最多 20 个变量。可选单位（秒／毫秒）、范围和说明。轮数／次数引用整数变量；等待时长、间隔、结束后等待和滑动时长支持数字变量。字段保留原有范围限制，所有引用都需满足；明确填写单位时须与字段相符，无单位数字按各引用字段的单位使用。滑动毫秒的小数沿用现有取整处理。

草稿与本体增加可选 `inputs` 数组，空数组可省略。定义示例：

```json
{"id":"00000000-0000-4000-8000-000000000001","name":"rounds","alias":"循环执行轮次","type":"integer","default":5,"min":1,"max":10000}
```

`type` 为 `integer`／`number`；可选 `alias` 为显示别名，最多 40 字，不要求唯一；可选 `unit` 为 `seconds`／`milliseconds`，省略或空字符串表示无单位；可选 `description` 最多 200 字。节点引用示例：`config.count = {"input":"00000000-0000-4000-8000-000000000001"}`。引用编号由系统生成，修改名称或别名不改变引用；已有变量不需补填别名。导出、导入与运行快照保留别名；已引用变量需先解除引用才能删除。

含变量的工作流使用 `schema_version: 2`，无变量保持 1。草稿允许默认值暂未填完；保存为可运行内容时检查默认值及引用。运行记录的 `workflow` 保存实际数值副本，`inputs` 保存本次生效值，`input_overrides` 保存临时填写；暂停、断线及后台重启后均继续使用该记录。详细设计及验收范围见[运行变量方案](workflow-inputs-plan.md)。

保存后编号为 `manual-<草稿UUID>`。可运行内容只有 `canvas` 和 `end` 两类内部动作，不能混入旧动作。画布草稿和已保存内容分别保留；不能用未保存草稿替换正在运行或导出的任务本体。

文字条件在启动时有模型配置及可读取密钥则优先使用模型；未配置时使用本地读字。条件判断与等待条件共用规则，模型失败不切回本地。模型优先已通过用户验收并部署，直接填写 Key 仍待实施，见[整改方案](recognition-fallback-plan.md)。无需文字判断的点击、滑动和等待不依赖本地读字或模型。

## 运行与定时

`POST /api/devices/restart-adb` 手动重启电脑上的 ADB 服务并尝试重连已登记手机，不接收参数，返回刷新后的手机列表。有任何未结束任务（含等待手机、恢复和暂停）或正在处理的任务操作时拒绝重启；普通断线不自动重启 ADB。接口成功表示服务已重启，是否连上须检查各手机的 `online`，不能将仍离线视为恢复成功。

```sh
python scripts/studio.py run manual-草稿UUID --device 手机自身编号
python scripts/studio.py runs
python scripts/studio.py pause 运行编号
python scripts/studio.py resume 运行编号
python scripts/studio.py stop 运行编号
```

`POST /api/runs` 接收 `device_id`、`workflow_id` 和可选的 `inputs`（变量编号到数字的映射）。省略 `inputs`、传 `{}` 或省略某个编号时使用默认值；不接受 `null`、字符串或布尔值作为变量值。未知变量或超出范围时不创建运行。

```sh
python scripts/studio.py run manual-草稿UUID --device 手机自身编号 --inputs-file 本次参数.json
```

参数文件示例：`{"00000000-0000-4000-8000-000000000001":10}`。原有不带参数文件的命令继续使用默认值；位置仍在工作流编辑器按手机标注。

`GET /api/runs/{id}/events` 读取事件；`GET /api/runs/{id}/export` 导出执行证据，不能作为工作流导入。`DELETE /api/runs/{id}` 清理已结束记录，运行截图仍留在本机归档；删除整个任务才同时清理其全部截图。

`GET/POST /api/schedules` 查询或新增定时；`PUT/DELETE /api/schedules/{id}` 修改或删除。新增与修改字段为 `name`、`device_id`、`workflow_id`、`enabled`、`rule`，执行保存的工作流内容及变量默认值，不沿用手动运行时的临时值。规则支持 `once`（at）、`daily`（time）、`weekly`（time、days）、`interval`（minutes），界面时间统一使用上海时区。

同机一次一个任务；暂停仍占用手机，定时遇到忙碌或离线会跳过。运行途中掉线等待；重启保留人工暂停。继续前核对手机、锁屏和原应用；已发出但结果不明的动作拒绝重放。

## 工作流文件导出

任务卡片“更多 → 导出工作流”导出本体和关联草稿。编辑器“更多 → 导出当前草稿”只导出草稿；未完成的编辑也可携带。

- `GET /api/workflows/{id}/export`
- `GET /api/workflow-drafts/{UUID}/export?revision=N`

文件格式为 `adb-studio.workflow`。无变量时 `format_revision: 1`；本体或草稿含变量定义／引用时为 2，新版可导入两版。`content.workflow` 为可运行内容或 null，`content.draft` 必须包含画布草稿；`assets` 携带引用的 PNG。草稿仅携带编辑字段，不包含服务端 revision／updated_at。本体与草稿有差异时分别保留，并在下载前提示。

响应 `X-Workflow-Draft-State` 为 saved／modified／incomplete／draft。缺少任务或草稿返回 404，草稿编号冲突返回 409，非法图片与引用返回 400，超限返回 413。

## 导入工作流副本

任务页右上角选择文件，预览后点“导入副本”。只接受编辑器导出的工作流文件或草稿文件；拒绝裸本体、旧任务动作、无草稿文件。不支持自动转换。

`POST /api/workflow-import/preview` 与 `POST /api/workflow-import` 请求：

```json
{"operation_id":"本次操作UUID","file":{},"name":"可选的新名称"}
```

预览不写入。导入生成新草稿 UUID，任务编号为 `manual-<新UUID>`，保留画布、标注、内部连线及草稿与本体的差异；不运行、不创建定时。仅草稿文件需手动保存后才能运行。重复导入可创建独立副本；同一次请求重试使用相同 operation_id，避免重复写入。同编号不同内容返回 409；原副本已删除则返回 removed=true，不重建。

请求最多 32 MiB；每份本体或草稿最多 `2 MiB - 1024 字节`；最多 40 张 PNG，原始图片合计最多 20 MiB，单张最多 8 MiB、宽高各最多 8192。格式、坏图、缺图、重复附件、路径和旧动作返回 400。写入失败回退本次内容，重启按清单清理无引用图片。文件保留变量定义、默认值与节点引用，不携带本次运行的临时值、模型密钥、令牌、运行记录或定时。

## 可选样本检查

`POST /api/workflows/{id}/validate` 检查编辑器条件样本，返回诊断结果；不阻止无样本任务保存或运行。当前不执行旧纯视觉任务的图片分支检查。独立模型看图检查见[识别说明](vision-recognition.md)。

## 旧数据清理

首次启动读取旧任务原始记录，在转成可运行内容前删除旧本体、关联定时、执行记录和事件，按清单清理运行截图。清理前在同一 data 目录生成 `before-editor-only-<UUID>.sqlite` 备份；备份失败则中止清理。工作流编辑器创建的任务、草稿、手机与模型设置保留。新启动不再预置任何任务。
