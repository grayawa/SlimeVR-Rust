# Windows 修复包 fix1

日期：2026-10-05。适用于 Windows 11 x64；继续使用系统 WebView2 和原版 `vrconfig.yml` / `vrconfig.yaml`。

## 连接与发现

- WebSocket 监听器由各自的连接持有并清理。旧连接的延迟 `open` / `close` / `error` / `message` 事件不能再改变新连接状态；组件卸载后，已排队的回调也失效。
- 界面断线提示说明连接中断或连接超时，不再仅凭 WebSocket 断开就宣称后端崩溃。
- 发现广播改为每 10 秒一次，启动时先留出同样的安静窗口。官方固件的 `Connection::update()` 收到服务器广播也会刷新连接超时；原先每 2 秒广播，在切换后端后可能让未重新握手的设备一直维持旧连接状态。固件主动发送握手的接收和响应没有等待限制，已连接设备的保活频率不变。

广播间隔调整针对官方固件的超时行为。定制固件、网络问题仍需实机核对；切换后端前退出 Java 服务，设备仍未出现时重启对应追踪器。

固件参考：[connection.cpp](https://github.com/SlimeVR/SlimeVR-Tracker-ESP/blob/main/src/network/connection.cpp)，`searchForServer()` / `update()`。

## 日志

GUI 记录 WebSocket 连接成功、断开码、原因、是否正常关闭，以及错误时的连接状态。错误对象经 IPC 写入日志时保留名称、消息、调用栈和嵌套原因，避免只留下 `{}`。

后端连接任务失败时写入 `api_connection_error`，含客户端编号、地址和错误原因；Tauri 将其收集到同一日志。失败只结束对应连接，接收端和其他客户端继续运行。

日志默认位于 `%APPDATA%\dev.slimevr.SlimeVR\logs\gui-tauri.log`；界面“打开日志文件夹”也可定位。

## 验证

- 113 个 Rust 测试通过，Clippy 无警告。新增真实 UDP / CLI 测试模拟官方固件的 3 秒超时，验证切换后端后不重启也能重新握手、注册传感器。
- 22 个 GUI / 桌面 / 后端协议测试通过。新增旧连接事件竞态、卸载清理、错误日志序列化，以及非法 WebSocket 数据的诊断与服务存活测试。
- TypeScript 类型检查、改动文件 ESLint、生产网页构建通过。
- Windows 后端和 GUI 使用 release 交叉编译；打包检查 PE 架构、DLL 依赖、网页资源嵌入、文件 SHA-256 和 ZIP 完整性。修复包未在本环境执行 Windows 原生运行。

首次断线的原因仍须由新日志确认；修复旧连接事件竞态不代表已经证明它就是某次实机断线的触发原因。
