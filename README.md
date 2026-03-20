# CodeMerge

CodeMerge 是一个基于 `Rust + gpui + gpui-component` 的桌面文件合并工具，用于从目录或文件集合中生成目录树和合并后的文本结果。

## 当前定位
- 桌面端代码整理与上下文打包工具
- 面向本地仓库、代码审查、LLM 上下文准备等场景
- 处理核心与 UI 分层，支持持续工程化维护

## 主要能力
- 选择文件夹、单独文件与 `.gitignore`
- 编辑目录黑名单和扩展名黑名单
- 输出 `Default / XML / PlainText / Markdown`
- 支持完整模式和仅目录树模式
- 支持内容压缩、字符数与 token 估算
- 支持目录树复制、预览复制、结果文件导出
- 支持配置持久化
- 支持大文件预览的懒加载与虚拟列表渲染

## 架构概览
- `src/domain.rs`
  - 稳定领域类型与默认配置
- `src/processor/*`
  - 文件遍历、读取、压缩、合并、统计
- `src/services/*`
  - 预检、处理、预览、树构建、树索引、配置加载保存
- `src/ui/*`
  - 应用状态、Workspace 视图、交互事件、后台轮询
  - 目录树面板采用“树索引 + UI 投影 + 轮询适配层”结构，减少 `workspace` 顶层字段散落
- `src/utils/*`
  - i18n、配置存储、临时文件、路径辅助

## 本地运行
```bash
cargo run
```

Windows 是主要运行目标。Linux CI 维持编译和测试兼容。
Ubuntu/Debian 构建需额外安装 `libxkbcommon-x11-dev`，否则 `gpui` 在链接阶段会报缺少 `-lxkbcommon-x11`。

## 质量门禁
```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

提交前至少跑完以上三项。

## 配置与数据
- 配置文件：
  - Windows: `%APPDATA%/codemerge/config.json`
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
- Linux 产物包含 `.deb`
- `assets/` 属于源码资源目录，必须随仓库一起提交；运行时 SVG 图标与 Windows `assets/app.ico` 都从这里读取
- Windows 构建直接复用仓库内的 `assets/app.ico`，用于嵌入可执行文件与安装包图标

## 已知限制
- `.gitignore` 的否定规则 `!` 目前不支持
- 复杂 UI 仍以 `workspace` 为中心，目录树已拆到独立 controller，其他面板后续仍需继续下沉
