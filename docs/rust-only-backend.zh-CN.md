# Rust 后端与上游参考工具

生产后端位于 `server-rust/`。GPUI 和 Tauri 负责启动 Rust 可执行文件，后端使用 `vrconfig.yml` / `.yaml` 与 SolarXR 协议，GUI 偏好由前端保存。

## Tauri 后端生命周期

`--rust-server` 指定可执行文件，`--path` 指定所在目录，`--no-server` 连接已运行的服务。`--backend auto` 自动发现后端，`--backend rust` 要求找到 Rust 可执行文件。

退出自己启动的后端时，宿主先关闭 stdin，让录制和 journal 正常完成；等待超时后终止进程。外部服务的生命周期由其启动方式管理。完整参数见 [Tauri 指南](../gui/README.tauri.md)。

## 参考源码与 fixtures

Rust 算法测试读取提交到仓库的 golden fixtures。参考生成工具位于 `server-rust/tools/`，通过固定的上游提交读取 Java / Kotlin 源码，并在隔离目录中构建测试适配器。参考版本为 [83941fd38e91cc91ca6b360deab5c2ae986dd1b6](https://github.com/SlimeVR/SlimeVR-Server/tree/83941fd38e91cc91ca6b360deab5c2ae986dd1b6)。

普通构建和测试使用 Rust 工具链。重新生成参考数据需要 Python、JDK 和参考依赖；生成器帮助提供源码位置、缓存及 JDK 参数。SolarXR 子模块包含上游生成的各语言协议绑定。

```sh
cargo test --manifest-path server-rust/Cargo.toml --workspace --locked
python3 server-rust/tools/generate-core-golden.py
python3 server-rust/tools/generate-autobone-golden.py --skip-core-build
```

参考执行范围、容差和数据来源见 [核心验证](rust-core-validation.zh-CN.md) 与 [校准和 AutoBone](rust-calibration-autobone.zh-CN.md)。仓库资源和开发入口见 [仓库内容](repository-cleanup.zh-CN.md)。
