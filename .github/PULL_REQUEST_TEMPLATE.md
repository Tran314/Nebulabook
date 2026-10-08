## 变更

简述本次原生桌面改动。

## 数据安全与兼容

- [ ] 已有数据兼容或提供非破坏式迁移
- [ ] 保存错误可见，失败后保留编辑
- [ ] 没有新增默认网络请求/浏览器/服务端依赖

## 验证

- [ ] cargo fmt --all -- --check
- [ ] cargo test --locked --all-targets
- [ ] cargo clippy --locked --all-targets -- -D warnings
- [ ] cargo build --locked --release
- [ ] Linux X11 / Wayland 及中文字体检查（涉及 Linux 时）
- [ ] Windows 原生窗口检查（涉及 Windows 时）
- [ ] 更名、升级不改变旧数据目录或覆盖备份

请明确未运行的检查、平台限制和剩余风险。UI 改动可附截图。
