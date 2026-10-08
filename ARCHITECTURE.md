# 原生架构

## 运行边界

单个 Rust 桌面进程。eframe 创建操作系统窗口，以 egui 渲染 UI；Windows10+使用WGPU/DX12，默认硬件优先并保留系统WARP软件后备；Linux使用Glow/OpenGL，启用X11和Wayland。不启动 HTTP 服务、不打开浏览器、不嵌入 WebView、不执行导入的 HTML/JavaScript。文件对话框由操作系统提供；Linux另外提供不依赖portal的完整路径入口。

## 数据流

1. 取得用户数据目录中的排他文件锁。
2. 读取并验证版本化 Notebook JSON；错误阻止覆盖原文件。
3. UI 编辑独立缓冲区，800ms 自动保存或手动保存。
4. 将变更应用到 Notebook 副本，验证并持久化成功后才替换内存中的已保存状态。
5. 切换笔记、导入或关闭窗口时先保存；失败保留缓冲区并显示错误。

存储层负责同目录临时写入、同步和原子替换。恢复备份保存上一份成功快照，多实例文件锁避免两个程序各自覆盖对方。该机制不承诺对所有远程/异常文件系统的掉电安全；仍应做独立备份。

## 迁移边界

旧浏览器数据按 origin 隔离，原生程序不能自动读取。附带的导出脚本由用户在旧 origin 主动运行，仅导出 IndexedDB，不迁移账号凭据。导入模块先完整解析、校验候选数据，再合入内存副本；失败不部分写入。重复导入默认复制并生成新 ID。HTML 仅作非执行解析，正文变为纯文本，原文保留在备份中。

旧浏览器形态：NebulaLocalDB / Dexie v1（IndexedDB physical version10），notes/folders/tags/settings 四个对象仓库。原生模型使用独立 schema_version，不尝试改写浏览器数据库。

## 取舍

保留记事本最重要的编辑、保存、查找、导入导出。Markdown为文本，HTML不再作为可执行DOM。文件夹/标签可保留迁移元数据，当前没有完整管理 UI。移除云同步占位、后端鉴权与数据库部署依赖，避免旧服务被误部署。

## Linux 发行边界

Linux x64/ARM64在各自原生Ubuntu22.04 runner编译，要求glibc2.35+。不捆绑图形驱动或字体。系统中文字体由限时Fontconfig发现和固定路径后备读取；XDG用户数据目录沿用原生4.0.0身份，新增目录0700/文件0600，不更改已有目录权限。

发行包必须以实际打包二进制的SHA256匹配X11和Wayland检查证据。X11验证真实键盘编辑/保存及失败关闭保护；Wayland验证窗口渲染启动。实体GPU、输入法和desktop portal仍需具体桌面验收。

## 名称与持久化身份

可见品牌 Nebulabook、Rust包/二进制 `nebulabook`、仓库 `Tran314/Nebulabook`。持久化身份仍为 `ProjectDirs::from("com", "Nebula", "Nebula Notepad")`，保留Linux `nebulanotepad`和Windows旧应用数据路径；不自动搬动或复制既有数据。旧浏览器 `NebulaLocalDB` 与 `nebula-legacy-indexeddb` 协议保持不变。
