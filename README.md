# CodeMerge

CodeMerge 是一个基于 `Rust + gpui + gpui-component` 的桌面文件合并工具，用于从目录或文件集合中生成目录树和合并后的文本结果。

## 当前定位
- 桌面端代码整理与上下文打包工具
- 面向本地仓库、代码审查、LLM 上下文准备等场景
- 处理核心与 UI 分层，支持持续工程化维护

## 主要能力
- 选择文件夹、单独文件；文件夹预检结果会显示在可搜索的目录树中，可从树中精确排除文件，也可移除手动添加的单文件或单独清除当前文件夹
- 支持解析 `.zip` 压缩包中的文本文件并参与单文件合并
- 左侧输入面板支持仅作用于本次合并的目录 / 扩展名规则，成功后自动清空且不落盘
- 主工作区采用桌面端增强三栏：左侧输入与处理选项、中间任务状态与完整活动、右侧结果与持久规则
- 开始按钮固定在中栏，并在原位切换为取消按钮；完整活动支持全部 / 失败 / 跳过筛选、错误展开和诊断复制
- 右栏使用“结果 / 规则”一级页签，规则管理单击即可到达；危险操作与超过 32MB 的复制使用 Dialog 确认
- 运行或取消任务期间，所有影响结果的输入与规则会锁定
- 输出 `Default / XML / PlainText / Markdown`
- 支持完整模式和仅目录树模式
- 支持内容压缩、字符数与 token 估算；合并输出中的目录结构、文件路径标签、字符/Token 统计和分隔线可分别关闭，全部关闭时只输出原始文件内容
- 支持目录树复制、合并结果分块后台复制和结果文件后台导出；复制可显示进度并取消，导出按钮会显示保存中、成功或失败状态
- 支持配置持久化
- 配置加载或保存失败时会保留持久告警，并提供重置配置或重试保存入口
- 处理任务使用可取消扫描与独立累计进度；取消请求、最终取消、完成和异常中断会显示明确终态与耗时
- 处理结果会按扩展名和直接父文件夹汇总文件数、字符数与 Token 数；状态面板的统计页提供指标可切换条形图，以及按字符数排列的最大/最小文件榜单
- 支持大文件预览的懒加载与虚拟列表渲染
- 当合并结果过大时，`合并后内容` 页签会先进入延迟加载态，可选择 `加载1MB` 或手动 `加载全部`，避免首次切页签卡顿
- 预览会截断超长单行的显示内容，避免极端长行在首帧排版时拖慢界面；复制与导出仍保留完整内容
- 标题栏显示当前版本号，并提供 GitHub 开源仓库入口
- 最低完整支持窗口为 `1180×720`；左栏默认 300px（可调 280–360px），中栏默认 280px（可调 260–340px），右栏至少保留 560px
- Windows / macOS 支持品牌化自定义窗口标题栏；Linux 在不支持 client-side decorations 的环境会自动退回系统窗口装饰
- macOS 可通过设置环境变量 `CODEMERGE_SYSTEM_TITLEBAR=1` 强制退回系统标题栏，绕过不稳定的自定义标题栏环境

## 架构概览
- `src/application/*`
  - `WorkspaceStore` 是工作区会话状态的唯一事务入口；状态按 draft / execution / preview / navigation 分片，统一通过 Action、Reducer、Effect 和 `ChangeSet` 演进
  - `WorkspaceCoordinator` 只解释 effect，`TaskSupervisor` 统一管理 Tokio 任务、`JobId`、取消和终态回传；阻塞文件与 CPU 工作通过 `spawn_blocking` 执行
- `src/domain.rs`
  - 稳定领域类型与默认配置
- `src/processor/*`
  - 文件遍历、读取、压缩、合并、统计
- `src/services/*`
  - 预检、处理、预览、结果复制、树构建、树索引、配置加载保存
- `src/ui/*`
  - Workspace 仅保留焦点、单 Store、Coordinator、View 集合和订阅；GPUI 控件、delegate、滚动句柄及渲染缓存归各 View 所有
  - Pane 使用固定 `ChangeSet` 依赖掩码订阅 Store；无关分片变化不刷新，空闲时没有后台轮询
  - 三栏桌面工作区分别承载输入与处理、状态与活动、结果与规则；主操作固定在中栏，右栏通过一级页签在结果和规则间切换
  - 状态栏通过“概览 / 统计”页签分离实时进度与结果分析；分组统计在处理完成时生成，渲染阶段不会重新遍历或读取文件
  - 结果区采用“结果面板容器 + 树面板视图 + 预览面板视图”拆分，避免滚动和树交互放大到整块结果区重绘
  - 目录树采用“过滤投影缓存 + 可见行重建”结构，展开/折叠不再重复做全量过滤投影
  - 文本树与结构树共享过滤投影并使用虚拟列表；搜索采用 75ms latest-wins 防抖
  - `ProcessResult` 由 `Arc` 共享，完成、树、预览、复制和导出路径不再深拷贝完整结果；输入面板和活动投影按 revision 缓存
  - 预览面板把可见范围桶状态保留在局部 view，`PreviewModel` 只保留文档/chunk/请求等业务状态
  - 预览表缓存与 table delegate 共享筛选后的行集合，避免在大结果下为同步重复复制完整路径元数据
- `src/utils/*`
  - i18n、配置存储、临时文件、路径辅助

## 本地运行
```bash
cargo run
```

Windows、macOS、Linux 都纳入了构建链路；CI 当前在 Ubuntu 与 macOS 上持续验证。
Ubuntu/Debian 构建需额外安装 `libxkbcommon-x11-dev`，否则 `gpui` 在链接阶段会报缺少 `-lxkbcommon-x11`。
Linux 是否显示完整自定义标题栏取决于桌面环境对 client-side decorations 的实际支持；如果运行时仍返回 server decorations，应用会退化为紧凑页内 header，以避免双标题栏。

## 键盘操作

- `Ctrl/Cmd+Enter`：开始处理
- `Ctrl/Cmd+.`：取消任务
- `Ctrl/Cmd+F`：聚焦当前结果搜索
- `Ctrl/Cmd+Shift+C`：复制当前活动结果
- `Ctrl/Cmd+S`：导出结果
- `Ctrl/Cmd+,`：切换到右栏持久规则页签
- `Escape`：关闭当前 Dialog 或临时浮层并恢复工作区焦点

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
  - Store dispatch、进度批次数与批内事件数
  - 活动记录队列峰值
  - 子视图条件刷新次数
  - `sync_tree()` 次数与 `tree.set_items()` 次数
  - 输入缓存重建、树过滤投影重建次数
  - Copy job 启动与复制进度批次数
  - `sync_preview_table()` 次数
  - 预览 range 请求次数

## 配置与数据
- 配置文件：
  - Windows: `%APPDATA%/codemerge/config.json`
  - macOS: `~/Library/Application Support/codemerge/config.json`
  - Linux: `~/.config/codemerge/config.json`
- 临时结果：
  - 系统临时目录下的 `codemerge/`
  - 处理中目录由生命周期守卫持有；取消、失败或任务被强制终止时立即回收，成功结果则在切换、重置或退出时统一清理
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
