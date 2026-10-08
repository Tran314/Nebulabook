# Nebulabook v4.1.0 · Linux 与 Windows 原生桌面

Rust 重构版本现在位于独立仓库 [Nebulabook](https://github.com/Tran314/Nebulabook)。原 [note-system](https://github.com/Tran314/note-system) 仓库及 v4.0.0 保留。

## 下载

- Linux x64 / ARM64：tar.gz，glibc 2.35+，需要 X11/Wayland 桌面、OpenGL/ES 2.0 兼容驱动及中文字体；包内附用户目录安装脚本。
- Windows x64 / x86 / ARM64：EXE，Windows 10+，需要对应架构的 Microsoft VC++ v14 运行库。ARM64 CI 为 Windows 11；x86 在 WOW64 验证。
- `SHA256SUMS.txt` 与每个平台 `build-info` 可验证来源源码、架构及文件校验和。

## 本次新增

Linux 原生 x64/ARM64 构建与 GUI 检查、系统字体发现、私有数据目录、手动文件路径导入导出、完整中文安装与迁移说明。Windows 三架构支持继续保留。

## 数据与边界

Nebula 原生版本现更名 Nebulabook；数据目录/JSON 格式沿用 4.0.0，无需移动数据；升级前关闭旧程序并导出备份。旧浏览器 IndexedDB 需要先在原地址导出 JSON，再导入原生程序。程序不会连接旧数据库或删除旧数据。

默认离线，无浏览器/WebView/服务端。笔记未加密；建议配合磁盘加密并做独立备份。Linux 是动态链接发行包，不保证所有发行版/显卡/输入法组合；虚拟桌面 CI 不等于所有实机认证。Windows EXE 暂未代码签名。

详见 [README](https://github.com/Tran314/Nebulabook#readme)。
