# Java 工程清理

生产后端统一使用 `server-rust/`。旧 `server/core`、`server/desktop`、`server/android`、Gradle wrapper / 配置和 Java 版本探测 JAR 已移除。原代码仍可从 Git 历史或 [固定的上游版本](https://github.com/SlimeVR/SlimeVR-Server/tree/83941fd38e91cc91ca6b360deab5c2ae986dd1b6) 查看。

Tauri 和 Electron 自动启动 Rust 可执行文件，沿用 `vrconfig.yml` / `.yaml`、SolarXR 和既有 GUI 设置。`--rust-server` 指定可执行文件，`--path` 指定所在目录，`--no-server` 只连接已经运行的服务。Tauri 的 `--backend auto` / `rust` 保留；Java JAR / JVM 的启动选项和打包配置已移除。关闭自己启动的后端时先关闭 stdin，让录制和 journal 正常完成；超时后终止进程。

移植数学代码的 ktmath / jMonkeyEngine 许可证保留在 `server-rust/licenses/`，并随 Tauri、Electron、GPUI 和 Overlay 构建包分发。

GPUI、Overlay、共享 React 页面、翻译、OpenVR helper 和 SteamVR 驱动资源保留。WebView2 需求仍取决于是否使用 Tauri。旧 Java / Android 构建任务已从 CI 移除，当前工作流见 [CI 说明](rust-ci.zh-CN.md)。

Rust 算法测试继续使用已有 golden fixtures，普通构建和测试无需 Java。用于重新生成参考数据的 Kotlin 适配器仅在 `server-rust/tools/` 中使用；SolarXR 子模块保留其上游生成的各种语言协议。

参考生成工具通过 `reference_sources.py` 从 Git 历史导出上游提交 `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`，放在系统临时目录的 `slimevr-upstream-reference/<提交>/`，不恢复当前工作区的旧工程。原始文件路径和 SHA-256 仍记录在 fixtures 中。重新生成时需要 Python 3.12+、Java / Kotlin 编译环境和对应 Maven 依赖，沿用各工具原有命令。

浅克隆缺少参考提交时，可先获取固定版本：

```sh
git fetch https://github.com/SlimeVR/SlimeVR-Server.git 83941fd38e91cc91ca6b360deab5c2ae986dd1b6
```

也可设置 `SLIMEVR_REFERENCE_ROOT`，指向单独的上游 Git checkout；此时记录该 checkout 的提交与实际源码哈希。普通 Rust 测试不需要执行这些生成工具，也不需要完整 Git 历史。
