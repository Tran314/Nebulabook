# 本地运行

安装官方 Rust 工具链以及操作系统所需的本地图形/链接库后执行：

```sh
cargo run --locked
```

这是本地原生窗口，没有网页地址、开发服务器或预览端口。正式构建执行 `cargo build --locked --release`，运行 `target/release/nebulabook`（Windows 后缀 `.exe`）。

如未出现窗口，检查图形会话和显卡驱动。没有 DISPLAY/Wayland 的无头 Linux 环境仍可执行数据层测试，但不等同于完成原生窗口验收。
