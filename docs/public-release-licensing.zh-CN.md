# 公开准备、许可与分发核对

核对日期：2026-10-10。源码基线：`9aee2878`。项目新增贡献采用 `GPL-3.0-or-later`；完整条款和例外以
[LICENSING.md](../LICENSING.md) 为准。上游基线为
`83941fd38e91cc91ca6b360deab5c2ae986dd1b6`。

## 本轮核对范围

- Rust 后端、GPUI、Tauri 的 `cargo metadata --locked` 分别解析了 317、950、
  514 个包，数量覆盖多平台与开发依赖；实际链接集合按目标和 feature 确定。
  已检查声明的许可证表达式。主要是 MIT、Apache-2.0、BSD、ISC 等；少量
  MPL-2.0 包需要保留各自源码及通知义务。声明中的可选 GPL/LGPL 分支有
  MIT 或 Apache 等兼容选项。这些已核对声明提供 GPLv3 兼容的授权路径。
- 检查了本地 pnpm 缓存中的许可声明；构建工具、字体、浏览器兼容性数据
  与最终程序代码分开判断。FlatBuffers 是 Apache-2.0，字体是 OFL/UFL，
  Twemoji 图像是 CC-BY-4.0，npm 包装器代码使用 MIT。
- 子模块及 vendor 许可不变，原作者声明和数学参考许可保留。GPL 正文覆盖项目新增代码，历史 MIT / Apache 授权与第三方条款按原范围保留。
- 跟踪文件检查覆盖用户日志、vrconfig、证书私钥与上传压缩包，这些类别的检查结果为零。跟踪的
  `gui/.env` 只有三个固件服务 URL 配置变量。
- Gitleaks 8.30.1 使用默认规则扫描了全部本地可达 Git 历史：2,496 个提交、
  约 62.90MB 文本。两处 `generic-api-key` 匹配均为上游 Java 代码中的
  `Advapi32.INSTANCE`，扫描规则识别的候选项均已核对为误报。
- 扫描了 GitHub API 返回的 15 个 PR 说明，规则匹配数为零；Issue 评论和
  PR 行内评论的返回数量均为零。扫描覆盖可访问的文本与本地 Git 引用。
  Actions 日志、压缩产物、图片及外部附件列入公开前人工核对范围。
- 文档整理 PR #15 已合并，React、打包工具及 Linux / Windows Rust 检查全部通过。
  当前仓库仍为私有，GitHub Releases 列表为空。

## 当前许可与分发文件

- 项目根目录提供 GPL 正文、许可范围、版权通知和第三方索引；Cargo 和
  npm 的项目元数据统一为 `GPL-3.0-or-later`。
- README 与贡献规则区分本项目 GPL 新增代码、继承的 MIT/Apache 部分和
  独立许可的第三方资源。vendor 的已有 Apache 许可继续保留。
- GPUI、Tauri 解压包、Overlay 与后端解压包附带项目许可证、第三方资源通知及
  `SOURCE-CODE.txt`，记录实际构建 commit 和递归子模块 / 锁文件 / 构建入口。
  本地非 commit 标签标记其版本信息范围；分发者提供实际源码和本地修改。
- 第三方索引覆盖主要组件与嵌入资源，每个平台的实际传递依赖按构建目标另行核对。发布二进制时仍须逐项满足实际依赖的通知及
  对应源码要求。

## 源码公开前

README 使用“开发预览阶段”说明项目状态，给出配置备份、反馈方式和实机
验收入口。功能清单分别记录实现与验收范围，使用者按对应版本判断适用场景。

- [x] 提供 GPL 正文、上游许可、版权通知、第三方声明和贡献许可规则。
- [x] 扫描本地可达 Git 历史及可访问的 PR 说明文字，核对扫描结果。
- [x] 提供克隆、子模块、构建、开发状态和测试范围说明。
- [ ] 人工复核准备保留的 Actions 日志、artifacts、截图及附件，遮蔽个人信息。
- [ ] 确定公开展示名称与衍生项目标识，按 [上游商标规则](../TRADEMARK.md)
      核对 README、GUI、图标及下载页面。

上游商标规则允许事实性说明兼容性与来源，发行名称需要清楚体现独立维护
身份。当前 Tauri 的 `productName`、窗口标题为 `SlimeVR`，应用标识为
`dev.slimevr.SlimeVR`，Cargo 作者字段为 `SlimeVR Team`。这些发行元数据和
继承的标识需要在面向用户发布前核对；原有版权声明继续保留。

## 二进制公开发布前

- [ ] 为每个发布包固定构建 commit，记录实机验收结果，使用开发预览或
      GitHub prerelease 标识，并提供对应源码与构建说明。
- [ ] 核对各包的项目许可、实际依赖通知、字体及图像许可和源码引用。
- [ ] 为 Tauri 安装包补齐 GPL 正文、许可范围、版权通知、第三方声明及
      对应源码引用。当前 `gui/src-tauri/tauri.conf.json` 的安装资源仅列入
      项目原有 MIT / Apache 许可、后端数学许可、驱动和 OpenVR helper；
      解压包的声明复制由独立打包脚本负责。

发布包提供的源码应覆盖实际构建输入、本地修改和子模块版本。GitHub 自动
生成的源码压缩包需要结合子模块获取说明使用。Actions artifacts 的默认
保留期为 30 天；面向长期下载的版本通过 Releases 保存二进制、校验值及
源码获取说明。

## GitHub 管理设置

以下设置属于维护建议，按项目开放协作的范围选择：

- 在 `Settings → Code security` 配置 secret scanning、push protection
  及 private vulnerability reporting，并为选定的私密报告渠道编写
  `SECURITY.md`。
- 为 `main` 配置 ruleset 或分支保护，要求 PR 与相关 CI 检查通过。
- 在 `Settings → Actions` 核对默认 token 权限与外部贡献者工作流审批。
  当前工作流文件显式设置 `contents: read`，PR 检查使用 `pull_request`。
- 补充仓库简介与 topics；Issue 模板引导填写版本、复现步骤和脱敏日志。

本轮 GitHub 授权对 Actions 权限设置和分支保护查询返回 HTTP 403，管理
设置的实际状态由维护者在仓库网页核对。

## 公开后的许可效果

GPL 允许商业使用、分发与修改。分发 GPL 覆盖的二进制时，需要按许可证
提供对应源码；私下使用与修改可以保留在私有环境。上游宽松许可证
仍允许别人使用原有代码；此前的宽松授权继续按其条款有效。

SlimeVR 商标许可独立于代码许可，本项目由独立开发者维护，发行说明明确这一身份。GPL 专属修改回馈 MIT/Apache 上游，需要
有关权利人另行授权。公开仓库会公开其可达历史及原有讨论、工作流记录；
未来排障附件应遮蔽个人配置、Wi-Fi 密码和真实日志中的私人信息。
