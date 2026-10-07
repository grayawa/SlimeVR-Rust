# Windows 11 x64 便携包

日期：2026-10-04。本次产物为 `SlimeVR-Rust-Tauri-0.1.0-Windows11-x64.zip`，30.54 MiB。

2026-10-06：新增 `Windows11-x64-fix6-hand1.zip`。重新构建 Tauri 生产网页与 Windows 宿主，使用与 GPUI 手部切换修复版相同的 Rust 后端。新增节点来源保护，详见 [手部切换修复说明](rust-steamvr-hand-handover.zh-CN.md)。仍不附 WebView2 Runtime；运行前完全退出旧前端和后端。

2026-10-05：新增 `Windows11-x64-fix1.zip` 修复包；连接竞态、发现广播与日志变更见 [fix1 说明](rust-windows-fix1.zh-CN.md)。

fix1 下载：https://files.catbox.moe/yvbw0e.zip ，30.52 MiB；已重新下载并验证 SHA-256 和 ZIP CRC。SHA-256：`d6f9af54b67fe9f42458c21c5661b8437b82eebfd0532d7a02b2658ca608bbe1`。

解压整个文件夹，退出原 Java 服务，再双击 `SlimeVR.exe` 或 `Start-SlimeVR.cmd`。界面和 Rust 后端自动一起启动。使用 Windows 11 已有的 WebView2，按用户要求不附运行库；`WebView2Loader.dll` 是必需的调用加载库，不是 WebView2 Runtime。

包内包含原版官方 v6.0.0 Windows SteamVR 驱动、从参考源码交叉编译的 Bindings Provider、OpenVR DLL、官方 VC++ x64 依赖及许可、使用说明、实机清单和文件 SHA-256 清单。无需安装 Java、Node 或 Rust。配置仍使用 `%APPDATA%/dev.slimevr.SlimeVR/vrconfig.yml` / `vrconfig.yaml`；这是免安装发行形式，不改变原配置路径。SteamVR 注册驱动后保持解压目录位置不变。

后端和界面使用 `x86_64-pc-windows-gnu` release 交叉构建；界面显式启用 `tauri/custom-protocol`，生产网页嵌入 EXE，不依赖 Vite 开发服务器。GUI 是 x64 Windows GUI 子系统，不弹后端控制台。

当前验证：所有 EXE / DLL 的 PE x64 架构、导入依赖完整性、生产编译状态、启动脚本、ZIP CRC、每个打包文件 SHA-256、必需资源存在和未打包 WebView2 Runtime。尚未在 Windows 11 原生环境运行，真实硬件和 SteamVR 按 [统一实机清单](rust-unified-hardware-test.zh-CN.md) 验收。

可复用打包脚本：`gui/scripts/package-windows-portable.py`。输入生产 GUI EXE、Rust 后端 EXE、Bindings Provider 目录，以及官方 VC++ DLL / 许可 / 下载来源目录；输出 ZIP 和 `.zip.sha256`。`--help` 提供参数说明。GUI 直接用 Cargo 构建时必须启用 `--features tauri/custom-protocol`，仅 `--release` 不能保证生产网页加载。

本次官方下载的 VC++ redistributable SHA-256 为 `cc0ff0eb1dc3f5188ae6300faef32bf5beeba4bdd6e8e445a9184072096b713b`；完整下载 URL 和所附 DLL 的哈希在压缩包 `licenses/VC-Runtime-SOURCE.json` 和 `BUILD-MANIFEST.json` 中。ZIP SHA-256：`cf01f349b44def6787005cf7151ab60f4ae7fc5cdc082d4b38ca271f82f0afc8`。
