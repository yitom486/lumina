# ffprobe（项目本地，不入库）

Windows x64 **essentials** 包里的 `ffprobe.exe`，仅用于媒体元数据探测。不要装到系统 PATH。

## 当前版本

- 来源：[gyan.dev FFmpeg builds](https://www.gyan.dev/ffmpeg/builds/)
- 发行：`ffmpeg-release-essentials.zip`（下载时为 9.0.1-essentials）
- 运行时：`ffprobe.exe`（本阶段不需要 `ffmpeg.exe`）

## 本目录应有

| 文件 | 作用 |
|------|------|
| `ffprobe.exe` | CLI 探测（`-print_format json -show_format -show_streams`） |

二进制已 gitignore。重新安装：

```powershell
$dir = "$env:TEMP\lumina-ffmpeg"
$dest = "$PSScriptRoot"
New-Item -ItemType Directory -Force -Path $dir, $dest | Out-Null
curl.exe -L --fail -o "$dir\ffmpeg-essentials.zip" `
  https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip
$extract = "$dir\extract"
New-Item -ItemType Directory -Force -Path $extract | Out-Null
tar.exe -xf "$dir\ffmpeg-essentials.zip" -C $extract
$ffprobe = Get-ChildItem -Path $extract -Recurse -Filter ffprobe.exe | Select-Object -First 1
Copy-Item -Force $ffprobe.FullName "$dest\ffprobe.exe"
```

运行时由 `MediaInspector` 解析：优先 `src-tauri/native/ffmpeg/ffprobe.exe`，开发时相对 CARGO_MANIFEST_DIR；打包后相对 exe 旁 `ffprobe.exe`（后续 build 可拷贝）。
