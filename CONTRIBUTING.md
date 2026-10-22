# 参与贡献

## 开发环境

- Rust stable（建议 rustup）
- Linux 编译需要 `build-essential`、`pkg-config`、`libfontconfig1-dev`、`libxcb*`、`libxkbcommon-dev`
- Windows 可用 MSVC 或 `x86_64-pc-windows-gnu`
- macOS 需要 Xcode Command Line Tools，且必须在 Mac 上本机编译

```bash
git clone https://github.com/gordonwen/mqttc.git
cd mqttc
cargo test
cargo run
```

无显示环境时 `cargo run` 无法弹出窗口，属预期。`cargo test` 不依赖窗口。

更完整的模块说明与常见任务见 [.monkeycode/docs/DEVELOPER_GUIDE.md](.monkeycode/docs/DEVELOPER_GUIDE.md)。第一次读源码建议从 [.monkeycode/docs/ARCHITECTURE_GUIDE.md](.monkeycode/docs/ARCHITECTURE_GUIDE.md) 开始。

## 提交

提交信息使用前缀：

- `feat`：新功能
- `fix`：缺陷修复
- `docs`：文档
- `chore`：构建、依赖、仓库杂项

界面文案必须同时提供中文和英文，走 `I18n.t("中文", "English")` 与 `i18n::t`。不要在源码里硬编码密钥。新增的编解码、配置迁移、规则匹配等纯逻辑请附单元测试。

## 拉取请求

1. 从最新默认分支拉出主题分支
2. 保持改动聚焦，一个 PR 只做一类事
3. 打开 PR 前本地执行 `cargo test`
4. 按 `.github/PULL_REQUEST_TEMPLATE.md` 填写说明

预编译 Windows / Linux 产物请放到 GitHub Releases，不要提交 `dist/` 下的可执行文件。

## 行为准则

参与本仓库即表示同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。
