# CodeMerge

CodeMerge 是一个基于 `Rust + gpui + gpui-component` 的桌面文件合并工具，用于从目录或文件集合中生成目录树和合并后的文本结果。

## 当前定位
- 桌面端代码整理与上下文打包工具
- 面向本地仓库、代码审查、LLM 上下文准备等场景
- 处理核心与 UI 分层，支持持续工程化维护

## 主要能力
- 选择文件夹、单独文件与 `.gitignore`
- 支持解析 `.zip` 压缩包中的文本文件并参与单文件合并
- 编辑目录黑名单和扩展名黑名单
- 输出 `Default / XML / PlainText / Markdown`
- 支持完整模式和仅目录树模式
- 支持内容压缩、字符数与 token 估算
- 支持目录树复制、预览复制、结果文件导出
- 支持配置持久化
- 支持大文件预览的懒加载与虚拟列表渲染
- 标题栏显示当前版本号，并提供 GitHub 开源仓库入口
- Windows / macOS 支持品牌化自定义窗口标题栏；Linux 在不支持 client-side decorations 的环境会自动退回系统窗口装饰

## 架构概览
- `src/domain.rs`
  - 稳定领域类型与默认配置
- `src/processor/*`
  - 文件遍历、读取、压缩、合并、统计
- `src/services/*`
  - 预检、处理、预览、树构建、树索引、配置加载保存
- `src/ui/*`
  - 应用状态、Workspace 编排、面板视图、交互事件、后台轮询
  - 结果区采用“结果面板容器 + 树面板视图 + 预览面板视图”拆分，避免滚动和树交互放大到整块结果区重绘
  - 目录树采用“过滤投影缓存 + 可见行重建”结构，展开/折叠不再重复做全量过滤投影
  - 预览面板把可见范围桶状态保留在局部 view，`PreviewModel` 只保留文档/chunk/请求等业务状态
- `src/utils/*`
  - i18n、配置存储、临时文件、路径辅助

## 本地运行
```bash
cargo run
```

Windows、macOS、Linux 都纳入了构建链路；CI 当前在 Ubuntu 与 macOS 上持续验证。
Ubuntu/Debian 构建需额外安装 `libxkbcommon-x11-dev`，否则 `gpui` 在链接阶段会报缺少 `-lxkbcommon-x11`。
Linux 是否显示完整自定义标题栏取决于桌面环境对 client-side decorations 的实际支持；如果运行时仍返回 server decorations，应用会退化为紧凑页内 header，以避免双标题栏。

## 质量门禁
```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

提交前至少跑完以上三项。

## 性能排查
- 真实性能评估请优先使用 `cargo run --release`
- 可启用 GPUI Inspector 观察元素树和重绘情况：
  ```bash
  GPUI_INSPECTOR=1 cargo run --release
  ```
- 可通过日志查看 GPUI 内部布局/事件轨迹：
  ```bash
  RUST_LOG=gpui=trace cargo run --release 2> trace.log
  ```
- 仓库内置了 `src/ui/perf.rs` 的轻量计数器，供测试和本地调试统计：
  - 子视图条件刷新次数
  - `sync_tree()` 次数与 `tree.set_items()` 次数
  - `sync_preview_table()` 次数
  - 预览 range 请求次数

## 配置与数据
- 配置文件：
  - Windows: `%APPDATA%/codemerge/config.json`
  - macOS: `~/Library/Application Support/codemerge/config.json`
  - Linux: `~/.config/codemerge/config.json`
- 临时结果：
  - 系统临时目录下的 `codemerge/`
- 当前版本会对配置恢复显式区分：
  - 配置不存在
  - 配置内容损坏
  - 配置读写失败

## 开发约定
- 代码搜索优先使用 `ace-tool`。
- 新逻辑优先进入 `services/*` 或纯函数模块，不要继续堆到 `workspace` 交互方法里。
- 修改配置、后台任务、临时目录生命周期时必须补测试。
- 行为或工程门禁变化时同步更新 `AGENTS.md` 与本 README。

## 打包与发布
- CI 位于 `.github/workflows/ci.yml`
- Release workflow 位于 `.github/workflows/release.yml`
- macOS 打包脚本位于 `scripts/package-macos.sh`
- macOS `.app` 元数据模板位于 `packaging/macos/Info.plist.template`
- Linux 产物包含 `.deb`
- macOS 产物包含按架构区分的 `.zip`（`CodeMerge.app`）与 `.dmg`
- Linux `.deb` 会安装应用启动器到 `usr/share/applications/codemerge.desktop`，并安装图标到 `usr/share/icons/hicolor/scalable/apps/codemerge.svg`
- `assets/` 属于源码资源目录，必须随仓库一起提交；运行时 SVG 图标与 Windows `assets/app.ico` 都从这里读取
- Windows 构建直接复用仓库内的 `assets/app.ico`，用于嵌入可执行文件与安装包图标

## 已知限制
- `.gitignore` 的否定规则 `!` 目前不支持
- `Workspace` 仍负责跨面板动作编排和资源生命周期，后续还可以继续把更多动作型逻辑下沉到独立 controller/service
