# 发布核对与维护

本清单覆盖发行身份、公开资料、构建产物和平台验收。历史公开准备核对记录见 [CHANGELOG](../CHANGELOG.md)。

## 发行身份与状态

使用 `SlimeVR-Rust` 名称，窗口、托盘和界面标明独立开发预览。Tauri 产品名为 `SlimeVR-Rust`，安装应用标识为 `io.github.grayawa.slimevr-rust`；GUI 偏好和后端配置沿用 `dev.slimevr.SlimeVR` 目录。

README 提供 AI 使用声明、开发预览说明、配置备份、反馈方式及验收入口。发布版本使用开发预览或 GitHub prerelease 标识，记录构建提交及该版本的实机测试结果。商标与标识使用遵循 [TRADEMARK.md](../TRADEMARK.md)。

## 源码与公开资料

每次新增公开资料时核对以下项目：

- [ ] 扫描准备公开的 Git 历史、讨论文本和工作流日志，人工核对密钥规则候选项。
- [ ] 核对 artifacts、截图、外部附件及问题报告中的个人信息，处理网络凭据、设备身份和私人路径。
- [ ] 核对克隆、递归子模块、锁文件、构建与测试入口。

## 二进制发布

- [ ] 固定构建 commit、子模块、锁文件和工具版本；本地修改随对应源码提供。
- [ ] 按根目录 [LICENSING.md](../LICENSING.md) 和 [第三方声明](../THIRD_PARTY_NOTICES.md) 核对随包授权文件与对应源码。
- [ ] 检查 `SOURCE-CODE.txt`、`BUILD-MANIFEST.json` 或 `BUILD-SOURCE.json` 中的版本与源码一致性。
- [ ] 核对安装器中的声明目录及安装包 artifact 附带的许可文件。
- [ ] 检查文件哈希、归档完整性、架构、运行库和平台依赖。
- [ ] 按 [实机清单](rust-unified-hardware-test.zh-CN.md) 记录“通过 / 失败 / 未测”，附上系统、设备、配置和日志时间。
- [ ] 在 Release 长期保存二进制、校验值及完整对应源码获取与构建说明。

GPUI、Tauri、Overlay 与后端打包流程携带许可和源码通知。Tauri 开发 / 构建钩子准备声明资源，安装资源映射携带声明目录。发布者核对实际产物及其对应源码。

源码需要覆盖实际构建输入、本地修改和子模块版本。GitHub 自动生成的源码压缩包配合递归子模块说明使用。Actions artifacts 默认保留 30 天；长期下载使用 Releases。构建与打包入口见 [分发指南](rust-distribution.zh-CN.md)。

## GitHub 管理

按协作范围在仓库网页核对：

- `Settings → Code security`：secret scanning、push protection、private vulnerability reporting，报告方式见 [SECURITY.md](../SECURITY.md)。
- `main` 的 ruleset / 分支保护：PR 与相关 CI 检查要求。
- `Settings → Actions`：默认 token 权限与外部贡献者工作流审批。当前工作流使用显式 `contents: read`，PR 检查使用 `pull_request`。
- 仓库简介、topics 与 Issue 模板：版本、复现步骤和脱敏日志字段。
