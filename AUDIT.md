# Linux 适配与原生发布验证记录

日期：2026-10-08。独立项目来源：`Tran314/note-system` 的 `0fc4091d96512053cb491976a5476a898b1c559d`。计划版本 v4.1.0，目标仓库 `Tran314/Nebulabook`。旧仓库及 v4.0.0 不删除、不移动 tag。

## 安全与数据边界

- 本地 Rust + egui/eframe；无浏览器/WebView、账户、后端服务、遥测或更新联网。
- 版本化 JSON 校验未知字段、重复 ID、日期、引用与循环；损坏/未来版本不会重置为空库。
- 同目录临时文件同步后原子替换；保留上一份成功快照；缺失主文件但存在备份时停止并提示恢复。
- fs2 排他锁防止两个实例同时写；逐字节检测锁外修改，失败不覆盖。锁文件存在不代表锁仍被持有。
- 编辑缓冲区与持久化状态分离；保存失败保留草稿，切换与关闭受保护。RawInput 回归保留同帧文本/IME和鼠标事件顺序。
- 导入先全量验证，再复制生成新 ID；64 MiB 导入/结果库上限，单篇 HTML 8 MiB；HTML 不执行、不联网，原文和旧元数据可保留。
- 导出使用 create_new，不覆盖已有文件。Linux 手动路径要求绝对路径，不执行 shell，不展开命令/变量；可在保存失败后导出当前草稿。
- Linux 新目录 0700，新数据/锁/备份/导出/诊断文件 0600；已有目录权限不修改。Windows 继承用户目录 ACL。
- Fontconfig 最长 1.5 秒、输出最多 16 KiB，字体最多 64 MiB，只读取系统普通字体文件；中文语言和 TTC face index 校验。导入拒绝目录/设备/FIFO等非常规文件。
- 安装脚本仅写当前 HOME 内的可执行文件、链接和菜单入口；校验包内 SHA256，不读取/移动笔记数据。

这不是对恶意本机并发进程、所有网络文件系统或掉电情况的完整防护。笔记未加密，上一份快照不能替代独立备份。

## v4.1.0 本地验证

助手 Linux x86_64 / Debian 13、Rust 1.99.0；没有访问用户电脑或真实笔记。

已通过：

- 75 个完整 Rust 测试：61 库单元 + 7 启动/平台配置 + 5 Linux XDG/权限/更名兼容 + 2 工作流集成
- 41 个无 desktop 数据层测试
- Linux 全目标 Clippy（warnings denied）与 rustfmt
- Windows x64、x86、ARM64 全目标交叉 Clippy（源码/测试检查，不是 EXE 运行）
- 系统 Noto Sans CJK 实际中文 glyph 检查，等宽/比例字体均通过
- 4 个旧 IndexedDB 只读导出脚本 Node 测试
- Linux x64 release 原生 ELF 构建
- 16 项 Linux 打包/安装回归（C 测试程序及明确标记的模拟 smoke 证据，不是 GUI 验收）
- 15 项独立离线发布校验正反例：旧 SHA、缺少/伪配 GUI 证据、错误 ELF/PE、glibc 超标、额外 tar 路径、混入资产及 checksum 篡改均拒绝；使用合成平台头和证据，不代表目标平台执行
- Actionlint、Python 脚本语法和 Shell 语法检查

Linux GUI 未在本地通过：本环境创建 AF_UNIX socket 返回 EPERM，官方 Xvfb 无法建立显示服务器监听套接字。因此不将无头 egui 测试或成功编译标记为窗口运行通过。新仓库 CI 需要在正常 Linux runner 上完成真实 X11/Wayland 检查后，才能生成可发布 Linux 包。

不满足 glibc 2.35 基线或缺少 GUI 证据的本机包只允许 `local-only` 命名且标记不可发布。不得把这些开发包当作正式 Linux Release。

## Linux 与 Windows 发行门禁

CI 计划为 Ubuntu 22.04 x64 / ARM64 原生、Windows 2025 x64 / x86 WOW64、Windows 11 ARM64 原生。

Linux：

1. 格式、全部测试、Clippy、release 链接。
2. Xvfb/X11 真实窗口键盘编辑、保存、权限、关闭、重开和保存冲突阻止退出。
3. Weston/Wayland 真实渲染器/首帧中文字体初始化与进程存活。
4. 实际测试二进制 SHA256 必须与打包二进制相同。
5. ELF64 machine、原生主机、动态解释器、glibc 直接符号最高版本不超过 2.35；tar 文件集合、可执行权限、内外 SHA256 核验。

Windows：原生 renderer 初始化/存活；x64/x86/ARM64 的 PE machine、目标/主机、大小、SHA256 核验。

发布仅允许 `Tran314/Nebulabook` 的 main、v4.1.0 显式发布标记或手动输入，且全部 5 个矩阵成功。main SHA 必须仍为本次 commit。只有 HTTP 404 视为 tag/release 缺失；不同既有 tag、draft 或校验差异均停止，不覆盖。发布后重新下载资产校验。

配置存在不等于已经运行：以每个来源 commit 的 Actions 与 Release 为准。在新仓库完成 CI 前，不宣称 v4.1.0 发布或 Linux GUI 已通过。

## 已验证的旧版本历史

原仓库 v4.0.0 的源码为 `4ff7a82cb9e3eb64738fb5735fb08756da30173d`，Windows 三架构真实 CI 已成功：[发布运行](https://github.com/Tran314/note-system/actions/runs/37803464788)。三个 EXE 已下载核验 PE、SHA256 和来源元数据。随后 `0fc4091d…` 仅补 Windows 运行库 README，CI 成功且未重新发布。这些事实不能替代 v4.1.0 新提交的验证。

## 已知限制

- GUI 虚拟桌面软件渲染不等于所有真实 GPU、DPI、窗口管理器、输入法或 portal 的认证。
- rfd 系统文件选择器为同步调用，第三方 portal 卡住可能阻塞 UI；手动路径入口可预先绕过 portal，但不能中断已经挂起的系统调用。
- Markdown 为文本；无旧网页富文本/云同步/服务端账号/完整文件夹标签管理。
- JSON 整库保存接近 64 MiB 时可能造成 UI 停顿，不适用于大型知识库。
- 所有二进制暂未代码签名；Linux 动态库与字体由系统供应方提供。
- 旧浏览器迁移只做 mock 事务测试，未对用户真实浏览器执行。迁移后需要核对数量和内容。

## 依赖记录

依赖锁定沿用已审查的原生 4.0.0，仅应用版本变为 4.1.0；本次未新增 crate。此前同日 RustSec 数据库扫描结果为 0 个已知漏洞，2 个维护状态警告：`paste 1.0.15`（当前功能无可达使用）与 `ttf-parser 0.25.1`（字体依赖链）。参见 [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html)、[RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html)。应持续跟踪 GUI/字体依赖升级；此历史扫描不保证未来无漏洞，也不是渗透测试或形式化证明。

## Nebulabook 更名回归

包/bin、UI、仓库和发行文件均统一为 Nebulabook / `nebulabook`；数据身份和旧浏览器协议不变。新增回归实际写入旧数据目录，再由更名后的库在独立子进程打开，核验原有笔记与文件字节未变且不创建新的空白数据目录。诊断配置采用 `NEBULABOOK_` 前缀，同时保留旧 `NEBULA_` 显式配置后备；新名称优先。安装回归确保旧程序、菜单、launcher及真实旧笔记目录均保持不变。

## 真实 Linux CI 发现与修复

首次 ARM64 窗口启动发现 runner 缺少 `libxkbcommon-x11.so.0`，已补入 CI 运行库；README 的用户安装依赖已有该包。随后 [7f6f247 的真实 CI](https://github.com/Tran314/Nebulabook/actions/runs/37811469265) 在 Linux x64/ARM64 均成功初始化 Glow 和中文字形，但严格键盘保存验收发现标题 Tab 导航又被正文当成字符，导致正文多出前导制表符。没有放宽保存断言或发布失败产物。

已用实际 egui RawInput 在分阶段、整批和逐字符时序复现并修正：只消费从标题进入正文的第一枚导航 Tab，正文已有焦点时的 Tab/ShiftTab 缩进和点击后 Tab 仍保留。4项新增回归全部通过，原输入排序与关闭排队机制不变。后续真实 CI 继续检查完整编辑/保存/关闭/重开以及 Wayland。烟测失败时保留本次隔离测试数据和截图，不读取用户笔记。
