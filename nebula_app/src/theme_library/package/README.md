# 主题 ZIP / Theme ZIP

一个 ZIP 包含主题 JSON、作者信息和图片资源，导入后可以离线使用。

```text
example.pebrel-theme.zip
  manifest.json
  theme.pebrel-theme.json
  assets/
    background.png
    preview.jpg
```

## 分享 / Share

先在主题编辑器导出 Pebrel JSON，再打包：

```sh
pebrel theme pack example.pebrel-theme.json --output example.pebrel-theme.zip --author "Your name" --github your-handle --license CC-BY-4.0 --version 1.0.0 --preview preview.jpg
```

`--preview` 是可选的社区预览图；背景的相对路径以主题 JSON 所在目录为基准。

检查或导入别人分享的文件：

```sh
pebrel theme check example.pebrel-theme.zip
pebrel theme import example.pebrel-theme.zip
```

导入只添加到本地主题库。在主题选择器中选择并应用，才会改变窗口。
本阶段打包支持静态背景图片及可选预览图；编辑器内的 ZIP 导出入口和动态媒体后续接入。
删除主题 JSON 目前保留已安装的资源目录；自动清理和应用内卸载尚未接入。

Export Pebrel JSON from the theme editor, then use `theme pack` to include the
local background image. `theme check` verifies the ZIP and resources; `theme
import` adds it to the local library without applying it.

## 体积 / Size

| 项目 / Item | 上限 / Maximum |
| --- | --- |
| ZIP 文件 / Archive | 48 MiB |
| 解压总量 / Unpacked total | 64 MiB |
| 单视频及视频合计 / One video and video total | 32 MiB |
| 清单 / Manifest | 64 KiB |
| 文件数 / Entries | 64 |

实际字节数、路径和 SHA-256 都会检查。打包或导入不会执行包内代码。
`pebrel theme limits` 输出安装器实际使用的限制。

图片路径在包内使用相对路径，导入时转换为安装目录内的本地路径。
不接受路径穿越、盘符、设备文件、符号链接、重复文件名、加密或分卷 ZIP。
格式版本 1 使用普通 ZIP；Zip64、ZIP 文件注释与目录占位条目不属于本阶段合同。

The installer enforces actual stream sizes and SHA-256, validates portable
relative paths, and rejects undeclared entries. Video/animation/shader resource
kinds reserve metadata for later capabilities; this build does not activate them.
The CLI handles files as cold operations, without resident scans or media decoding.
