# Agent 工作流接口

从项目目录运行 `python3 scripts/studio.py`。工具自动读取本机令牌；不要把令牌写进工作流、文档或模型提示。

工作流文件导出已验收并部署；导入仍待开发。范围与验收安排见[导入与导出开发方案](workflow-import-export.md)。

## 常用命令

```sh
python3 scripts/studio.py devices
python3 scripts/studio.py request GET /devices/手机自身编号/screenshot --output /绝对路径/当前画面.png
python3 scripts/studio.py workflows
python3 scripts/studio.py runs
python3 scripts/studio.py request POST /workflows --file /绝对路径/工作流.json
python3 scripts/studio.py run 编号 --device 手机自身编号
python3 scripts/studio.py request GET /runs/运行编号/export --output /绝对路径/运行记录.json
python3 scripts/studio.py stop 运行编号
```

手机编号取 `devices` 返回的 `id`，不是无线地址。工作台会在实际操作前核对设备自身编号。

## 工作流格式

```json
{
  "schema_version": 1,
  "id": "short-swipe",
  "name": "滑动两次",
  "description": "在当前页面先等待，再上滑。",
  "devices": [],
  "entry": "swipe",
  "model_limit": 10,
  "nodes": [
    {"id": "swipe", "label": "每隔 25 秒上滑，共两次", "type": "swipe_loop", "interval": 25, "count": 2, "minutes": null, "long": false, "next": "end"},
    {"id": "end", "label": "完成", "type": "end", "result": "succeeded"}
  ],
  "tests": [{"node": "swipe", "text": "interval=25,count=2", "expected": "valid"}]
}
```

`devices` 为空表示通用工作流；专用界面应填写已适配手机编号。同一个 `id` 只保存一份工作流，`POST /workflows` 直接覆盖当前内容，保存后即可运行。不接受历史版本选择，不生成快照；旧请求里的 `version` 字段会被忽略。

已有数据升级时，每个工作流仅保留原来版本号最高的内容，并以工作流编号作为存储键；重复启动不会重新产生旧版本。草稿、定时和执行记录保留。正在运行的任务继续按启动时的步骤执行，下次运行使用覆盖保存后的内容。

步骤说明与编号会直接显示在网页，使用用户能读懂的中文说明。

可选 `launch_mode` 指定运行方式：`standard` 按步骤执行，`device_point` 使用目标手机的本机点击标注，`swipe_settings` 提供滑动时长与间隔设置。后两项分别要求本地固定点击或滑动循环步骤，不支持画布任务。旧数据未填写时，仅按原 `point-click`／`swipe` 编号兼容识别；明确填写 `standard` 会覆盖编号默认行为。导出文件始终写明运行方式，更换编号也保留原意。

| 步骤类型 | 参数 | 行为 |
| --- | --- | --- |
| `wait` | `seconds` | 等待，最大 86400 秒 |
| `launch` | `package`，可选 `restart` | 打开应用，检查前台应用；`restart` 默认 `false`，为 `true` 时先强制结束应用再重新打开，具体页面由应用决定 |
| `tap_text` | `text`, `verify` | 控件文字优先，文字识别补充；匹配唯一位置；检查结果文字 |
| `tap_point` | `x`, `y`, `guard`, `verify` | 相对坐标；点按前检查页面文字，之后检查结果文字 |
| `swipe` | `from`, `to`, `duration_ms` | 单次相对坐标滑动；异常恢复时不重放未确认动作 |
| `key` | `key`, `verify` | BACK、HOME、ENTER；检查结果文字 |
| `branch` | `cases`, `otherwise`, 可选 `guard` | `cases` 为 `contains` 与 `next` 数组；页面依据 `guard` 匹配时，未命中条件直接走 `otherwise`，无需模型；未知画面再调用模型 |
| `swipe_loop` | `interval`, `count`, `minutes`, `long` | 当前页面定时上滑；次数或时长至少设置一个 |
| `qishui_navigate` | 无 | 已采样的汽水音乐金币入口、遮挡关闭和任务入口识别 |
| `qishui_loop` | `coin_region`, `identity_region`, `warmup`, `stall`, `finish_stall`, `finish_swipes` | 区域均为 `[左,上,右,下]` 相对坐标；多帧金币识别和视频切换复查 |
| `end` | `result` | `succeeded` 或 `needs_help` |

除 `branch`、`vision_route`、`canvas` 与 `end`，每个步骤需要 `next`。分支和 `next` 必须引用存在的步骤；入口需要能够到达结束步骤。`canvas` 的出口与循环范围由编排器另行校验。

模型只选择声明过的分支或导航目标。API 不接收任意脚本、电脑 Shell 或任意 ADB 命令。

## 工作流文件导出

任务卡片“更多 → 导出工作流”下载 `.workflow.json`：包含当前本体、存在的画布草稿和引用的 PNG 样本。草稿与本体不同时，下载前提示分别保留，不自动覆盖本体。编排器“更多 → 导出当前草稿”先保存当前编辑，再仅导出草稿；缺少连线或尚未完成参数也可导出。

```sh
python3 scripts/studio.py request GET /workflows/任务编号/export --output /绝对路径/任务.workflow.json
python3 scripts/studio.py request GET '/workflow-drafts/草稿UUID/export?revision=当前校对编号' --output /绝对路径/草稿.workflow.json
```

接口返回文件 JSON，类型为 `adb-studio.workflow`，文件格式编号 `format_revision: 1`，其中 `content.workflow` 与 `content.draft` 分别存本体与可移植草稿，`assets` 存去重后的 PNG Base64。草稿不含服务校对编号、保存时间；文件不含定时、执行记录或服务配置。定点点击的本机手机标注不随模板打包，换电脑后需重新标注；画布内的设备坐标与文字区域原样保留。

两个 GET 导出接口不写入内容；编排器点击导出前的自动保存仍走现有草稿接口。服务用 `X-Workflow-Draft-State` 返回 `none`、`saved`、`modified`、`incomplete` 或 `draft`，用 `X-Workflow-Launch-Mode` 返回实际运行方式，供网页显示下载说明。任务／草稿不存在返回 404；草稿校对编号已变化返回 409；缺图、无效图片或不合法样本名称返回 400；超限返回 413。

每份本体或可移植草稿不超过 `2 MiB - 1024 字节`，为后续正常编辑请求预留余量。文件总大小最多 32 MiB，最多 40 张图片，原始 PNG 合计最多 20 MiB。单张沿用现有视觉检查规则：PNG 最多 8 MiB，宽高各最多 8192，解码内存上限 128 MiB。缺图时显示具体样本，不静默丢失图片。文件分享前请确认步骤文字、手机编号和样本内容。

## 运行与可选样本检查

`tests` 是文本样本：`node` 指定判断步骤，`text` 是识别结果，`expected` 是预期分支。仅在手动执行样本检查时，分支步骤须提供样本；`guard` 未匹配的样本预期为 `unknown`，已知页面的否定分支需提供 `guard`；汽水任务须覆盖视频与网络阻塞样本。文字样本检查与运行中的图像判断用于不同环节。`tests` 可省略或为空，不影响保存与运行。

运行不触发样本检查；各台适用手机均可首次直接运行，更换模型后也不需要重新试跑。样本检查可单独调用，只返回诊断结果，不产生启用标记。

```sh
python3 scripts/studio.py request POST /workflows/编号/validate
```

恢复任务时，重新识别当前页面。一般点击、按键只有在验证结果通过后才向后执行，无法确认则标记需要处理；无法验证的固定点击和单次滑动不重放。定时滑动不补发待确认动作，并检查原来的应用是否仍在前台；汽水任务重新观察金币和视频状态。

## HTTP 接口

编排器的草稿独立于可运行内容保存，允许缺少连线或尚未填完参数，不会进入 `/workflows`、定时或执行队列。接口 `GET /api/workflow-drafts` 列出草稿，`GET /api/workflow-drafts/{id}` 读取单份，`PUT /api/workflow-drafts/{id}` 保存，`DELETE /api/workflow-drafts/{id}?revision={revision}` 删除。

草稿字段为 `id`（UUID）、`revision`、`name`、`description`、`entry`、`nodes`、`viewport` 和服务生成的 `updated_at`。每个节点保存 `id`、`kind`、`label`、`config`、`targets` 和 `position`；`targets` 是出口编号到目标节点编号的映射，`position` 为画布位置 `[x,y]`，`viewport` 为 `[x,y,zoom]`。新草稿以 `revision: 0` 提交；保存响应返回递增后的并发校对编号。更新与删除必须带上最后读取的 `revision`，冲突返回 HTTP 409，客户端应保留当前编辑并提示重新读取或保存副本。草稿的完整执行校验仍需在手动保存工作流时完成。

`POST /api/workflow-drafts/{id}/save` 接收 `{"revision":当前草稿校对编号}`，校验连线、循环范围与坐标标注后覆盖对应的工作流本体；不累积历史记录。`GET /api/editor/observe/{device_id}` 返回原图数据与屏幕宽高、显示屏标识，用于标注，不发送触控命令。新编排步骤序列化为 `{"type":"canvas","step":{...}}`，其出口由 `step.targets` 决定，不能同时声明 `next`。坐标标注绑定手机与屏幕形态；无法确认多屏手机的触控显示编号时拒绝标注。

编排器文字条件中的 `guard`（页面依据）可省略，默认留空。未填或仅有空格时直接判断文字条件；填写后先匹配整页文字，再判断指定区域，依据不匹配时请求处理。条件判断与等待条件使用同一规则。

自动化任务卡片直接提供“编辑”：有草稿的任务打开画布，其余任务直接编辑当前内容。`GET /api/workflows` 的每项任务带有 `archived` 布尔状态。所有已保存任务可通过 `PUT /api/workflows/{id}/archive` 归档、`DELETE /api/workflows/{id}/archive` 恢复；归档保留草稿、任务内容与执行记录，停用相关定时。归档中拒绝启动、继续和新建定时；恢复后定时仍需手动开启。详情见 [编辑与归档方案](workflow-edit-archive.md)。

所有接口前缀 `/api`；Agent 使用 `Authorization: Bearer <令牌>`。响应错误提供 `error` 字段。

| 方法与路径 | 用途 |
| --- | --- |
| GET `/devices` | 设备、连接方式和常亮状态 |
| POST `/devices/refresh` | 重新发现设备 |
| GET `/devices/discover` | 列出局域网中广播的无线调试连接、配对地址；仅返回局域网 IP，不自动连接 |
| POST `/devices/pair` | 用 `{"address":"手机IP:配对端口","code":"6位配对码"}` 配对无线调试；配对码不保存 |
| POST `/devices/connect` | 用 `{"address":"手机IP:连接端口"}` 连接局域网手机；核验自身编号后登记，保存地址供断线重连 |
| PUT `/devices/{id}` | `name` 或 `keep_awake` |
| GET `/devices/{id}/screenshot` | 当前截图 PNG |
| GET / POST `/workflows` | 查询任务 / 覆盖保存 |
| PUT / DELETE `/workflows/{id}/archive` | 归档 / 恢复任务 |
| DELETE `/workflows/{id}` | 彻底移除任务本体、关联草稿与定时；未结束时拒绝。保留历史执行记录，禁止续跑已移除任务；内置任务重启后不再生成 |
| POST `/workflows/{id}/validate` | 可选样本诊断，不影响运行资格 |
| GET / POST `/runs` | 查询 / 运行，参数见下方 |
| DELETE `/runs/{id}` | 清理已结束的记录，归档与截图保留；进行中的任务拒绝清理 |
| POST `/runs/{id}/stop` | 停止，包括等待中与已暂停的任务 |
| POST `/runs/{id}/pause` | 请求人工接管，等待 `state=paused` 后可独占手机操作 |
| POST `/runs/{id}/resume` | 检查同机任务、连接、解锁、原应用与待确认动作后，继续已暂停或已停止的原记录 |
| GET `/runs/{id}/events` | 判断与行动记录 |
| GET `/runs/{id}/export` | 本次运行的步骤与参数、结果、记录和截图路径 |
| GET / POST `/schedules` | 查询 / 创建定时 |
| PUT / DELETE `/schedules/{id}` | 更新 / 删除定时 |
| GET `/settings` | 后台与模型设置 |
| PUT `/settings/model` | `endpoint`, `model`, `api`, `key_env` |
| GET `/shots/{run}/{file}` | 关键截图 PNG |
| GET `/ws` | 设备与运行变更事件；事件到达后重新查询 |

运行参数：`device_id`, `workflow_id`。运行当前保存内容；不再提供版本参数或 `publish` 接口。已启动任务的步骤与进度不随后续保存改变。

定时参数：`name`, `device_id`, `workflow_id`, `enabled`, `rule`。

### 人工接管与继续

使用 `python3 scripts/studio.py pause 运行编号` 请求接管；查询 `/runs`，等待状态变为 `paused`。`pause_requested=true` 只表示请求已经发出，在途命令尚未结束时不能接管。操作完成并回到目标页面后，使用 `python3 scripts/studio.py resume 运行编号` 继续；仍可用 `stop` 停止。

投屏操作可与任务并行，不自动暂停或继续任务；需要暂停任务、独占手机时才请求人工接管。手动改变应用或页面后，任务原有的检查仍可能暂停或停止任务。

- `paused` 是未结束状态，保留步骤、参数与进度；继续占用手机，拒绝新任务和记录清理，定时触发会跳过。后台重启保持暂停。暂停后可通过设备页唤醒或锁屏。
- `cancelled` 表示用户已停止，释放手机且不会自动恢复；可显式调用 `resume` 保留原编号、开始时间、本次运行的步骤与参数、模型调用次数与进度继续，恢复时清空 `ended_at`。同机另有未结束任务时拒绝继续。已完成和需要处理的记录不接受继续。
- 暂停、停止时记录原应用。旧停止记录可使用 `checkpoint.foreground` 中的原应用信息；缺少原应用依据时拒绝继续。停止后继续仍检查解锁、原应用、尺寸与未确认动作，不重新创建任务或改用新步骤。
- ADB 超时、`INJECT_EVENTS` 触控拒绝会转为暂停，不自动重试。`reason` 为简短说明，`error_detail` 保留原始错误；旧记录的长报错由网页折叠展示。
- 继续前读取手机当前状态并检查原应用；同一应用内的具体页面仍需用户确认。无法确认原应用时保持暂停。网页定点点击额外核对启动时保存的屏幕尺寸，暂停后重新标注不会覆盖当前任务的步骤与参数。
- 未确认的固定点击、循环点击及单次滑动拒绝继续，维持原状态，需核对页面后重新开始。带结果验证的点击／按键先验证，不直接重发。循环滑动沿用保守恢复：待确认动作计入尝试次数，不补发；`uncertain_swipes` 记录未确认次数。
- 普通等待节点保留已等待秒数；循环滑动恢复时重新等待一个间隔，暂停时长不计入任务时长。汽水任务重新观察，不沿用暂停前的完成判断。
- 工作台不能拦截外部 ADB 或直接触屏。接管、继续都由用户明确发起，不解除系统权限限制。

| rule | 格式 |
| --- | --- |
| 单次 | `{"kind":"once","at":"2026-10-14T10:00:00+08:00"}` |
| 每天 | `{"kind":"daily","time":"09:00"}` |
| 每周 | `{"kind":"weekly","time":"09:00","days":[1,3,5]}`，周一为 1 |
| 间隔 | `{"kind":"interval","minutes":60}` |

定时绑定工作流编号，下次执行读取当前保存内容；已启动任务按原步骤继续执行。离线、忙碌、后台停机错过的定时点都跳过，不积压。间隔规则以保存时间为起点。

网页定时页支持搜索与开启状态筛选，卡片显示下次执行和最近触发结果；“新建定时”／“修改”打开表单，删除需确认。单次已到期时需修改时间再开启。创建、修改和启停均沿用现有接口：更新后重新计算下次时间并清空 `last_result`，固定间隔重新计时。删除规则不会停止已开始的任务。

## 滑动运行设置

网页「自动化任务 → 定时滑动 → 运行」可输入总时长（分钟）和滑动间隔（秒），默认 15 分钟、25 秒。先等待再滑动，到达时长结束；时长小于间隔时只等待，不滑动。网页记住上次运行设置。

「定时任务」中的同名输入随每条规则保存，修改时自动带回。执行中的任务使用启动时的设置，修改不会影响已开始的任务。

`POST /api/runs` 和定时创建／修改接口接受 `"swipe": {"minutes":15,"interval":25}`。总时长大于 0、最多 10080 分钟，间隔大于 0、最多 86400 秒；支持小数。该参数覆盖本次运行的次数限制，不改变工作流本体。省略时沿用原定义。

手机截图兼容多显示屏设备：ADB 输出中的显示屏提示会被移除，图片校验通过后才交给网页和识别流程。截图损坏时返回明确错误，可重新截图。

折叠屏截图每次重新检查内外屏状态，指定当前亮着的物理屏幕；无法确定目标时提示重新点亮屏幕，避免误截黑着的内屏。

## 截图分段计时

截图接口仍返回 PNG。USB 优先取带 sRGB 色彩空间信息的 RGBA 或 RGBX 原始画面，由电脑无损生成 PNG；格式不支持或数据损坏时回退手机 PNG。无线连接直接使用手机 PNG。识别流程复用已解析图片，每次请求重新取图。

响应增加 `Cache-Control: no-store` 与 `Server-Timing`。计时单位为毫秒：`device` 为设备核验，`display` 为亮屏检查，`capture` 为手机取图与传输，`decode` 为画面解析，`encode` 为电脑生成 PNG，`total` 为服务端总耗时。`source` 的 `raw` 表示电脑生成 PNG，`png` 表示手机生成 PNG；发生回退时，计时包含失败尝试。

```sh
python3 scripts/studio.py request GET /devices/手机自身编号/screenshot --output /绝对路径/当前画面.png --timings
```

同次截图使用已核验的 ADB 连接编号，断线后报错；旧版 ADB 无连接编号时保留逐步核验。该优化不改变运行互斥、动作恢复或截图标注规则。真机结果见 [截图提速与验收](screenshot-performance.md)。

## 招财鸭点击喂鸭

`feed-duck` 第 2 版在折叠屏外屏固定点击 `(528,1866)`。不识别、不截图、不校验页面。请先打开招财鸭并保持外屏，执行期间不要换页面。

设置点击次数、间隔秒数；立即点击第一次，其后按间隔执行。支持 `"tap":{"count":10,"interval":5}`，定时规则保存各自设置。点击次数 1 到 10000，间隔 1 到 86400 秒。第 1 版保留供回退。

断线等待、停止和同机互斥仍有效；未确认成功的点击不会重放。

## 记录清理

网页「执行记录」默认显示最新一条详情。详情中的“清理这条记录”处理单条，顶部“清理已结束”处理点击时已结束的记录；两种操作均需确认。已完成、已停止和需要处理的记录可清理，执行中、等待手机、恢复中和已暂停的任务会保留。

批量清理逐条调用 `DELETE /api/runs/{id}`。清理后从列表移除，原始运行、判断记录和截图保留在本机；该接口不删除定时规则或改变工作流内容。部分记录清理失败时，网页显示成功数量与失败原因，可重新清理剩余记录。

## 截图标注与定点点击

在「设备管理 → 任务详情」选择“定点点击”后点“标注点击位置”，或在「自动化任务 → 定点点击 → 运行」选择手机后标注。弹窗显示当前亮着的屏幕截图，点击目标后出现红点，按“确定位置”保存。标注本身不会操作手机；取消不改变已保存位置。

截图缩放后的点选位置换算为原图像素；各台手机单独保存坐标和原图尺寸，重启保留。定点点击开始时将坐标和尺寸写入运行记录，循环执行时不截图、不识别；暂停后继续时会获取一张截图核对尺寸。换屏、旋转或页面布局变化后应重新标注。

保存后即可点“运行”或设置定时。定时触发使用该手机当时保存的位置。设备修改接口接受 `"tap_point":{"x":528,"y":1866,"width":1060,"height":2376}`，坐标必须在原图范围内。

## 唤醒与锁屏

设备详情顶部提供“唤醒”和“锁屏”。唤醒使用 Android 唤醒按键及 `wm dismiss-keyguard`，仅适用于无密码或系统允许 ADB 解除的锁屏；需要密码时提示用户自行解锁。不在已打开的页面上滑动。锁屏使用睡眠按键，重复点击不会反向唤醒。运行任务期间先人工接管并等待任务暂停，或停止任务，再使用唤醒和锁屏。

`POST /api/devices/{id}/screen` 接受 `{"action":"wake"}` 或 `{"action":"lock"}`，返回操作结果 `message`。

## 纯视觉任务

工作流可声明 `recognition: "vision"`，使用 `vision_check`、`vision_tap` 与 `vision_route`；省略时沿用本地优先。截图样本检查可选；定位、复查和结果检查均计入模型预算，更换模型后可直接运行。不能用纯视觉方式直接运行尚未适配的本地读字或高频步骤。

需要按画面状态选择不同后续步骤时，可用 `vision_route`。每个分流项填写互斥的 `condition` 和目标步骤 `next`；无法确认时任务停止，所有分流状态都要提供截图样本。

字段、样本与只读检查接口见 [纯视觉识别](vision-recognition.md)。模型配置随运行固定，密钥不写入记录；执行导出增加 `vision`，包含本次视觉结果、截图引用与耗时。
