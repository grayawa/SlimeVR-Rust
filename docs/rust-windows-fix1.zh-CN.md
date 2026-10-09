# Windows 连接、设备发现与诊断

Tauri 使用系统 WebView2；后端读取原版 `vrconfig.yml` / `.yaml`。Windows 包的使用方式见 [便携包说明](rust-windows-portable.zh-CN.md)。

## 连接生命周期

每个 WebSocket 连接持有并清理自己的监听器。回调根据当前连接身份与组件生命周期判断有效性，当前连接的事件更新当前状态。界面根据断开码、原因和超时信息提示连接状态。

发现广播在没有在线 sensor 时每 10 秒发送一次，启动后先等待同样的安静窗口。固件主动握手会立即进入准入与响应流程，已连接设备使用接收器的保活周期。切换后端前退出占用同一端口的服务；设备等待后仍未出现时，重启对应追踪器。

固件参考：[connection.cpp](https://github.com/SlimeVR/SlimeVR-Tracker-ESP/blob/main/src/network/connection.cpp) 的 `searchForServer()` / `update()`。定制固件按其实际超时与握手规则验收。

## 诊断与验证

GUI 记录连接、断开码、原因、正常关闭标记与错误时的连接状态。错误对象保留名称、消息、调用栈和嵌套原因。后端连接任务失败产生 `api_connection_error`，包含客户端编号、地址和原因；对应连接结束后其他客户端与接收器继续运行。

自动检查覆盖连接事件竞态、组件卸载、错误序列化、非法帧、实际 UDP / CLI 握手与传感器注册。实机排查按发生时间关联前端连接日志和后端事件，见 [日志收集](rust-logging.zh-CN.md)。
