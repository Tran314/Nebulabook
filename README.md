# Nebulabook · Rust 原生记事本

轻量、离线、本地保存的桌面记事本。Rust + egui/eframe 创建真正的原生窗口，不需要浏览器、WebView、Electron、账号或后端。

本仓库是 [Tran314/note-system](https://github.com/Tran314/note-system) 原生重构版本的独立后续项目，从原仓库 `0fc4091d96512053cb491976a5476a898b1c559d` 分出。旧仓库及其版本历史保留，原 MIT 许可保持不变；这里不包含已移除的前后端代码。

## 功能

- 中文界面，标题/正文编辑、全文搜索、置顶、回收站与恢复
- 800 ms 自动保存，Ctrl+S 保存、Ctrl+N 新建、Ctrl+F 搜索
- TXT、Markdown、HTML、旧版 JSON 和原生 JSON 导入
- TXT / Markdown 导出、完整 JSON 备份；导出不会覆盖已有文件
- 同目录原子保存、上一份快照备份、多实例排他文件锁
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

项目及应用新名称为 Nebulabook，但数据目录刻意沿用旧 Nebula 原生 4.0.0 的身份，绝不因改名创建一份看似空白的新库。关闭旧程序后，同一系统用户直接运行 Nebulabook 即可读取原来的笔记、备份和锁；不复制、不覆盖、不自动移动数据。旧新版同时运行时仍会互斥。

数据目录不随仓库名、程序位置或 CPU 架构改变：

- Linux：`${XDG_DATA_HOME:-$HOME/.local/share}/nebulanotepad/notebook.json`（`XDG_DATA_HOME` 应为绝对路径）
- Windows：通常是 `%LOCALAPPDATA%\Nebula\Nebula Notepad\data\notebook.json`
- 以窗口底部“数据文件”悬停显示的实际路径为准

数据是未加密的本地明文 JSON。Linux 新建应用目录为用户私有，主文件、备份和锁文件使用私有权限；已有目录权限不会被擅自修改。敏感信息应配合系统磁盘加密。

主文件 `notebook.json`、上一份成功快照 `notebook.json.bak`、实例锁 `notebook.json.lock` 位于同一目录。锁由操作系统持有，进程结束会释放；`.lock` 文件存在本身不代表仍被占用，不要在程序运行时删除锁文件。

1. 定期通过“导入 / 导出”导出完整 JSON 备份到其他安全位置。
2. 复制数据目录前先关闭所有 Nebulabook 实例。
3. 主文件损坏时，先保留/重命名损坏文件，确认 `.bak` 可读后复制为 `notebook.json`，再打开应用。
4. 若主文件缺失而 `.bak` 存在，程序会停止并提示恢复，不会用空数据覆盖它。
5. 保存发生权限错误、磁盘已满、外部修改或锁竞争时，当前编辑会保留；先导出包含草稿的备份，再处理错误。不要强制终止进程。

上一份快照不能替代长期备份。建议使用本地文件系统；NFS、同步盘或异常文件系统的锁/原子替换语义可能不同，未承诺跨设备同时编辑或所有掉电场景安全。

### 从原生 4.0.0 迁移

同一用户机器上关闭旧程序后运行新版即可使用原数据目录。跨机器复制前先备份并关闭程序；推荐从新程序导入完整 JSON 备份。导入默认生成新 ID 的副本，不覆盖已有笔记，重复导入会增加副本。

### 从旧浏览器版迁移

不要先清除浏览器数据或卸载旧版。

1. 在原来的浏览器、原来的站点地址打开旧 Nebula。
2. 审阅 [`scripts/export-legacy-data.js`](scripts/export-legacy-data.js)，在该页面开发者工具 Console 中运行。它仅在只读事务中导出 `NebulaLocalDB`，不修改/删除数据。
3. 在原生程序选择“导入”，选中下载的 JSON。
4. 核对标题、正文、数量及回收站，再导出一份原生 JSON 备份。
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
cargo clippy --locked --all-targets -- -D warnings
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
