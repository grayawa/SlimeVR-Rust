# SlimeVR React / Tauri 前端

`gui/src` 是沿用 SlimeVR 页面与 Fluent 翻译的 React 界面，`gui/src-tauri` 是 Tauri 2 原生宿主。GPUI 原生界面位于独立的 `gui-gpui/`，两者共用 Rust 后端和 SolarXR 协议。

## 环境与启动

需要根目录 `.node-version` 指定的 Node.js、pnpm 10.33.0、Rust 1.88+ 和 Python 3。完整桌面包还需要 CMake 3.26+、C++23 编译器和对应系统依赖，见 [Tauri 官方要求](https://v2.tauri.app/start/prerequisites/)。Windows 使用 Visual Studio C++ Build Tools、Windows SDK 和 WebView2；macOS 使用 Xcode Command Line Tools。Debian/Ubuntu 的界面依赖示例：

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

从仓库根目录执行：

```sh
git submodule update --init --recursive
corepack enable
pnpm install --frozen-lockfile
pnpm tauri:rust:dev
```

`pnpm gui` 与 `pnpm tauri:rust:dev` 相同，准备协议绑定、Rust 后端、SteamVR 驱动和 OpenVR helper，再启动 Tauri 开发窗口。首次准备驱动需要联网，固定版本和校验哈希由构建脚本管理。SteamVR 桌面桥接支持 Windows / Linux；macOS 可使用浏览器、OSC 和 VMC。

## 网页开发与已有后端

```sh
# 浏览器开发，需要另外启动 Rust 后端
pnpm web

# 只启动 Tauri 窗口，连接已有后端
pnpm --dir gui exec tauri dev -- -- --no-server
```

界面默认连接 `ws://localhost:21110`。浏览器可通过 `http://127.0.0.1:5173/?ip=192.168.1.10&port=21110` 指定其他服务。宿主发现本机端口已占用时跳过后端启动；自动连接以端口可用性为判断依据。启动前确认该端口由目标后端提供服务。

Tauri CLI 第二个 `--` 后的参数传给应用；打包后的程序直接接收参数：

| 参数                         | 用途                                |
| ---------------------------- | ----------------------------------- |
| `--no-server`                | 只连接已有后端                      |
| `--backend auto` / `rust`    | 自动发现后端 / 要求找到 Rust 后端   |
| `--rust-server <file>`       | 指定后端可执行文件                  |
| `--path <directory>`         | 指定后端所在目录                    |
| `--config <file>`            | 指定原版 `vrconfig.yml` / `.yaml`   |
| `--log-level <level>`        | error / warn / info / debug / trace |
| `--no-steamvr`               | 关闭 SteamVR 桥接                   |
| `--steamvr-endpoint <path>`  | 覆盖 SteamVR IPC 端点               |
| `--bindings-provider <file>` | 指定 OpenVR helper                  |
| `--no-bindings-provider`     | 禁用 OpenVR helper                  |

退出时宿主关闭自己启动的后端的 stdin，让 BVH 和 journal 正常完成；超时后终止。单独启动的后端由原启动方式管理。

## 构建与资源

```sh
# 完整桌面包：网页、宿主、Rust 后端、驱动和 OpenVR helper
pnpm tauri:rust:build

# 仅宿主及网页，连接单独运行的后端
pnpm tauri:build
```

输出在 `gui/src-tauri/target/release/`，安装包位于其 `bundle/` 目录。各平台使用对应的 `tauri.rust*.conf.json`，后端使用 Rust 可执行文件。直接调用 `tauri dev/build` 前执行 `pnpm --dir gui tauri:prepare`，准备脚本和完整构建脚本会选择目标平台资源。

Windows 解压包由 [GitHub Actions](../docs/rust-ci.zh-CN.md) 构建，WebView2 使用系统安装。已有 SlimeVR 驱动可直接复用，程序保留现有注册；固件升级功能仍然保留。软件包从本仓库 artifacts 获取。

## 配置、桌面功能与日志

浏览器把界面偏好保存在 localStorage，Tauri 使用 `gui-settings.dat` / `gui-cache.dat`，保持原有文件名、JSON 格式和应用标识 `dev.slimevr.SlimeVR`。读取和保存保留未知偏好字段。后端直接读写原版 YAML，见 [兼容范围](../docs/rust-config-compatibility.zh-CN.md)。

| 系统    | Tauri 界面数据目录                                  | 默认后端配置目录                       |
| ------- | --------------------------------------------------- | -------------------------------------- |
| Windows | `%APPDATA%/dev.slimevr.SlimeVR`                     | 同左                                   |
| macOS   | `~/Library/Application Support/dev.slimevr.SlimeVR` | 同左                                   |
| Linux   | `$XDG_DATA_HOME/dev.slimevr.SlimeVR`                | `$XDG_CONFIG_HOME/dev.slimevr.SlimeVR` |

Linux 未设置 XDG 变量时使用 `~/.local/share/` 与 `~/.config/`。窗口位置由 window-state 插件管理，托盘提供显示、隐藏和退出。原生文件对话框、外链、配置/日志目录、Discord Rich Presence 均通过 Tauri 适配层提供；浏览器使用相应回退行为。`override.ftl` 从后端配置目录加载。

界面日志为数据目录下的 `logs/gui-tauri.log`，默认级别 `info`，可用 `--log-level debug` 或 `SLIMEVR_LOG_LEVEL` 调整。约 10 MiB 轮转，保留四份历史文件。便携包带 `Start-SlimeVR-Debug.cmd`；详细收集方法见 [日志说明](../docs/rust-logging.zh-CN.md)。问题排查使用本地日志。

## 验证

```sh
pnpm --dir gui lint
pnpm --dir gui test:desktop
pnpm --dir gui test:backend
pnpm --dir gui test:websocket
pnpm --dir gui web:build
cargo test --manifest-path gui/src-tauri/Cargo.toml --locked
```

通信测试使用 `server-rust/target/debug/slimevr-server`，先构建它或设置 `SLIMEVR_RUST_BINARY`。桌面适配测试覆盖浏览器回退、Tauri 检测、文件对话框与取消、配置/缓存保存、日志分级及事件清理。

`pnpm --dir gui test:assignment-browser` 构建网页并通过真实后端测试分配页、设置更新与断线恢复。需要 Chromium、Chrome 或 Edge，也可指定 `SLIMEVR_BROWSER_EXECUTABLE`。若未检测到浏览器，测试会跳过，请检查结果是否实际执行。

设备通信、佩戴和 SteamVR / VRChat 行为仍按 [统一实机测试清单](../docs/rust-unified-hardware-test.zh-CN.md) 验证。
