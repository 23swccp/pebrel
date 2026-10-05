# Offline relay installation / 离线中转安装

This kit contains both Linux x86_64 and ARM64 binaries extracted from one verified
Pebrel APK. `SOURCE_COMMIT` identifies the binaries' source commit. No executable
is downloaded during installation. The installer selects the host architecture
and verifies its SHA256 before executing the bundled binary.

本安装包从经过校验的同一份 Pebrel APK 提取 Linux x64 与 ARM64 二进制。
`SOURCE_COMMIT` 记录二进制的源码提交；安装时不下载可执行文件。
脚本选择服务器架构，校验 SHA256 后才运行包内程序。

## Requirements / 环境要求

- Linux x64/ARM64 with running systemd 239+ or Alpine OpenRC.
- Root access for installation and management. The relay itself drops root privileges.
- `sh`, `uname`, `id`, `dirname`, `sha256sum`, and `chmod`.
- Your desktop and phone must reach the selected relay port. The installer does
  not change firewall rules. Docker, Node.js and a domain name are not required.

需要运行中的 systemd 239+ 或 Alpine OpenRC；管理服务使用 root 权限，中转
进程自身不以 root 身份运行。电脑和手机需能访问所选端口，脚本不修改防火墙。

## Install / 安装

Transfer the archive and its `.sha256` file to the server, then use the actual
archive filename in place of `Pebrel-vVERSION-relay-manual.tar.gz` below:

将安装包和对应的 `.sha256` 文件上传至服务器，把以下文件名替换为实际名称：

```sh
sha256sum -c Pebrel-vVERSION-relay-manual.tar.gz.sha256
tar -xzf Pebrel-vVERSION-relay-manual.tar.gz
cd pebrel-relay-manual
sudo sh install.sh SERVER_IP 443
```

Replace `SERVER_IP` with the IP address or hostname your devices use to reach
this server. The second argument is the relay port and defaults to `443`.
Run `sh install.sh ...` directly if already logged in as root.

`SERVER_IP` 填写电脑和手机能访问的服务器 IP 或主机名；第二个参数为中转端口，
省略时使用 `443`。已使用 root 登录时直接执行 `sh install.sh ...`。
脚本展示四个步骤，任一步骤失败就停止，不把安装失败显示为成功。

The bundled binary owns service setup, permission checks, conflict detection and
readiness verification. Existing modified installations are not silently replaced.

服务安装、权限检查、冲突检测和就绪验证由包内二进制统一处理；不会静默覆盖
已被修改的安装。错误时保留输出中的错误代码，先检查服务状态再重试。

## Connect and manage / 连接与管理

On the desktop, open **Settings → Phone connection → Relay**, select this SSH
server and choose **Use this relay server**. Generate the pairing QR code there
and scan it on your phone. SSH passwords and private-key passphrases are SSH
credentials, not relay passwords; the installation generates relay credentials.

在电脑“设置 → 手机连接 → 中转服务器”中选择此 SSH 主机并使用它，再生成
二维码供手机扫描。SSH 密码与私钥口令只用于 SSH 认证，中转访问凭据由安装
流程生成，不需要在安装表单另设中转密码。

```sh
sudo /opt/pebrel-relay/pebrel-relay service-status
sudo /opt/pebrel-relay/pebrel-relay service-stop
sudo /opt/pebrel-relay/pebrel-relay service-start
```

`service-uninstall` stops and removes managed service files while retaining
pairing configuration. Add `--purge` only when you intend to remove that
configuration and its keys as well.

`service-uninstall` 停止并移除受管理的服务文件，默认保留配对配置；只有需要
同时删除配对配置和密钥时才追加 `--purge`。

## Build this kit / 生成安装包

From a source checkout containing this installer, provide the verified APK and
the exact 40-character source commit of its embedded binaries:

在包含本安装器的源码目录执行，指定经过校验的 APK 与包内二进制对应的完整
40 位源码提交：

```sh
python3 mobile/tools/package_manual_relay.py \
  --apk Pebrel-vVERSION-android-universal-preview.apk \
  --commit SOURCE_COMMIT \
  --output Pebrel-vVERSION-relay-manual.tar.gz
```

The command writes the archive and its SHA256 sidecar. Creating a kit locally
does not publish a Release or modify an existing public asset.

此命令生成离线压缩包与 SHA256 校验文件，不会发布 Release 或修改公开资产。
