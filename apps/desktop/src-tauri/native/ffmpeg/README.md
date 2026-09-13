# FFmpeg / ffprobe（项目本地，不入库）

Windows x64 **essentials**（gyan.dev），给媒体探测与字幕抽出。不要装到系统 PATH。

## 当前版本

- 来源：https://www.gyan.dev/ffmpeg/builds/
- 包：`ffmpeg-release-essentials.zip` → 9.0.1-essentials
- 文件：`ffprobe.exe`、`ffmpeg.exe`

## 用途

| 工具 | Phase | 用途 |
|------|-------|------|
| ffprobe | 2 | 容器/流元数据 JSON |
| ffmpeg | 3 | 抽出文本字幕轨 → SRT/ASS |

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
