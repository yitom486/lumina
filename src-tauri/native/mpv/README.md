# libmpv（项目本地，不入库）

Windows x64 **dev** 包，给 Lumina 链接和运行时加载。不要装到系统 PATH。

## 当前版本

- 来源：[shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake/releases)
- 发行：`20260830`
- 文件：`mpv-dev-x86_64-20260830-git-e8673660ab.7z`（普通 x86_64，不是 v3/AVX2）
- 运行时：`libmpv-2.dll`
- 链接：`mpv.lib`（由 DLL 导出生成，MSVC）

## 本目录应有

| 文件 | 作用 |
|------|------|
| `libmpv-2.dll` | 运行时 |
| `mpv.lib` / `mpv.def` | MSVC 链接 |
| `include/mpv/*.h` | 头文件 |
| `libmpv.dll.a` | MinGW 导入库（MSVC 目标可忽略） |

二进制已 gitignore。重新安装：

```powershell
$dir = "$env:TEMP\lumina-mpv"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
curl.exe -L --fail -o "$dir\mpv-dev.7z" `
  https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20260830/mpv-dev-x86_64-20260830-git-e8673660ab.7z
tar.exe -xf "$dir\mpv-dev.7z" -C "$PSScriptRoot"
```

然后用 VS `dumpbin /exports` + `lib /def` 再生成 `mpv.lib`（`/name:libmpv-2.dll`）。

M4 起由 `build.rs` 指向此目录，并把 DLL 拷到 exe 旁。在此之前请勿把 DLL 加到全局 PATH。
