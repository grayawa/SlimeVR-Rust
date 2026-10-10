# 文档目录

这里提供 SlimeVR-Rust 的当前行为、开发入口、验证方法与实机验收范围。安装和首次启动从 [项目 README](../README.md) 开始，版本变更见 [CHANGELOG](../CHANGELOG.md)。

## 使用与测试

| 文档                                                  | 内容                                                          |
| ----------------------------------------------------- | ------------------------------------------------------------- |
| [功能状态](rust-feature-status.zh-CN.md)              | 功能范围、源码入口、平台与测试边界                            |
| [构建、下载与分发](rust-distribution.zh-CN.md)        | Actions、独立构建、AIO 发布、Windows / Linux 解压包及本地打包 |
| [原版 YAML 配置](rust-config-compatibility.zh-CN.md)  | 路径、字段映射、迁移、保存和兼容范围                          |
| [设备操作](rust-device-operations.zh-CN.md)           | 发现、敲击分配、磁力计命令与 ACK                              |
| [SteamVR 桥接](rust-steamvr-bridge.zh-CN.md)          | 驱动、共享、头显 / 控制器、手部来源和派生速度                 |
| [SteamVR 仪表盘](rust-steamvr-dashboard.zh-CN.md)     | 面板启动、尺寸、列表、纹理与输入                              |
| [校准与 AutoBone](rust-calibration-autobone.zh-CN.md) | 复位、StayAligned、训练、PFS / PFR 录制与保存                 |
| [BVH 导出](rust-bvh-export.zh-CN.md)                  | 保存位置、格式、采样与文件生命周期                            |
| [日志收集](rust-logging.zh-CN.md)                     | 级别、路径、轮转和性能诊断                                    |
| [统一实机清单](rust-unified-hardware-test.zh-CN.md)   | 六点追踪、桌面、仪表盘、外围设备与证据记录                    |

## 架构与开发

| 文档                                                     | 内容                                              |
| -------------------------------------------------------- | ------------------------------------------------- |
| [后端架构](rust-backend-architecture.zh-CN.md)           | 数据链路、模块、接收、会话和算法边界              |
| [API 与 runtime](rust-backend-api-architecture.zh-CN.md) | 状态所有权、线程、RPC、队列、快照、保存和指标口径 |
| [前后端联调](rust-frontend-integration.zh-CN.md)         | React / Tauri 启动、能力、订阅、操作与 journal    |
| [GPUI 指南](rust-gpui-guide.zh-CN.md)                    | 前端分层、页面、草稿、引导、功能覆盖与平台集成    |
| [GPUI 组件库](rust-gpui-components.zh-CN.md)             | 独立 crate、组件接口、主题和组件预览              |
| [算法验证](rust-core-validation.zh-CN.md)                | 固定上游参考、生成工具、fixtures、容差和复现      |
| [runtime 性能](rust-udp-runtime-performance.zh-CN.md)    | 微基准、负载测试、测量环境与原始结果              |
| [发布核对](release-checklist.zh-CN.md)                   | 发行身份、构建产物、公开资料与发布验收            |

源码构建参数见 [后端 README](../server-rust/README.zh-CN.md)、[算法 README](../server-rust/README.core.zh-CN.md)、[GPUI README](../gui-gpui/README.zh-CN.md) 和 [Tauri README](../gui/README.tauri.md)。参与开发见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## 文档维护

专题指南直接描述当前实现与接口。修改行为、参数或功能范围时同步改写对应指南和受影响的代码注释。版本变更和已完成修复写入 changelog；复现所需的测试条件、指标口径、原始数据与容差保留在验证文档。具体提交的检查结果以 Actions 记录为准，真实设备结果按实机清单记录。
