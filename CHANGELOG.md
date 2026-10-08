# 更新日志

## 4.1.0 · Linux 桌面适配与独立仓库

- 项目及应用统一更名为 Nebulabook，Rust 包/可执行文件为 `nebulabook`，保留旧数据目录和格式以直接使用原笔记。
- Rust 原生版本迁移到 `Tran314/Nebulabook`；旧仓库及历史不删除。
- Linux x64 / ARM64 原生 CI、X11/Wayland 启动检查与 tar.gz 发行包。
- 系统中文字体发现、私有数据目录、无 portal 时的手动路径导入导出。
- Linux 运行依赖、架构选择、备份恢复、旧版迁移与故障排查文档。
- 修复标题 Tab 进入正文时被重复插入为制表符的问题，保留正文 Tab / ShiftTab 缩进。
- 保留 Windows x64 / x86 / ARM64 的 DX12 渲染及发布检查。

## 4.0.0 · 原生重构

Rust + egui/eframe 取代 React/Node/Electron。增加本机 JSON、原子保存、上一份快照备份、多实例锁、纯文本编辑/搜索、置顶、回收站和旧浏览器导出迁移。

原始版本与完整旧历史在 [Tran314/note-system](https://github.com/Tran314/note-system)。旧 Web/服务端功能不代表当前原生版本的能力。
