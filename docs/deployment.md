# Mac／Linux／Windows 部署与发行

macOS、Linux、Windows 共用 Rust 后台与静态网页，可运行工作流、截图和投屏。文字条件在配置模型及可读取密钥时优先由模型判断；未配置时使用本地读字，两者都不可用则在开始前提示原因。Mac 可使用苹果文字识别。

## 平台范围

| 系统 | 架构 | 运行方式 |
| --- | --- | --- |
| macOS | Apple Silicon、Intel | 原生发行包，launchd 用户服务 |
| Linux | x86_64、aarch64 | 原生发行包，systemd 用户服务；或 Docker |
| Windows 10／11 | x86_64 | 原生程序，当前用户登录自启 |
| 飞牛 | 按机器实际架构选择 | 原生 systemd 用户服务；或 Docker |

Linux 原生运行需要 ADB；安装脚本需 Python 3.10+ 与 systemd 用户会话。已验证的飞牛 x86_64 发行包使用 musl 静态构建，不依赖特定 glibc 版本；自行使用 GNU 目标构建时，需匹配目标机器的 glibc。Docker 已包含运行依赖。macOS 原生包包含 ADB 与当前架构的文字识别程序；源码构建需 Xcode 命令行工具。

已验证的飞牛静态 x86_64 部署包已保存在仓库 [releases/linux-x86_64/2026-10-23](../releases/linux-x86_64/2026-10-23/README.md)，不依赖特定 glibc 版本；使用说明及实测范围见该目录。

## Windows 部署

需要 Windows 10／11、Python 3.10+（包含 `pythonw.exe`）。源码构建另需 Rust 1.96+、Visual Studio C++ 构建工具、Node.js 22.19+。在项目目录用 PowerShell 执行：

```powershell
npm.cmd --prefix web ci
npm.cmd --prefix web run check
npm.cmd --prefix web run build
cargo build --locked --release
python scripts/setup-adb.py
python scripts/setup-mirror.py
python scripts/install-service.py install
```

`setup-adb.py` 从 Google 官方下载当前 Windows platform-tools，保存到 `data/bin/platform-tools/`。已有完整 ADB 时可跳过下载，安装时用 `--adb "完整路径\adb.exe"` 指定；同目录必须保留 `AdbWinApi.dll`、`AdbWinUsbApi.dll`。

打开 <http://127.0.0.1:17841>。后台安装到 `%LOCALAPPDATA%\ADB Studio`，文件访问限当前用户；登录自启通过启动文件夹内的 `ADB Studio.lnk` 完成，无需管理员权限。实际路径以 `status` 输出为准。关闭网页不停止后台；退出 Windows 登录或电脑休眠会中断运行。

```powershell
python scripts/install-service.py status
python scripts/install-service.py remove
```

`remove` 正常停止后台、关闭自启，保留数据。重新执行 `install` 会更新并启动，保留数据库、令牌和已有 `.env`。查看日志：安装目录的 `logs/service.log`、`logs/service-error.log`。安装目录可用 `--runtime "目录"` 指定，后续安装、状态、移除都需使用相同参数。

ADB 超时导致任务暂停后，“技术详情”保留具体命令、连接选择器、实际耗时与 12 秒时限；后台 `service.log` 记录同一条超时命令。在动作前屏幕检查、截图或连接核验中，还会附带请求手机编号及 `canvas_screen`、`capture`、`target` 等调用阶段。全局命令没有连接选择器时显示“ADB 服务”。配对码与输入文字隐藏，不记录命令输出；原有历史超时记录不回填新信息。

已安装但后台停止时，按 Win+R，输入 `shell:startup`，双击 `ADB Studio` 快捷方式即可启动；无需重新构建。程序隐藏运行，启动后打开工作台确认。登录自启不会在退出登录、关机或休眠时维持任务运行。

连接手机：

1. 手机开启开发者选项、USB 调试，使用数据线连接电脑。
2. 手机提示时允许 USB 调试；若仍不显示，安装手机厂商的 Windows USB 驱动。
3. 工作台进入“设备管理”，确认手机在线，再检查截图与投屏。
4. Android 11+ 也可通过手机的“无线调试”获取地址和配对码，从工作台添加局域网手机。其他工作台连接同一手机时，先停止其任务。

无线配对时保持手机的配对码弹窗打开，关闭后重新获取地址和配对码。配对成功后使用“无线调试”主页面的连接端口连接，两种端口不同。若提示未收到配对回复，先检查手机与电脑的网络是否互通；代理或 VPN 应绕过局域网。电脑接主路由器、手机接访客 Wi-Fi 或下级路由器时，可能无法互通。

文字条件与独立模型看图检查使用“系统设置”中的模型配置，密钥通过环境变量或后台 `.env` 提供。直接填写 Key／Base URL 的设置调整仍待实施，见[模型设置方案](recognition-fallback-plan.md)。Windows 不附带本地读字引擎；不使用模型时，可自行提供 `data/bin/ocr.exe` 或 `ocr.py`，协议见下方环境变量说明。

Windows ZIP 发行包包含后台、网页、ADB 及投屏组件。解压后可运行 `.\adb-studio.exe`；关闭时按 Ctrl+C。需要登录自启时，在解压目录执行 `python scripts/install-service.py install`。源码构建后可用 `python scripts/package-release.py --skip-build` 生成 ZIP，使用 `python scripts/check-release.py dist/adb-studio-windows-x86_64.zip` 验证。

## Mac／Linux 原生发行包

在对应系统解压对应架构的压缩包，进入目录后运行：

```sh
./adb-studio
```

默认打开 `http://127.0.0.1:17841`。Linux 先安装适合本机的 ADB，Android 11+ 无线配对需支持 `adb pair` 的新版本。不要复制 Mac 的 ADB 或可执行文件到 Linux。

安装为后台服务：

```sh
python3 scripts/install-service.py install
python3 scripts/install-service.py status
```

Mac 登录后运行；Linux 使用 systemd 用户服务。Linux 若需无人登录持续运行，由管理员为该用户开启 `loginctl enable-linger 用户名`，或选择 Docker。

```sh
python3 scripts/install-service.py remove
```

移除服务保留运行数据。`install` 可选 `--binary`、`--runtime`、`--adb`，用于指定构建文件、安装位置和本机 ADB；更新不覆盖已有数据库与模型密钥。Mac 重新安装会保留既有服务的环境配置；Linux 自定义环境通过 systemd override 配置，更新主服务文件不删除 override。

新安装自动登记已连接且授权的手机，不预置个人设备编号；升级保留已登记的手机与名称。

默认数据位置：

| 内容 | Mac | Linux |
| --- | --- | --- |
| 安装与数据 | `~/Library/Application Support/ADB Studio/` | `${XDG_DATA_HOME:-~/.local/share}/adb-studio/` |
| 日志 | `~/Library/Logs/ADB Studio/` | `journalctl --user -u adb-studio.service` |
| 自启配置 | `~/Library/LaunchAgents/local.adb-studio.plist` | `${XDG_CONFIG_HOME:-~/.config}/systemd/user/adb-studio.service` |

安装目录的 `data/` 保存数据库、截图、识别样本与访问凭据。模型密钥可用环境变量或安装目录 `.env` 提供。更换自定义密钥变量时，同时配置该变量的实际值。

### Mac 无线连接异常

无线调试重新开启后，先使用手机当前显示的连接端口。若该端口可通过 TCP 访问，但 `adb connect` 报 `No route to host`，同时系统能发现手机广播而 `adb mdns services` 为空，可使用工作台“添加局域网设备 → 连接异常？重启 ADB → 重启 ADB 并重连”。没有网页时，可执行 `adb kill-server`，再执行 `adb start-server`。

重启会短暂断开所有手机与投屏；有运行、等待手机、恢复或已暂停任务时禁止重启。工作台重启后尝试连接已登记手机，优先采用新广播地址，再尝试保存的地址，并核对手机编号。界面会显示连接结果，投屏可重新开启；普通断线仍自动重连，不会自动重启整个 ADB。该按钮也适用于 Windows／Linux。重启后仍失败时，继续检查无线调试、当前端口、配对、局域网访问权限与代理配置。

Mac 后台启动 ADB 时默认使用自带的 libusb 驱动，避免原生 USB 驱动卡在 `IOServiceOpen`、服务启动超时。已有 `ADB_LIBUSB` 环境设置会优先保留；Windows／Linux 不受此设置默认值影响。驱动选择在 ADB 服务启动时生效。

## 飞牛原生部署

飞牛可使用 Linux 原生包，无需 Docker。核对机器架构、glibc 和 ADB 后，按上面的原生安装步骤运行；已开启 `Linger` 的用户可以直接保持后台常驻。USB 手机还需配置该用户的设备读写权限，并在手机上允许 USB 调试。

## 飞牛 Docker

在飞牛上保存本项目源码，在项目目录执行。先复制配置，填写实际密钥与访问网址；`deploy/.env` 和运行目录不进入 Git。

```sh
cp deploy/.env.example deploy/.env
# 编辑 deploy/.env 后：
docker compose --env-file deploy/.env -f deploy/compose.yaml up -d --build
```

只有 USB 手机时，增加设备映射：

```sh
docker compose --env-file deploy/.env -f deploy/compose.yaml -f deploy/compose.usb.yaml up -d --build
```

USB 配置映射 `/dev/bus/usb` 并允许 USB 设备号，适配插拔；无需 `privileged`。它允许容器访问主机 USB 总线，因此只在需要连接 USB 手机时启用。手机仍需授权调试。

网页“设备管理 → 添加局域网设备”可扫描正在广播无线调试的手机。扫描依赖局域网的 mDNS 广播；发现不到时可手动填写地址。配对成功后，返回手机“无线调试”主页，按当前显示的 IP 和连接端口连接。连接端口与配对端口不同，配对后也可能变化。工作台保存连接地址供断线重连；端口变化后需重新填写。也可在容器中手动运行：

```sh
docker compose --env-file deploy/.env -f deploy/compose.yaml exec studio adb pair 手机IP:配对端口
docker compose --env-file deploy/.env -f deploy/compose.yaml exec studio adb connect 手机IP:连接端口
docker compose --env-file deploy/.env -f deploy/compose.yaml exec studio adb devices -l
```

容器使用独立 ADB 服务地址 `127.0.0.1:5038`，避免占用主机默认 5037；手机端视频回传与后台处于同一网络。配对密钥位于持久目录的 `.android/`。不要把 ADB 服务或手机调试端口开放到公网。

工作台数据默认保存到 `deploy/storage/`，可在 `.env` 中用 `ADB_STUDIO_STORAGE` 改成飞牛的绝对路径。容器以 root 运行以便 USB 访问，宿主机目录也会产生 root 所有文件；如只用无线且要改用户，需同时保证数据目录及 HOME 可写。

飞牛控制台需要当前用户具备 Docker 权限。Docker 部署不依赖 macOS 自启脚本。此 Compose 使用 Linux 主机网络，Mac 上推荐原生安装。

## HTTPS 与访问范围

投屏需要浏览器支持 WebCodecs H.264，远程网页需要可信 HTTPS。复用现有反向代理，后端仍为 HTTP；代理须支持 WebSocket，并将浏览器原来的 Host 与 Origin 传给后台，不要删除或伪造来源头。

例如浏览器使用 `https://studio.example.com`，配置：

```text
ADB_STUDIO_ORIGINS=https://studio.example.com
```

多个网址用逗号分隔，必须包含协议与非默认端口，不能带路径、用户名或通配符。若还要通过局域网 HTTP 做普通管理，应明确添加该 HTTP 地址；HTTP 地址一般不能投屏。

配置后仅接受这些网址以及本机 HTTP 的请求。忽略来自客户端的转发协议头；HTTPS 来源由后台配置明确声明。HTTPS 会话使用 Secure Cookie，任务消息与投屏都使用 `wss`。

当前网页在允许的来源自动建立会话，适用于可信家庭网络或受控组网。域名与 HTTPS 本身不提供登录保护；外网发布前需加有登录保护的访问入口，不能仅开放端口。

家庭局域网也可使用自签名服务证书：证书需包含实际访问的 IP 或域名，并在每台客户端导入、信任。仅在浏览器点击“继续访问”不能代替证书信任；不要关闭证书校验。到期或变更地址后需重新签发并更新客户端。

## 环境变量

| 变量 | 作用与默认值 |
| --- | --- |
| `ADB_STUDIO_ROOT` | 网页及组件根目录；原生发行包默认使用可执行文件旁目录，源码运行回退项目目录 |
| `ADB_STUDIO_DATA` | 数据目录；默认根目录下 `data`，容器固定 `/data` |
| `ADB_STUDIO_ADB` | ADB 文件；默认查包内 ADB，Mac 开发目录，再查 PATH；Windows 使用 `adb.exe` |
| `ADB_STUDIO_PYTHON` | Windows 执行 `.py` 识别程序时的 Python 路径，默认 `python` |
| `ADB_STUDIO_STOP_FILE` | Windows 后台的本地停止请求文件，由安装工具设置；手动运行无需设置 |
| `ADB_STUDIO_HOST` | 默认 `127.0.0.1`；局域网或容器为 `0.0.0.0` |
| `ADB_STUDIO_PORT` | 默认 `17841` |
| `ADB_STUDIO_ORIGINS` | 明确允许的网址；未配置时保留现有本机／IPv4 局域网规则 |
| `MODEL_KEY` | 默认模型密钥；自定义变量名在模型设置中填写 |
| `ADB_SERVER_SOCKET` | ADB 自身变量；容器使用 `tcp:5038`（仅本机，支持自动启动 ADB） |

投屏服务端优先读取 `data/bin/scrcpy-server-v3.1`，再读取发行包 `bin/`。Mac／Linux 本地文字识别优先读取 `data/bin/ocr`、包内 `bin/ocr`；仅 Mac 可以回退 Swift。Windows 查找数据目录及包内 `bin/` 的 `ocr.exe`、`ocr.py`。自备识别程序需从标准输入读取 PNG，向标准输出写 JSON 数组，每项包含 `text`、`x`、`y`、`confidence`；坐标为原图像素，可信度范围 0–1。Linux、Windows 不随包安装文字识别引擎。

## 更新、备份与回退

只更新网页时，先备份 `web/build`，复制新资源，最后用临时文件原子替换 `index.html`。保留旧哈希资源供已打开的页面使用；无需重启后台，新界面刷新后生效。

后台更新前记录当前任务编号、参数与进度。在线执行中的任务先请求人工接管，确认暂停后备份并更新，再显式继续原任务；原本已暂停的任务保持暂停。离线等待中的任务保留原记录，重启后继续等待连接。

迁移前先停止旧后台的调度，妥善处理运行及暂停任务。备份完整数据、模型密钥及 ADB 配对信息；运行中的 SQLite 数据库应用备份接口导出，或停服务后复制，不能只复制正在写入的主数据库文件。

Mac 与飞牛同时连同一手机时，只允许一套工作台调度。迁移保留原任务的暂停状态和定时开关，不自动恢复任务或开启定时。回退先停新后台，再恢复旧后台；跨版本回退使用升级前备份，不覆盖原备份。

Compose 更新使用相同命令 `up -d --build`；停止使用 `down`，不要删除持久目录。重建容器后检查手机授权、模型配置、任务状态和样本文件。

## 构建与发布

本机 Windows 已通过后台测试、网页检查和构建、模拟设备、编排器、模型检查、投屏、平台配置与 ZIP 独立启动检查；用户已确认真机 Wi-Fi 连接、截图和投屏验收通过。Mac 已完成本轮投屏接管与刷新恢复的荣耀 USB 实机检查，范围见 [变更记录](changes/2026-10-26-投屏接管与刷新恢复.md)；Mac 完整实机回归与完整 Linux 后台回归尚未完成。WSL Ubuntu 仅完成平台配置检查，不能代替完整 Linux 验收。

```sh
python3 scripts/package-release.py
```

脚本在当前系统构建网页与 release 后台，下载并校验固定 scrcpy 组件；Mac 额外编译识别程序。Windows 下载并包含官方 ADB。输出 `dist/adb-studio-系统-架构.tar.gz`（Windows 为 `.zip`）及 SHA-256 文件，排除数据库、截图、配对信息与密钥。

`.github/workflows/release.yml` 配置 Mac／Linux、Intel／ARM 以及 Windows x86_64 的构建和模拟检查；手动触发可下载构建产物，推送 `v*` 标签时创建 GitHub 草稿发行版。只有用户确认后才提交、推送或创建正式标签。产物当前未签名；macOS 正式对外发行前还需配置签名与公证。

Docker 检查与构建：

```sh
docker build --target checks -t adb-studio-check .
docker build -t adb-studio .
```

飞牛 Debian 12 x86_64 的原生升级、HTTPS 投屏与实体屏幕开关记录见 [飞牛原生部署](feiniu-native.md)。真实手机连接、TLS 证书、目标 NAS 权限及其他平台架构的实际表现，仍需分别验收。各平台的实测范围应在发行说明中标明。
