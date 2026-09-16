# FFmpeg / ffprobe（项目本地，不入库）

各平台安装包都携带本地 FFmpeg/ffprobe，给媒体探测与字幕抽出使用；不要要求用户
另行安装到系统 PATH。

## 当前版本

- Windows x64 来源：https://www.gyan.dev/ffmpeg/builds/
- Windows 包：`ffmpeg-release-essentials.zip` → 9.0.1-essentials
- Windows 文件：`ffprobe.exe`、`ffmpeg.exe`
- macOS/Linux 来源：https://ffmpeg.martin-riedl.de/
- macOS/Linux release：9.0.1；按 runner 架构下载静态 `ffmpeg`、`ffprobe`
- Unix 文件名：`ffprobe`、`ffmpeg`

构建前由 `.github/actions/setup-native-tools/action.yml` 下载并放入本目录；原件不入库。
Tauri bundle 再将它们复制到安装包的 `resources/ffmpeg/`。

## 用途

| 工具 | 使用场景 | 用途 |
|------|-------|------|
| ffprobe | 媒体探测 | 容器/流元数据 JSON |
| ffmpeg | 字幕处理 | 抽出文本字幕轨 → SRT/ASS |

## 重装

见历史脚本或：

```powershell
# 假设 zip 已在 %TEMP%\lumina-ffmpeg\ffmpeg-essentials.zip
$extract = "$env:TEMP\lumina-ffmpeg\extract"
$dest = "$PSScriptRoot"
tar.exe -xf "$env:TEMP\lumina-ffmpeg\ffmpeg-essentials.zip" -C $extract
Copy-Item -Force (Get-ChildItem $extract -Recurse -Filter ffprobe.exe | Select -First 1).FullName "$dest\ffprobe.exe"
Copy-Item -Force (Get-ChildItem $extract -Recurse -Filter ffmpeg.exe | Select -First 1).FullName "$dest\ffmpeg.exe"
```
