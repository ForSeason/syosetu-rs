# AGENTS

该项目提供了一个基于 Rust 的命令行工具，可抓取 syosetu 网站的小说并调用 DeepSeek API 进行翻译，界面使用 ratatui 进行交互。

## 代码结构
- `src/main.rs`：程序入口，解析命令行参数，初始化日志并启动 `App`。
- `src/app.rs`：保存 UI 状态并负责事件循环与业务逻辑。
- `src/ui.rs`：封装了 TUI 的绘制函数。
- `src/syosetu.rs`：实现 `NovelSite` trait 以抓取两种站点 (`ncode.syosetu.com` 和 `syosetu.org`)，并提供 `Translator` 用于调用 DeepSeek API。
- `src/memory.rs`：简单的 JSON 文件实现，用于保存章节翻译及专有名词表。

## 开发约定
1. 使用稳定版 Rust 工具链。
2. 提交前请执行 `cargo fmt` 保证代码格式统一。
3. 运行 `cargo clippy --all-targets -- -D warnings` 以确保没有警告。
4. 项目当前没有单元测试，但仍建议在提交前运行 `cargo test` 以确认代码能够顺利编译。
5. 日志默认写入 `app.log`，生成的 JSON 文件也会保存在项目根目录（已在 `.gitignore` 中忽略）。

