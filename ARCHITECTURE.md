# 原生架构

## 运行边界

单个 Rust 桌面进程。eframe 创建操作系统窗口，以 egui 渲染 UI；Windows使用WGPU30/DX12，默认硬件优先并保留系统WARP软件后备；硬件要求资源绑定Tier2，较早Windows10和旧GPU组合的验证边界见README；Linux使用Glow/OpenGL，启用X11和Wayland。不启动 HTTP 服务、不打开浏览器、不嵌入 WebView、不执行导入的 HTML/JavaScript。文件对话框由操作系统提供；Linux另外提供不依赖portal的完整路径入口。

## 数据流

1. 取得用户数据目录中的排他文件锁。
2. 读取 .nebula 认证封装，验证固定格式版本、完整性及内部 Notebook schema；错误阻止覆盖原文件。
3. UI 编辑独立缓冲区，800ms 自动保存或手动保存。
4. 将变更应用到 Notebook 副本，验证并持久化成功后才替换内存中的已保存状态。
5. 切换笔记、导入或关闭窗口时先保存；失败保留缓冲区并显示错误。

存储层负责同目录临时写入、同步和原子替换；首次发布使用硬链接原子 create-new，不支持硬链接时明确报错。恢复备份保存上一份成功快照，多实例文件锁避免两个程序各自覆盖对方。该机制不承诺对所有远程/异常文件系统的掉电安全；仍应做独立备份。

## 导入与数据边界

支持 `.nebula`、TXT、Markdown 和 HTML 导入。导入模块先完整解析、校验候选数据，再合入内存副本；失败不部分写入。重复导入默认复制并生成新 ID。HTML 仅作非执行解析，正文变为纯文本，原文保留在完整 `.nebula` 备份中。完整导出仅使用 `.nebula`；TXT / Markdown 只导出所选笔记正文。

不读取或迁移旧原生 JSON，不导入 JSON / Dexie，不提供浏览器导出脚本或明文 JSON 导出。已有 `.nebula` 模型中的旧元数据字段保留为非执行数据，不能作为删除字段或重置笔记库的理由。仅有旧 JSON 时打开新笔记库，旧文件保持原样；用户须自行在原程序导出受支持的文本格式。

## 取舍

保留记事本最重要的编辑、保存、查找、导入导出。Markdown 为文本，HTML 不作为可执行 DOM。已有 `.nebula` 中的文件夹、标签和元数据会保留，当前没有完整管理 UI。没有云同步、后端鉴权或数据库部署依赖。

## Linux 发行边界

Linux x64/ARM64在各自原生Ubuntu22.04 runner编译，要求glibc2.35+。不捆绑图形驱动或字体。系统中文字体由限时Fontconfig发现和固定路径后备读取；XDG用户数据目录沿用原生4.0.0身份，新增目录0700/文件0600，不更改已有目录权限。

发行包必须以实际打包二进制的SHA256匹配X11和Wayland检查证据。X11验证真实键盘编辑/保存及失败关闭保护；Wayland验证窗口渲染启动。实体GPU、输入法和desktop portal仍需具体桌面验收。

## 名称与持久化身份

可见品牌 Nebulabook、Rust 包/二进制 `nebulabook`、仓库 `Tran314/Nebulabook`。持久化身份仍为 `ProjectDirs::from("com", "Nebula", "Nebula Notepad")`，保留 Linux `nebulanotepad` 和 Windows 应用数据路径。主文件为 `notebook.nebula`，实例锁名稳定为 `notebook.json.lock`，保证先前 `.nebula` 构建与当前程序互斥；锁文件名不表示应用仍支持 JSON 数据文件。环境配置只使用 `NEBULABOOK_` 前缀。

## .nebula 封装与保存

XChaCha20-Poly1305 使用公开的格式密钥、24 字节系统随机 nonce、44 字节全部认证头和 16 字节 tag。这里只提供免密码的轻量混淆与完整性检查，不提供可靠保密或对抗源码持有者的篡改签名。精确布局和可复现 fixture 见 [NEBULA_FORMAT.md](NEBULA_FORMAT.md)。

`.nebula` v1 的封装和 Notebook schema 保持不变。历史 `legacy_source_sha256` 字段仍校验形状但不参与操作；新编码始终写入 null。应用不检查旁边的旧 JSON。打开、保存和导入继续严格验证当前格式；保存逐字节检查 `.nebula` 的外部变更，原子更新主文件并保存上一份快照。

主文件缺失且发现 `notebook.backup.nebula` 或先前构建留下的 `notebook.migration.nebula` 时，要求显式恢复，避免隐藏已有当前格式数据。当前程序不会创建迁移快照。损坏或不支持的 `.nebula` 不回退到其他格式，也不自动重置为空库。
