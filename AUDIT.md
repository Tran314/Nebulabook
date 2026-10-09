# Linux 适配与原生发布验证记录

日期：2026-10-08。独立项目来源：`Tran314/note-system` 的 `0fc4091d96512053cb491976a5476a898b1c559d`。计划版本 v4.1.0，目标仓库 `Tran314/Nebulabook`。旧仓库及 v4.0.0 不删除、不移动 tag。

下文保留各阶段验证历史；旧 JSON 迁移、浏览器导出、旧环境变量与相关测试记录描述的是当时实现，不是当前能力。当前 main 的 `.nebula` 专用边界及兼容代码清理见文末；已有 v4.1.0 Release 未被本次修改或重新发布。

## 安全与数据边界

- 本地 Rust + egui/eframe；无浏览器/WebView、账户、后端服务、遥测或更新联网。
- `.nebula` 认证封装及内部版本化模型校验未知字段、重复 ID、日期、引用与循环；损坏/未来版本不会重置为空库。
- 同目录临时文件同步后原子替换；保留上一份成功快照；缺失主文件但存在备份时停止并提示恢复。
- fs2 排他锁防止两个实例同时写；逐字节检测锁外修改，失败不覆盖。锁文件存在不代表锁仍被持有。
- 编辑缓冲区与持久化状态分离；保存失败保留草稿，切换与关闭受保护。RawInput 回归保留同帧文本/IME和鼠标事件顺序。
- 支持 `.nebula` / TXT / Markdown / HTML 导入，先全量验证，再复制生成新 ID；文本/结果载荷上限 64 MiB，`.nebula` 文件允许额外 60 字节封装，单篇 HTML 8 MiB；HTML 不执行、不联网，原文与已有 `.nebula` 元数据保留。
- 导出使用 create_new，不覆盖已有文件。Linux 手动路径要求绝对路径，不执行 shell，不展开命令/变量；可在保存失败后导出当前草稿。
- Linux 新目录 0700，新数据/锁/备份/导出/诊断文件 0600；已有目录权限不修改。Windows 继承用户目录 ACL。
- Fontconfig 最长 1.5 秒、输出最多 16 KiB，字体最多 64 MiB，只读取系统普通字体文件；中文语言和 TTC face index 校验。导入拒绝目录/设备/FIFO等非常规文件。
- 安装脚本仅写当前 HOME 内的可执行文件、链接和菜单入口；校验包内 SHA256，不读取/移动笔记数据。

这不是对恶意本机并发进程、所有网络文件系统或掉电情况的完整防护。当前 main 的公开密钥 `.nebula` 封装不提供可靠保密；TXT / Markdown 导出以及任何仍留在磁盘上的旧 JSON 都是明文。上一份快照不能替代独立备份。

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
- `.nebula` 整库序列化、封装与保存接近 64 MiB 时可能造成 UI 停顿，不适用于大型知识库。
- 所有二进制暂未代码签名；Linux 动态库与字体由系统供应方提供。
- 旧原生 JSON 和浏览器数据不再直接导入或自动迁移。仅有旧 JSON 时会打开新库，旧文件不受影响；已有 `.nebula` 和其恢复快照仍受保护。

## 依赖记录

原生 v4.1.0 基线的依赖锁定沿用已审查的原生 4.0.0，当时仅应用版本变为 4.1.0，未新增 crate。后续依赖升级另见下节。此前同日 RustSec 数据库扫描结果为 0 个已知漏洞，2 个维护状态警告：`paste 1.0.15`（当前功能无可达使用）与 `ttf-parser 0.25.1`（字体依赖链）。参见 [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html)、[RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html)。应持续跟踪 GUI/字体依赖升级；此历史扫描不保证未来无漏洞，也不是渗透测试或形式化证明。

## Nebulabook 更名回归（历史）

包/bin、UI、仓库和发行文件均统一为 Nebulabook / `nebulabook`；数据身份和旧浏览器协议不变。新增回归实际写入旧数据目录，再由更名后的库在独立子进程打开，核验原有笔记与文件字节未变且不创建新的空白数据目录。诊断配置采用 `NEBULABOOK_` 前缀，同时保留旧 `NEBULA_` 显式配置后备；新名称优先。安装回归确保旧程序、菜单、launcher及真实旧笔记目录均保持不变。

## 真实 Linux CI 发现与修复

首次 ARM64 窗口启动发现 runner 缺少 `libxkbcommon-x11.so.0`，已补入 CI 运行库；README 的用户安装依赖已有该包。随后 [7f6f247 的真实 CI](https://github.com/Tran314/Nebulabook/actions/runs/37811469265) 在 Linux x64/ARM64 均成功初始化 Glow 和中文字形，但严格键盘保存验收发现标题 Tab 导航又被正文当成字符，导致正文多出前导制表符。没有放宽保存断言或发布失败产物。

已用实际 egui RawInput 在分阶段、整批和逐字符时序复现并修正：只消费从标题进入正文的第一枚导航 Tab，正文已有焦点时的 Tab/ShiftTab 缩进和点击后 Tab 仍保留。4项新增回归全部通过，原输入排序与关闭排队机制不变。后续真实 CI 继续检查完整编辑/保存/关闭/重开以及 Wayland。烟测失败时保留本次隔离测试数据和截图，不读取用户笔记。

## eframe / wgpu 联动升级验证

本次依赖 PR 以 `9e56b7869cc28db6f9c34108ff138cef18434159` 为源码基线，联动使用 eframe/egui 0.36.2 和 wgpu 30.0.1；保留 Linux Glow/X11/Wayland、Windows DX12/FXC 和默认硬件优先/WARP 后备策略。构建 MSRV 提高至 Rust 1.95，来源为 [egui 0.36.2 的工作区配置](https://github.com/emilk/egui/blob/0.36.2/Cargo.toml)。Windows 的直接 wgpu 与 eframe 重导出类型设编译期回归检查，锁文件使 gpu-allocator 0.28 和 wgpu-hal 30.0.1 统一使用 windows 0.62.2。Dependabot 将 eframe/wgpu 归入同一更新组。

API 迁移涵盖 App::logic/ui、Panel、可变字体访问、菜单关闭及无窗口测试纹理回收。输入排序保留原文本/IME/保存/关闭断言；正文新获焦点后先完成一轮无新输入的 egui pass，再按原顺序交付待处理事件，避免首个 Tab 被当成焦点导航。隐藏窗口的原始事件交由 eframe 完整保留；隐藏关闭先取消并恢复窗口，等真实 UI 处理完排队输入后再保存/确认关闭。存储格式、持久化身份和保存失败保护保持不变。

Windows 图形验证与兼容边界：WGPU 30 的 DX12 适配器要求资源绑定 Tier 2，较旧 GPU 可能回退到 WARP，软件渲染可能降低性能。微软文档明确 Windows 10 1709 起的 WARP 支持 Feature Level 12_0 / 12_1，对应至少 Tier 2；更早 Windows 10 与旧 GPU 组合尚未验证。来源：[WGPU 适配器源码](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/dx12/adapter.rs)、[WARP](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp)、[Feature Level](https://learn.microsoft.com/en-us/windows/win32/direct3d12/hardware-feature-levels)。CI 的 Windows 2025 / Windows 11 ARM64 结果不代表全部旧系统或实体 GPU；Linux 的 glibc 2.35、真实 X11/Wayland、二进制 SHA256 绑定和全部发布门禁继续保留。

本地验证环境：助手 Debian 13 / x86_64、Rust 1.95.0，未访问用户电脑或真实笔记。

- 全部 80 项 Rust 测试通过：66 库单元、7 启动/平台配置、5 Linux 存储、2 工作流集成；另有 41 项无 desktop 测试通过。
- 新增隐藏窗口回归覆盖最小化/遮挡时关闭、自动保存期限前关闭、保存失败/外部修改冲突、排队输入在多次隐藏 tick 中保持顺序、恢复后最终关闭及明确放弃。保留全部原有文字/IME/点击/Tab/关闭断言。
- Linux x64 本机与 Linux ARM64、Windows x64/x86/ARM64 交叉全目标 Clippy（warnings denied）、rustfmt、必需系统 CJK 字形检查通过；交叉检查不代表目标二进制运行。
- Linux x64 release 编译通过。本机开发二进制直接符号需要 GLIBC 2.39，不能作为正式 glibc 2.35 发行包；Ubuntu 22.04 CI 的原生编译、ABI 和打包门禁继续强制执行。
- 4 项 Node 导出测试、16 项 Linux 打包/安装回归、15 项离线发行校验用例以及 Python/Shell/YAML 语法检查通过；合成平台头/GUI 证据仅验证脚本行为。
- 本环境没有 Xvfb/Weston，未运行本次升级的真实窗口烟测；尚未在本地运行 Windows EXE 或 Linux ARM64 程序。真实 X11/Wayland、Windows 三架构启动和正式产物证据必须以此次提交的完整 CI 为准。

原版本的依赖扫描和 GUI 记录不能替代此次升级的验证；此次未重新执行 RustSec 数据库扫描。

### Windows x86 上游 DX12 ABI 临时补丁

真实 x86 启动在创建 `egui_pipeline` 时返回 `0x80070057`，而同源码的 Windows x64/ARM64 和 Linux 两架构已通过。已定位 wgpu-hal 30.0.1 把流式管线子对象固定按 8 字节对齐；D3D12 ABI 在 x86 要求 4 字节指针对齐。使用官方 crate 的本地副本，仅将此处改为指针对齐并加入实际序列化回归，保留原许可证、来源校验和及精确变更说明：[vendor/PATCHES.md](vendor/PATCHES.md)。该补丁尚非上游正式修复，需要随依赖更新审阅；只有原生 x86 启动成功才可确认修复有效，不以交叉编译代替。

为在三种 Windows 架构执行上游模块的私有回归测试，vendored crate 被纳入工作区，默认成员仍只有应用；应用与回归共用根 Cargo.lock，vendor 内原发布锁文件只作来源记录。未增加 RustSec 忽略规则。对更新后的锁文件使用本地 1295 条 RustSec 公告快照作离线检查，未发现已知漏洞，有 `ttf-parser 0.25.1` 未维护警告；该快照未在本次刷新，不能视为最新漏洞数据库扫描或对本地源码补丁的安全认证。

应用 Clippy 明确选择 `-p nebulabook --no-deps --all-targets -- -D warnings`，保持原先对全部应用目标的严格检查范围；不因上游依赖成为工作区成员而扩展成上游第三方源码风格门禁。vendored crate 仍经过编译、三个 Windows 架构的原生 ABI 回归、真实启动及独立源码审查，未放宽运行时或发布检查。

## 2026-10-09 磨砂界面与 .nebula 初次更新（清理前记录）

此轮只更新 main，不增加 tag、不重发或覆盖已有 Release。应用仍是离线原生 Rust / egui；`vendor/`、渲染器选择和原五平台发布门禁未改。

视觉：应用内缓存柔焦色场、半透明侧栏/工具栏、圆角纸面和系统浅深色。顶部切换笔记/回收站；保留原生窗口边框/控制。背景纹理为 160×160 RGBA（102,400 字节像素），仅初始化/换主题时生成，没有桌面读取、实时背景模糊或持续动画。文字和恢复菜单优先可读性；回收站只禁用编辑，长正文仍可滚动。

存储：默认 `.nebula` 使用公开兼容密钥的 XChaCha20-Poly1305 认证封装，仅提供免密码轻量混淆和直接误改检测，不是可靠保密或对有意修改者的签名。原 JSON/备份保留原始字节并仍为明文。独立复核覆盖迁移原子发布、旧/新共享锁、迁移中断、新格式损坏不回退、旧源修改冲突、严格格式/大小/结构验证；没有遗留实施阻断。精确协议、权限及硬链接文件系统限制见 [NEBULA_FORMAT.md](NEBULA_FORMAT.md)。

本地验证：助手 Linux x64、Rust 1.95.0，没有访问用户电脑或真实笔记。

- 103 项 Rust 测试通过：74 库单元、7 启动配置、5 Linux 存储、2工作流、15迁移集成；其中31项应用输入/状态回归保留或加强精确断言。可选截图测试另行执行，不混入这些数量。
- 60 项无 desktop 测试、应用全目标 Clippy（`-D warnings`）与 rustfmt 通过。Linux x64 release 构建通过，本机开发二进制为11,655,184字节；正式包仍以 Ubuntu 22.04 CI 构建及ABI核验为准，不把本机开发产物发布。
- Linux ARM64 与 Windows x64/x86/ARM64 的应用全目标交叉 Clippy（`-D warnings`）通过；这是源码/类型检查，不是这些目标的本机执行。
- 16 种实际 egui / egui_glow / Mesa EGL surfaceless 渲染通过并人工查看：浅深色、640×420、2× DPI、长标题、空库、搜索无结果、回收站及其长文滚动、窄窗保存错误、退出保护、导出菜单、手动路径对话框。它们不创建操作系统窗口，不代替原生窗口生命周期验收。本轮截图只含合成中文笔记。
- 11 项独立 Python fixture/密码算法已知向量/认证读取测试、4项旧 IndexedDB 导出测试、16项打包安装测试、15项离线发布校验用例通过。后两组中的模拟平台头/GUI 证据只验证脚本，不代表真实平台执行。
- 更新 RustSec 数据库至 `550efd3d587a29b2e2c2b21b17a440da4fede999`（数据库日期2026-10-08），扫描当前465个锁定依赖：0个已知漏洞，仍有原有 `ttf-parser 0.25.1` 未维护提醒；没有新增忽略规则。这不等于安全认证。
- 单篇合成正文的数据封装/认证解码微测（release，5次均值，不含磁盘 I/O）：1 KiB 为0.015/0.027 ms，1 MiB为3.206/3.036 ms，10 MiB为38.363/27.873 ms；不推广为其他CPU、最大库或每帧性能承诺。

真实跨平台验收仍由本提交的完整 Actions 结论决定。Linux X11 烟测使用真实键鼠，认证解码核对精确标题/正文，检查原 JSON 未变、回收站恢复、640×420和2×原生窗口尺寸；13场景截图及其SHA256清单仅来自隔离测试目录。Wayland保持真实启动/字体检查，Windows保持三架构原生启动和 x86 ABI 测试。未验证所有物理GPU、输入法、desktop portal、混合DPI多显示器或文件系统组合。

## 当前 main：移除旧兼容流程

本轮仅清理当前源码，不改应用版本、已有 tag 或 Release。上面的测试数量、源码/二进制大小与运行结果是对应阶段的历史记录，不作为本轮清理后的验证结论。最终验证须针对清理后的提交重新运行；这里不预先宣称通过。

当前行为：

- 本机存储只接受 `.nebula`，移除旧原生 JSON 自动迁移、源哈希关联与降级写入检查。旧 JSON / `.json.bak` 不读、不改、不删；仅有这些旧文件时打开新的空白库。
- 移除 JSON / Dexie 导入、明文 JSON 导出、浏览器导出脚本及其 Node 测试。保留 `.nebula` / TXT / Markdown / HTML 导入、完整 `.nebula` 备份和 TXT / Markdown 正文导出。Node 不再是开发检查前置依赖。
- 只接受 `NEBULABOOK_` 环境配置；旧 `NEBULA_` 别名不再生效。
- 保持 `.nebula` v1 封装、Notebook schema、现有数据目录与 `notebook.json.lock` 实例锁。现有 `.nebula` 的文件夹、标签、原始 HTML 和旧元数据作为非执行数据保留。`legacy_source_sha256` 保留有效字段形状，读取后不参与操作，新编码写 null。
- 保持原子保存、上一份快照、严格格式/结构/大小校验、外部变更保护和失败草稿导出。主文件缺失但存在 `.nebula` 备份或此前生成的迁移快照仍阻止空白初始化；这些快照是应受保护的当前格式数据。
- 原生平台依赖、Windows x86 ABI 补丁、第三方许可与来源记录仍保留；删去旧业务流程不等于删去当前平台所需组件。

需要重点复核：有效 `.nebula` 含历史字段时的打开/保存/再导入、忽略旧 JSON 后仍逐字节保护当前主文件、稳定锁的互斥、备份恢复、JSON 拒绝导入不改变当前笔记库，以及五平台原有构建/启动门禁。精确协议和文件系统限制见 [NEBULA_FORMAT.md](NEBULA_FORMAT.md)。


本轮本地验证（Linux x64，Rust 1.95.0，隔离合成数据）：

- 102 项 Rust 测试通过（78 库单元、7 启动配置、5 Linux 存储、10 当前格式存储、2 工作流）；另有 59 项无 desktop 测试通过。删去旧迁移行为测试，改为当前格式与旧文件不受影响的回归，并加强认证载荷、集合上限、事务回滚及历史字段保留的断言。
- 全应用目标 Clippy `-D warnings`、rustfmt、release 构建通过。16 个实际 egui/Glow/Mesa EGL 离屏场景重新渲染通过；它们不替代真实窗口验收。
- 13 项独立 Python 格式/密码向量/fixture 测试、16 项打包安装和 15 项离线发布校验用例通过；actionlint、Python 编译、shell 语法和 diff 空白检查通过。后两组模拟打包证据只验证脚本，不代表真实平台执行。
- 当前本地环境未运行 X11/Wayland 原生窗口流程。CI 已改用独立认证 `.nebula` 视觉输入，保留从空目录启动后的真实键鼠编辑、保存、精确内容认证读取、关闭重开、回收站恢复、640×420、2× DPI 和失败关闭保护；五平台最终结论须查看本提交的 Actions。发布门禁与已有 Release 保持不变。
