# 已有 SteamVR 驱动的处理

启动检查发现 SteamVR 安装目录中的 `drivers/slimevr` 时，保留现有安装和注册，发送普通信息通知。Tauri / React 通过 Fluent 显示：

> 使用已有的 SlimeVR 驱动
>
> 检测到已有的 SlimeVR 驱动，已保留现有安装并跳过自动注册。

同一次界面启动中显示一次。简体中文和英文使用对应翻译，其他语言按现有 Fluent 回退机制显示。GPUI 将驱动状态提示放在 SteamVR 设置页。注册工具执行失败时使用错误通知。

自动检查通过真实后端与 WebSocket 模拟已有安装，核对文件内容、普通通知、连接恢复与请求响应；翻译检查覆盖中文和英文回退。真实驱动状态按 [SteamVR 测试清单](rust-unified-hardware-test.zh-CN.md#steamvr) 验收。

驱动路径、注册与启用方式见 [日常流程](rust-daily-workflow.zh-CN.md#steamvr-驱动管理)。
