# Nebulabook · Rust 原生记事本

轻量、离线、本地保存的桌面记事本。Rust + egui/eframe 创建真正的原生窗口，不需要浏览器、WebView、Electron、账号或后端。

当前 main 包含尚未发布的磨砂界面与 `.nebula` 格式更新；已有 v4.1.0 Release 保持原有内容。试用本轮改动请使用对应成功 [CI](https://github.com/Tran314/Nebulabook/actions) 的构建产物或从源码构建。

本仓库是 [Tran314/note-system](https://github.com/Tran314/note-system) 原生重构版本的独立后续项目，从原仓库 `0fc4091d96512053cb491976a5476a898b1c559d` 分出。旧仓库及其版本历史保留，原 MIT 许可保持不变；这里不包含已移除的前后端代码。

## 功能

- 中文界面，标题/正文编辑、全文搜索、置顶、回收站与恢复
- 800 ms 自动保存，Ctrl+S 保存、Ctrl+N 新建、Ctrl+F 搜索
- NEBULA、TXT、Markdown、HTML、旧版 JSON 和原生 JSON 导入
- 默认完整 .nebula 备份；TXT / Markdown / JSON 可显式明文导出；导出不会覆盖已有文件
- 同目录原子保存、上一份快照备份、多实例排他文件锁
- macOS 风格的磨砂分栏、圆角纸面与清晰排版，跟随系统浅深色；顶部切换笔记/回收站。这是应用内玻璃质感绘制，不读取桌面或调用系统背景模糊，详见 [视觉设计](VISUAL_DESIGN.md)
- 保存失败保留草稿，关闭前提示重试、导出或明确放弃

Markdown 按纯文本编辑。HTML 转为文本且保留原始 HTML 供追溯，不执行脚本。旧文件夹、标签、设置可作为迁移元数据保留，暂不提供完整管理界面。没有云同步、远程附件服务、富文本网页编辑器或自动更新。

## 下载和选择架构

从 [Releases](https://github.com/Tran314/Nebulabook/releases) 下载。每次发布附带 `SHA256SUMS.txt` 和 `build-info-<平台>-<架构>.json`，可检查源码 commit、编译器、目标架构、文件大小与 SHA256。

| 系统 | 文件 | 适用设备 |
| --- | --- | --- |
| Linux x86_64 | `nebulabook-v4.1.0-linux-x64.tar.gz` | Intel/AMD 64 位 Linux |
| Linux aarch64 | `nebulabook-v4.1.0-linux-arm64.tar.gz` | ARM64 Linux，例如运行 64 位桌面系统的 ARM 设备 |
| Windows x64 | `nebulabook-v4.1.0-windows-x64.exe` | Intel/AMD 64 位 Windows |
| Windows x86 | `nebulabook-v4.1.0-windows-x86.exe` | 32 位 Windows，或 64 位 Windows 的 WOW64 |
| Windows ARM64 | `nebulabook-v4.1.0-windows-arm64.exe` | ARM64 Windows 原生程序 |

Linux 使用 `uname -m` 查看架构：`x86_64` 选 x64，`aarch64` 选 arm64。ARM32/armv7、musl/Alpine 及 macOS 暂无已验证的发行包。CPU 架构相同不代表所有发行版都兼容。

### Linux 安装与运行

发行包使用 Ubuntu 22.04 构建，要求 glibc 2.35 或更新的 GNU/Linux、可用的 X11 或 Wayland 桌面会话，以及 OpenGL 2.0 / OpenGL ES 2.0 兼容驱动。包内不捆绑图形库和中文字体，不是通用静态二进制。Ubuntu 22.04 是构建/CI 验证基线；其他发行版需满足依赖，不能保证所有显卡/窗口管理器组合。

Debian / Ubuntu 常见运行依赖（已有完整桌面通常已安装大部分）：

```sh
sudo apt update
sudo apt install libxkbcommon0 libxkbcommon-x11-0 libgl1 libegl1 libgl1-mesa-dri \
  libwayland-client0 libxcursor1 libxi6 libxrandr2 fonts-noto-cjk \
  xdg-desktop-portal xdg-desktop-portal-gtk
```

GNOME/KDE 应使用与桌面匹配的 portal 后端；KDE 通常是 `xdg-desktop-portal-kde`。最小化窗口管理器需要自行提供 D-Bus 用户会话和 portal；文件选择器不可用时，可使用应用的“手动输入文件路径”导入/导出，不影响本地编辑保存。

把发行包和校验文件下载到同一目录，先校验对应文件。`SHA256SUMS.txt` 包含全部平台文件；仅下载一种时可选择对应行：

```sh
grep '  nebulabook-v4.1.0-linux-x64.tar.gz$' SHA256SUMS.txt | sha256sum -c -
tar -xzf nebulabook-v4.1.0-linux-x64.tar.gz
cd nebulabook-v4.1.0-linux-x64
./nebulabook
```

ARM64 替换文件名中的 `x64` 为 `arm64`。不需要 sudo 启动。请从终端首次运行以查看缺库/显示服务器错误。可直接从解压目录运行，也可执行包内 `./install-linux.sh` 安装至当前用户目录并添加应用菜单入口；安装脚本不更改笔记数据。详细安装路径以脚本输出为准。

不要用 root 运行程序。升级前关闭所有实例、备份数据，然后替换程序或重新运行安装脚本。默认安装程序在 `~/.local/lib/nebulabook/`，命令链接在 `~/.local/bin/nebulabook`，菜单入口在 `${XDG_DATA_HOME:-$HOME/.local/share}/applications/nebulabook.desktop`。使用 `--prefix` 时以脚本输出为准。卸载时关闭程序，再仅删除这些程序目录/链接/菜单入口；保留 `nebulanotepad` 数据目录即可保留笔记。安装新版本不会删除旧 `nebula-notes` 程序或菜单；核对新程序和备份后可自行清理旧程序，勿删除笔记数据。

### Windows 安装与运行

此依赖升级使用 WGPU 30 / Direct3D 12，优先显卡，在可用时使用系统 WARP 软件后备；不依赖显卡厂商 OpenGL 驱动，继续使用系统 FXC 编译器，不捆绑额外 DXC DLL。

图形验证与兼容边界：

- WGPU 30 的 DX12 硬件路径要求资源绑定 Tier 2 或更高；旧显卡可能转为 WARP 软件渲染，CPU 占用和性能可能下降。参见 [WGPU 的适配器检查](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/dx12/adapter.rs)。
- 微软文档确认 Windows 10 1709 起的系统 WARP 支持 Feature Level 12_0 / 12_1，对应至少 Tier 2。这是有官方资料支撑的推荐软件后备基线；更早的 Windows 10 与旧显卡组合尚未验证，不据此断言它们都无法运行。参见 [WARP 能力](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/directx-warp)及 [Feature Level 与资源绑定级别](https://learn.microsoft.com/en-us/windows/win32/direct3d12/hardware-feature-levels)。
- CI 使用 Windows 2025 的 x64 / x86 WOW64 和 Windows 11 ARM64 runner。各架构的实际启动结果以该提交的 CI 为准，这些环境不能替代所有 Windows 10、实体 GPU 或驱动组合的验证。

EXE 依赖 Microsoft Visual C++ v14 Redistributable。若提示缺少 `VCRUNTIME140.dll`，请从微软官方安装与所选 EXE 架构一致的运行库：[x64](https://aka.ms/vc14/vc_redist.x64.exe)、[x86](https://aka.ms/vc14/vc_redist.x86.exe)、[ARM64](https://aka.ms/vc14/vc_redist.arm64.exe)。[官方说明](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170)。x86 EXE 在 64 位 Windows 上同样需要 x86 运行库。

```powershell
Get-FileHash .\nebulabook-v4.1.0-windows-x64.exe -Algorithm SHA256
```

对照校验文件后运行。程序尚未代码签名，可能出现 SmartScreen 或未知发布者提示；请确认下载来源，不要随意绕过系统警告。x86 CI 在 WOW64 上运行，不等于纯 32 位 Windows 实机验证；ARM64 CI 使用 Windows 11，不代表旧 Windows 10 ARM64 设备全部验证。

## 中文、输入法与显示

优先读取系统中文字体；Linux 也通过 Fontconfig 发现可用字体。不再分发系统字体。缺少字体时会显示英文提示，请安装 Noto Sans CJK 或文泉驿并重启。Windows 使用微软雅黑/黑体等系统字体。

支持 X11 / Wayland 窗口。中文输入法依赖桌面的 IBus/Fcitx、会话设置以及 winit 支持；中文字体通过不等于每种输入法组合都已实测。高 DPI 采用窗口系统缩放。远程桌面、虚拟机可尝试系统 Mesa 软件渲染，但性能取决于环境。

## 数据、备份与恢复

数据目录沿用旧 Nebula 原生版身份，不随仓库名、程序位置或 CPU 架构改变。新默认主文件为 `.nebula`，不是简单更换 JSON 后缀：

- Linux：`${XDG_DATA_HOME:-$HOME/.local/share}/nebulanotepad/notebook.nebula`（`XDG_DATA_HOME` 应为绝对路径）
- Windows：通常是 `%LOCALAPPDATA%\Nebula\Nebula Notepad\data\notebook.nebula`
- 以窗口底部“数据文件”悬停显示的实际路径为准

`.nebula` 使用 XChaCha20-Poly1305、每次新编码的系统随机 nonce、认证版本头和完整性校验，提供**免密码的轻量混淆与误改检测**。兼容密钥随开源应用公开，任何能读取源码/程序的人都能解密或重新生成有效文件；**这不是密码保护、可靠保密或对抗有意篡改的数字签名**。没有密码弹窗，也不依赖本机密钥库，所以备份可在其他设备导入。敏感笔记应配合系统磁盘加密。协议与限制见 [NEBULA_FORMAT.md](NEBULA_FORMAT.md)。

主文件 `notebook.nebula`、上一份成功快照 `notebook.backup.nebula`、初始迁移恢复快照 `notebook.migration.nebula`（仅迁移时生成）位于同一目录。实例锁继续使用 `notebook.json.lock`，与旧版共用，防止新旧版本同时写入。进程退出后操作系统释放锁；锁文件存在不代表仍被占用，不要在运行时删除它。

Linux 新建目录 0700，主文件、备份、锁和导出文件 0600；已有目录权限不修改。Windows 继承当前用户目录 ACL。

1. 定期用“导入 / 导出”导出完整 `.nebula` 备份到其他安全位置。TXT、Markdown 和显式 JSON 导出为明文。
2. 复制或恢复数据前关闭所有新旧版本。导出不会覆盖已有文件，请选择新名称。
3. 主文件损坏时，先保留/重命名损坏文件，再将已确认完好的 `notebook.backup.nebula` 复制为 `notebook.nebula` 后重开。迁移阶段中断可使用 `notebook.migration.nebula`；它仅包含迁移时内容，可能比上一快照更旧。
4. 主文件缺失但发现新格式恢复备份时停止并提示，绝不静默回退到陈旧 JSON。错误头、未知格式/数据版本、损坏 nonce、认证失败、截断或多余字节均拒绝读取。
5. 保存失败时保留当前编辑，先导出含草稿的备份，再处理磁盘已满、权限、外部修改等错误。不要强制结束进程。

建议用支持硬链接及原子替换的本地文件系统（例如 ext4、NTFS）。首次保存和迁移以同目录已同步临时文件的硬链接原子发布，避免覆盖竞争中出现的文件；不支持硬链接的 FAT/exFAT 或受限文件系统会明确报错并保留原数据。NFS、同步盘或异常文件系统的锁/替换语义可能不同，不承诺跨设备同时编辑或所有掉电场景安全。上一份快照不能替代独立长期备份。

### 从原生 4.0 / 4.1 JSON 迁移

关闭旧程序后运行新版，会在原数据目录完整验证 `notebook.json`，保留其**原始字节和旧 `.json.bak`**，先生成可恢复的 `.nebula` 迁移快照，再原子创建新主文件。笔记 ID、内容、回收站、文件夹、标签、原始 HTML 和已支持的旧元数据均保留；未知字段/不支持版本不会被丢弃或强行迁移。旧 JSON 损坏时停止，不能创建空库掩盖问题。

新格式主文件存在时优先读取它。迁移后的旧 JSON 带有经过认证的原始 SHA-256 记录：若降级版或其他工具改写旧 JSON，新版会停止打开/保存，避免覆盖任一版本。关闭所有版本并分别备份，再将旧 JSON 移到其他目录、重开新版并按需显式导入。用户自行删除或迁走旧 JSON 不会锁死新库；不同内容重新出现时仍会提示冲突。旧 JSON 和旧备份仍是明文，不会自动删除；请核对新备份可恢复后自行决定如何保管。

跨设备推荐导出/导入完整 `.nebula` 备份。导出不附带本机旧文件的迁移关联，导入默认生成新 ID 副本；重复导入会增加副本。需要旧版兼容时，选择明确标为明文的 JSON 导出，而不要让旧版继续编辑已迁移目录。

### 从旧浏览器版迁移

不要先清除浏览器数据或卸载旧版。

1. 在原来的浏览器、原来的站点地址打开旧 Nebula。
2. 审阅 [`scripts/export-legacy-data.js`](scripts/export-legacy-data.js)，在该页面开发者工具 Console 中运行。它仅在只读事务中导出 `NebulaLocalDB`，不修改/删除数据。
3. 在原生程序选择“导入”，选中下载的 JSON。
4. 核对标题、正文、数量及回收站，再导出一份完整 .nebula 备份。
5. 确认备份可恢复后，自行决定是否清理旧数据。

也可导入旧 HTML/TXT/Markdown。HTML 转文本可能改变排版，原始 HTML 会保留。原生程序不能自动读取浏览器 IndexedDB，也不连接旧 PostgreSQL；服务端数据须先由旧系统导出。

## 故障排查

- `GLIBC_2.xx not found`：系统低于该产物基线。升级受支持发行版，或在本机从源码构建；不要手动替换系统 libc。
- 缺少 `libxkbcommon` / `libGL` / `libEGL`：安装相应发行版图形运行库。`ldd ./nebulabook` 可检查直接动态依赖；驱动/显示库还可能运行时加载。
- 无法连接 X11 / Wayland：从已登录桌面的终端运行，检查 `DISPLAY` / `WAYLAND_DISPLAY` / `XDG_RUNTIME_DIR`。普通 SSH 无图形会话不能显示窗口。
- Wayland 问题：若系统提供 XWayland，可临时 `env -u WAYLAND_DISPLAY ./nebulabook` 切换 X11 路径；这不是无桌面模式。
- 虚拟机显卡/OpenGL 问题：先修复系统驱动；Mesa 环境可尝试 `LIBGL_ALWAYS_SOFTWARE=1 ./nebulabook`。
- 文件选择器无响应/取消：检查 D-Bus 和 desktop portal，或使用应用内手动路径入口。填写完整路径，不会自动展开 shell 命令。
- 数据被占用：关闭另一个使用同一数据目录的窗口后重试。新仓库版与旧原生版也会互斥。
- 字体方框：安装中文字体后重启。字体和输入法是两个独立环节。

需要诊断时可显式指定一个尚不存在的日志文件；日志记录版本、平台、渲染器和引擎错误，不读取笔记内容。已有日志不会覆盖：

```sh
NEBULABOOK_STARTUP_LOG="$HOME/nebulabook-startup.log" ./nebulabook
```

报告问题请附系统/架构、版本、图形会话、报错以及校验结果；不要公开私人笔记。无交互检查可设 `NEBULABOOK_NO_ERROR_DIALOG=1`，它只禁止错误对话框，不自动修复启动错误。为兼容旧启动配置，仍接受 `NEBULA_STARTUP_LOG`、`NEBULA_NO_ERROR_DIALOG` 和 `NEBULA_REQUIRE_CJK_FONT`；同名后缀的 `NEBULABOOK_` 配置优先。

## 从源码构建

安装 [官方 Rust 工具链](https://rust-lang.org/tools/install/)（Rust 1.95+；仓库使用 stable）和 Git。Linux Debian/Ubuntu 构建依赖：

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libgl1-mesa-dev
git clone https://github.com/Tran314/Nebulabook.git
cd Nebulabook
cargo run --locked
cargo build --locked --release
./target/release/nebulabook
```

Windows 还需要 Visual Studio C++ Build Tools / Windows SDK；执行同样 Cargo 命令，产物为 `target\release\nebulabook.exe`。构建下载 crates.io 依赖，构建后的应用无需联网。保留已提交的 `Cargo.lock` 和 `--locked`。

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked -p nebulabook --no-deps --all-targets -- -D warnings
cargo test --locked --no-default-features
node --test tests/legacy-export.test.cjs
```

Node 18+ 仅用于旧数据导出脚本的开发测试，不是应用运行依赖。Linux 打包/GUI 检查、发行边界见 [LOCAL_CI.md](LOCAL_CI.md)，实现见 [ARCHITECTURE.md](ARCHITECTURE.md)，验证范围见 [AUDIT.md](AUDIT.md)。

## CI 与发布

[GitHub Actions](https://github.com/Tran314/Nebulabook/actions) 覆盖 Linux x64/ARM64 原生 runner 和 Windows x64/x86/ARM64。Linux 检查 Xvfb/X11 编辑保存重开与 Weston/Wayland 启动；Windows 检查原生渲染器初始化。软件渲染/虚拟桌面验收不是所有实体 GPU、输入法或发行版的认证。

普通推送只做检查并保留构建产物；v4.1.0 发布仅限本仓库 main、明确发布标记或手动发布输入，且全部矩阵成功。发布前核验 commit、架构、ELF/PE、大小、SHA256、Linux glibc 基线；已有不同 tag/release 不会覆盖。配置存在不代表某个提交已经跑过，请查看对应提交的 CI 结论。

## 目录与许可

- `src/app.rs`：原生界面、草稿和保存状态
- `src/model.rs`：版本化模型和校验
- `src/storage.rs`：文件锁、原子保存、备份
- `src/import_export.rs`：非破坏式导入导出
- `scripts/`：旧数据导出、Linux 验证/打包/安装
- `.github/workflows/ci.yml`：跨平台检查与受限发布

MIT，见 [LICENSE](LICENSE)。保留原项目版权声明，第三方 crate 使用各自许可；系统库和字体由系统供应方提供。原项目历史：[note-system](https://github.com/Tran314/note-system)，原生重构首发：[v4.0.0](https://github.com/Tran314/note-system/releases/tag/v4.0.0)。
