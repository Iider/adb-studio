# Mac／Linux 部署与发行

macOS、Linux 共用 Rust 后台与静态网页。Mac 可使用苹果文字识别；Linux 可直接运行纯视觉任务、截图、投屏及不依赖读字的任务。本地读字任务在没有兼容识别程序时会在开始前提示原因。

## 平台范围

| 系统 | 架构 | 运行方式 |
| --- | --- | --- |
| macOS | Apple Silicon、Intel | 原生发行包，launchd 用户服务 |
| Linux | x86_64、aarch64 | 原生发行包，systemd 用户服务；或 Docker |
| 飞牛 | 按机器实际架构选择 | 原生 systemd 用户服务；或 Docker |

Linux 原生包需与构建环境兼容的 glibc、ADB；安装脚本需 Python 3.10+ 与 systemd 用户会话。官方 Linux 发行文件在 Debian 12 环境编译，要求 glibc 2.36 或更新版本；更旧 Linux 需另行构建。Docker 已包含运行依赖。macOS 原生包包含 ADB 与当前架构的文字识别程序；源码构建需 Xcode 命令行工具。

已验证的飞牛静态 x86_64 部署包已保存在仓库 [releases/linux-x86_64/2026-10-23](../releases/linux-x86_64/2026-10-23/README.md)，不依赖特定 glibc 版本；使用说明及实测范围见该目录。

## 原生发行包

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

网页“设备管理 → 添加局域网设备”可扫描正在广播无线调试的手机，点选地址后配对或连接。扫描依赖局域网的 mDNS 广播；发现不到时使用手机“无线调试”页面显示的地址手动填写。配对端口与连接端口不同。工作台保存连接地址供断线重连；手机端口变化后重新填写。也可在容器中手动运行：

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
| `ADB_STUDIO_ADB` | ADB 文件；默认查包内 ADB，Mac 开发目录，再查 PATH |
| `ADB_STUDIO_HOST` | 默认 `127.0.0.1`；局域网或容器为 `0.0.0.0` |
| `ADB_STUDIO_PORT` | 默认 `17841` |
| `ADB_STUDIO_ORIGINS` | 明确允许的网址；未配置时保留现有本机／IPv4 局域网规则 |
| `MODEL_KEY` | 默认模型密钥；自定义变量名在模型设置中填写 |
| `ADB_SERVER_SOCKET` | ADB 自身变量；容器使用 `tcp:5038`（仅本机，支持自动启动 ADB） |

投屏服务端优先读取 `data/bin/scrcpy-server-v3.1`，再读取发行包 `bin/`。本地文字识别优先读取 `data/bin/ocr`、包内 `bin/ocr`；仅 Mac 可以回退 Swift。Linux 可自行提供相同 PNG 标准输入、JSON 标准输出协议的程序，但不随包安装文字识别引擎。

## 更新、备份与回退

迁移前先停止旧后台的调度，妥善处理运行及暂停任务。备份完整数据、模型密钥及 ADB 配对信息；运行中的 SQLite 数据库应用备份接口导出，或停服务后复制，不能只复制正在写入的主数据库文件。

Mac 与飞牛同时连同一手机时，只允许一套工作台调度。迁移不自动解除原任务暂停，不自动启用尚未试跑的工作流。回退先停新后台，再恢复旧后台；跨版本回退使用升级前备份，不覆盖原备份。

Compose 更新使用相同命令 `up -d --build`；停止使用 `down`，不要删除持久目录。重建容器后检查手机授权、模型配置、任务状态和样本文件。

## 构建与发布

```sh
python3 scripts/package-release.py
```

脚本在当前系统构建网页与 release 后台，下载并校验固定 scrcpy 组件；Mac 额外编译识别程序。输出 `dist/adb-studio-系统-架构.tar.gz` 及 SHA-256 文件，排除数据库、截图、配对信息与密钥。

`.github/workflows/release.yml` 配置 Mac／Linux、Intel／ARM 四个平台的构建和模拟检查；手动触发可下载构建产物，推送 `v*` 标签时创建 GitHub 草稿发行版。只有用户确认后才提交、推送或创建正式标签。产物当前未签名；macOS 正式对外发行前还需配置签名与公证。

Docker 检查与构建：

```sh
docker build --target checks -t adb-studio-check .
docker build -t adb-studio .
```

飞牛 Debian 12 x86_64 的原生升级、HTTPS 投屏与实体屏幕开关记录见 [飞牛原生部署](feiniu-native.md)。真实手机连接、TLS 证书、目标 NAS 权限及其他平台架构的实际表现，仍需分别验收。各平台的实测范围应在发行说明中标明。
