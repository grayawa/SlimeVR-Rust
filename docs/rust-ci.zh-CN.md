# Rust 分支的 GitHub Actions 检查

Rust / Tauri、GPUI 和 SteamVR Dashboard 工作流支持手动构建，也会在相关 PR 中自动运行。PR 按修改路径触发，避免翻译或无关工作流的更新重复构建所有 Rust 界面。

| 工作流 | PR 检查范围 | 手动构建 |
| --- | --- | --- |
| Rust and Tauri test bundles | Windows：后端测试和 Clippy、前端类型与通信测试、Tauri 构建和宿主测试 | Windows、Linux x64 / ARM64、macOS |
| GPUI native frontend | Windows：通信 / 状态测试、Clippy、后端和界面构建、着色器检查、真实后端回环测试 | Windows |
| SteamVR dashboard Windows bundle | Windows：通信和输入测试、Clippy、Overlay / 后端构建、真实后端回环测试、运行库校验和打包 | Windows |

修改各工作流自身会触发对应检查；修改共享后端、SolarXR 或 OpenVR 依赖也会触发相关工作流。GitHub Actions 的 Dependabot 更新因此能验证实际受影响的 Rust 构建。手动构建保留原来的 artifact 下载入口。PR 更新会取消同一 PR 的旧运行，手动运行不会被取消。

`Web GUI and workflow checks` 检查 React / Tauri 的类型和格式、构建共享网页，并运行固定版本、校验 SHA-256 的 actionlint。旧 Java、Gradle、Android 和 Java 安装包的构建流程已移除。`setup-node` / `setup-python` 的 action 版本与安装的 Node / Python 版本分别配置。

Dependabot 仍每周检查 GitHub Actions 更新。合并前查看相关 PR 的实际检查结果；Windows 构建和自动测试通过后，SteamVR 中的真实设备表现仍由实机测试确认。
