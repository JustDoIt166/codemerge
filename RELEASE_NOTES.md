# CodeMerge Release Notes

本次发布聚焦于工作区 UI 重构、归档支持与跨平台发布链路整理。

## 更新内容

- 重构主界面信息架构，拆分为输入区、状态区、结果区和规则管理区。
- 新增 Windows / macOS 自定义窗口标题栏，统一品牌区与状态胶囊；Linux 不支持 client-side decorations 时会自动退化为紧凑页内 header。
- 标题栏新增版本号显示与 GitHub 开源仓库入口，便于核对版本和访问项目主页。
- 新增窄窗口退化布局，避免三栏在小尺寸下挤压不可读。
- 将黑名单管理移出首页主流程，支持单条删除与更安全的危险操作确认。
- 优化处理中状态展示，增加摘要卡片、明确状态层和最近活动列表。
- 统一结果区文案与表头本地化，移除预览区面向最终用户无意义的调试信息。
- GitHub Release workflow 改为读取仓库内的 `RELEASE_NOTES.md` 作为发布说明。
- 修复 Linux `.deb` 安装后缺少桌面启动器的问题，现会一并安装 `.desktop` 文件和应用图标。
- 新增 macOS CI / Release workflow，按 `amd64` 与 `arm64` 产出 `CodeMerge.app` 的 `.zip` 和 `.dmg`。
- 新增 `.zip` 压缩包中的文本文件展开支持，可参与目录树、预检、预览和单文件合并。

## 构建产物

- Linux amd64: `.deb` 与 `.tar.gz`
- Windows amd64: `.zip` 与安装包
- macOS amd64 / arm64: `.zip`（`CodeMerge.app`）与 `.dmg`
