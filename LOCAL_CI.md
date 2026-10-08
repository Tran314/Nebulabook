# 开发与发布验证

## 常规检查

```sh
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo test --locked --no-default-features
cargo clippy --locked -p nebulabook --no-deps --all-targets -- -D warnings
cargo build --locked --release
node --test tests/legacy-export.test.cjs
```

Node 18+ 仅用于开发检查。设置 `NEBULABOOK_REQUIRE_CJK_FONT=1` 可让字体单测在无系统中文字体时失败，而非跳过。正常程序不依赖 Node 或 Python。

## Linux 真实虚拟桌面检查

Ubuntu 22.04 示例：

```sh
sudo apt install libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev libegl1-mesa-dev libgles2-mesa-dev libgl1-mesa-dri libegl-mesa0 \
  fonts-noto-cjk xvfb xauth xdotool openbox weston dbus-x11 imagemagick
cargo build --locked --release
bash scripts/smoke-linux.sh target/release/nebulabook
```

脚本创建独立临时数据目录和 D-Bus 会话。Xvfb/X11 下由真实键盘事件检查新建、编辑、保存、权限、正常关闭、重开及外部修改冲突阻止关闭；Weston/Wayland 下检查渲染器、中文 glyph 初始化和进程存活。证据在 `smoke-output/`。这些是软件渲染虚拟桌面检查，不是所有物理 GPU、输入法、desktop portal 或发行版的认证。

环境若禁止创建 Unix socket，显示服务器不能启动；不能将测试跳过写成 GUI 通过。仅数据层测试仍可运行。

## Linux 打包

正式发行必须在与目标架构相同的 GNU/Linux 上构建；CI 固定 Ubuntu 22.04 x64 / ARM64。示例：

```sh
rustup target add x86_64-unknown-linux-gnu
cargo build --locked --release --target x86_64-unknown-linux-gnu
bash scripts/smoke-linux.sh target/x86_64-unknown-linux-gnu/release/nebulabook
bash scripts/package-linux.sh --target x86_64-unknown-linux-gnu --smoke-output smoke-output
```

ARM64 替换为 `aarch64-unknown-linux-gnu` 并在 ARM64 主机运行。打包检查目标、ELF、原生主机、动态依赖、直接 GLIBC 符号版本、已运行二进制的 smoke SHA256，再生成 tar.gz、build-info、校验文件。

默认拒绝需要 glibc > 2.35 的二进制；不要简单提高正式包的限制冒称兼容。较新本机系统可使用 `--local-only --glibc-baseline <实际版本>` 生成明确标为 local-only 的测试包，它不能进入发布流程。指定其他二进制位置用 `--binary <文件>`，输出目录用 `--output-dir <目录>`。

打包/用户目录安装回归测试为 `python3 scripts/test-linux-packaging.py`，跨平台发行校验正反例为 `python3 scripts/test-release-verifier.py`；后者使用明确标记的合成平台头和 GUI 证据，只验证校验脚本，不代表真实平台执行。安装只使用当前用户目录，不需要 sudo，不更改笔记数据。

## 发布保护

`.github/workflows/ci.yml` 包含 5 个原生构建/测试任务：Linux x64/ARM64、Windows x64/x86/ARM64。x86 在 Windows x64 WOW64 上执行；Windows ARM64 使用原生 ARM64 runner。

v4.1.0 只有本仓库 `Tran314/Nebulabook` 的 main、明确 `[release v4.1.0]` 提交标记或手动 `publish_release=true` 才可发布。全部矩阵成功后，发布任务重新检查源码SHA、版本、ELF/PE、目标/主机架构、大小、校验和、Linux ABI及GUI证据。只把 HTTP 404 视为 tag/release 缺失；已有不同 tag、不一致产物或未完成 draft 都会停止。发布后重新下载全部资产校验。

更新版本时需要一起更新 Cargo.toml、Cargo.lock、README 文件名、release guard、发布脚本与验证脚本的版本；普通推送不发布。先阅读每个目标提交的真实 CI 结论，不因配置存在就宣布发布成功。
