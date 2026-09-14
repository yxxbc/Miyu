# GQY 全面改名与顾清影默认人格（2026-09-14）

> 交接文档：会话随时可能中断，接手时**先读完本文，再按「阶段总览」里第一个 ☐ 继续**。
> 每完成一步就把 ☐ 改成 ☑，并在「进度日志」追加一行。状态以代码与本机实际为准，动手前先核对。

## 一、目标（已与用户确认的决定）

| # | 决定 | 用户答复 |
|---|---|---|
| D1 | 项目从 Miyu 全面改名为 GQY / 顾清影 | 全部改，**含对外接口** |
| D2 | 命令 `miyu` → `gqy`，`miyu-voice` → `gqy-voice`，crate 名 → `gqy` | 是 |
| D3 | 数据目录 `~/.miyu` → `~/.gqy`，环境变量 `MIYU_*` → `GQY_*` | 是，**不保留旧名兼容** |
| D4 | 与上游 SHORiN-KiWATA/miyu-agent | **独立分叉**，不再合并上游 |
| D5 | 命名规则 | 代码标识符与英文文案 `Miyu`→`GQY`、`miyu`→`gqy`、`MIYU`→`GQY`；**中文句子里的 Miyu → 顾清影** |
| D6 | 历史文档（docs/releases、docs/plan*、docs/fixed） | **全部改** |
| D7 | 顾清影成为**内置默认人格**（`active_persona` 置空） | 是 |
| D8 | TUI logo 艺术字 | 写 `GQY`，副标题 `A G E N T` **不改** |
| D9 | 默认头像 / 看板图 | 用顾清影人格设置里现在那两张（见 §三.5） |
| D10 | 内置 Miyu 表情包 `src/memes/miyu` | **删掉**；默认人格改用顾清影自己的表情包库 |
| D11 | 顾清影数据 | **先完整备份到桌面**，再迁入 `default` |
| D12 | 旧 Miyu `default` 人格数据 | 挪进备份目录归档，库内 scope 改名 `miyu-legacy`（可恢复） |
| D13 | 提交策略 | **只提交到本地 gqy 分支，不推送**；按阶段分开提交 |

## 二、当前状态（写本文时）

- 分支 `gqy`，已提交：`841b55f8`（删 GQY.md 注入）、`57e6691d`（macOS 修复，测试 2338 全绿）。
- **工作区未提交**（D13 未决）：
  - 迁移补全：`src/config/persona_paths.rs`（`degenerate_persona_scope_rename` + 6 类目录）、`src/web/server.rs`（启动时库内迁移）、`src/state/conversation_db/sessions.rs`（`rename_persona_scope` 补 REPL 指针与表情包引用）、测试 `src/config/tests/paths.rs`、`src/state/tests/sessions.rs`、注释 `src/tools/scripts/index.rs`
  - Mac 语义检索：`src/embedding/worker.rs`、`src/platforms/plugins/renderer/worker.rs`（RLIMIT_AS 只在 Linux 设）
  - 以上**均未跑测试、未构建**
- 本机已安装 `~/.cargo/bin/miyu` 0.6.0（**不含**工作区修改）；`miyu-voice` 仍是 09-13 旧版；0.5.1 备份 `~/.miyu/bin-backup/miyu-0.5.1`
- 本机数据已手动完成 `md` → `persona-a37fae32f007` 迁移（脚本 `~/.miyu/bin-backup/migrate-persona-scope-md.sh`，库备份 `~/.miyu/bin-backup/persona-scope-md-*/`）
- 顾清影当前是自定义人格：`~/.miyu/data/prompts/顾清影.md`，清单 `~/.miyu/personas/persona-a37fae32f007/persona.toml`（22 内置脚本 + macos_news）
- daemon 运行中（旧 `miyu`）

## 三、摸底事实（动手时直接用）

1. **根目录**：`src/paths/mod.rs:44` `miyu_home_dir()` 与 `:117` 读 `MIYU_HOME`，默认 `home_dir().join(".miyu")`；运行时 socket `state/miyu/core.sock`（`src/paths/mod.rs:477-487`）。
2. **规模**：Rust 类型 `MiyuPaths` 883 处；`miyu_*` 标识符 18 个/171 处；`MIYU_*` 环境变量 63 个/277 处；`.miyu` 路径 65 处；JS `window.Miyu*` 19 个/91 处；文件名含 miyu 56 个；shell hook 标记 16 处；调用 `miyu` 命令 183 处。非 src：packaging 36、docs 75、web 33 个文件。
3. **内置默认人格**：`build.rs:52-61` 把 `src/prompts/miyu.md`、`miyu.hint.md`、`miyu-dialogs.md` XOR+base64 编进 `OUT_DIR/default_miyu_prompt.rs`；`src/prompts.rs` 解码。`active_persona` 为空时 `base_system_prompt` 用它（`src/config/persona_paths.rs:53`）。**改默认人格必须重新构建安装**，否则切到默认会变回 Miyu。
4. **手写提醒/预设对话**：`<prompts_dir>/hints/<scope>.md`、`dialogs/<scope>.md`（`src/persona_hint.rs:64,106`）；顾清影没有手写文件，自动蒸馏版在 `~/.miyu/state/persona-hints/persona-a37fae32f007.91df510ea1a0b6b4.md`。
5. **头像/看板**：默认人格硬编码 `/assets/miyu-logo.png`、`/assets/miyuwallpaper.png`（`src/web/persona.rs:164`），与顾清影的**不是同一张**。她的：`~/.miyu/data/persona-avatars/300b531ac897034adc4450b78002b004a805b31f2298c73a33df62d2dcb95d22.png`（头像）、`344758be3fbce6a057aa1df95b2a0ab8fc6e6d852391717fdb7e752740199972.png`（看板）。
6. **表情包库**：默认人格库名 `plugins.memes.persona_libraries["default"]`，缺省回退 `"miyu"`（`src/config/tool_plugins.rs:737`）。
7. **logo**：`src/terminal/starfield.rs:48` `BANNER_UNICODE`（6 行）、`:58` `BANNER_ASCII`（5 行），副标题 `:85` `"A G E N T"`。
8. **MCP 桥服务名 `"miyu"`**：`src/llm/openai_compatible/antigravity/mod.rs:77`、`claude_code/mod.rs:290`、`codex/stream.rs:330` 及对应测试，改名需全链路一致；`clientInfo.name` 在 `src/tools/mcp.rs:407`、`src/tools/web/api_search.rs:352`。
9. **库内绝对路径**：`conversation.db` 里约 600 处 `~/.miyu`，几乎都是历史对话/工具日志原文（不影响运行）；**会被程序使用的**只有 `artifact_assets.source_key`（1 行）与 `config.jsonc`（6 处）。
10. **顾清影数据清单**（约 65M）：`personas/persona-a37fae32f007` 3.0M、`state/personas/…` 192K、`data/memes/…` 444K、`home/mac/pictures/album/…` 57M、`extensions/scripts/personas/…` 20K、`data/prompts/顾清影.{md,json}`、`data/persona-avatars` 4.1M、`state/persona-hints/persona-a37fae32f007.*.md`；库内 43 会话 / 471 轮。
11. **旧 Miyu default 数据**：`personas/default` 356K、`state/personas/default` 72K；库内 7 会话、3 绑定、2 指针（current/repl）；无表情包/图库/脚本目录。
12. **已知 bug（待顺手修）**：`src/tools/scripts/dashboard.rs:13` `LAYER_LABELS` 只有 4 个，自定义人格扫 5 个根 → 脚本面板越界 panic（main 上同样存在）。
13. **agy 慢**：Antigravity 续用自身会话，输入逐轮累积、缓存 0、high 思考档；与迁移无关。缓解：新会话 / `/compact` / 换 medium。
14. **自动审批**：改库、移动数据目录的命令会被 Claude Code 自动审批拦下 → 数据类操作写成脚本，由用户 `! bash <脚本>` 执行。

## 四、阶段总览

| 阶段 | 内容 | 完成 |
|---|---|---|
| P0 | 写本交接文档 + 记忆指针 | ☑ |
| P1 | 处理 D13（两批未提交修复） | ☑ |
| P2 | 源码：顾清影成为默认人格（§五.P2） | ☑ |
| P3 | 源码：logo GQY、默认头像看板、删内置表情包、修脚本面板 | ☑ |
| P4 | 源码：全量改名 miyu→gqy（含命令、目录、环境变量、crate、打包、文档） | ☐ |
| P5 | 构建 + 全量测试（重新生成工具注册表夹具） | ☐ |
| P6 | 写数据迁移脚本（桌面备份、~/.miyu→~/.gqy、默认人格迁移） | ☐ |
| P7 | 用户执行迁移脚本；安装 gqy / gqy-voice；重装 shell hook；启动 daemon | ☐ |
| P8 | 验收：人格、脚本、图库、会话、语义检索、TUI logo、WebUI | ☐ |

## 五、各阶段细节

### P2 顾清影成为默认人格
- ☑ `src/prompts/miyu.md` 内容换成 `~/.miyu/data/prompts/顾清影.md`（原样照搬；文件名 P4 改 `gqy.md`，同步 `build.rs`）
- ☑ `miyu.hint.md` 换成 §三.4 的蒸馏提醒正文；`miyu-dialogs.md` 清空（顾清影无预设对话）；`persona_hint.rs` 两个依赖内置对话条数的测试改为不绑定内容
- ☑ `src/cli/setup.rs:98,100` 默认人格标签 → 顾清影（内置默认）/ GQY (built-in default)；OOBE 其余文案随 P4 文案替换
- ☐ `src/token_estimate.rs:107` include 路径（文件名未改，P4 随改名处理）
- 备注：Miyu 原内置提示词里的工具规则（来源标注、开发委派子代理、删除前确认等）顾清影.md 没有，本次未合入，待用户决定

### P3 身份素材与小修
- ☑ logo：`BANNER_UNICODE`（27×6）/ `BANNER_ASCII`（21×5）画成 GQY，副标题不改
- ☑ `web/assets/miyu-logo.png`、`miyuwallpaper.png` 换成 §三.5 两张（文件名 P4 改 gqy-*）
- ☑ 删除 `src/memes/miyu/`（37 文件）；`library_for_persona` 默认回退 `"miyu"`→`"gqy"`（`tool_plugins.rs:743`）及测试 `memes/mod.rs:397`；`packaging/common/assets.json` 删去 memes 条目
- ☑ `dashboard.rs`：`labeled_roots` 按目录本身标层，`directories` 按真实目录输出
- 注意（P7）：用户数据里默认人格表情包目录应为 `data/memes/gqy`；`~/.cargo/share/miyu/memes` 已无内置库

### P4 全量改名（机械替换后靠编译器兜底）
- 规则见 D5。建议顺序：文件/目录改名（`git mv`）→ 标识符（`MiyuPaths` 等）→ 环境变量 → 路径与命令字符串 → 文案（按行判断是否含中文决定 GQY / 顾清影）→ 文档（含历史文档，D6）
- 必改清单：Cargo.toml 包名与两个 `[[bin]]`、`Cargo.lock`（去掉 `--locked` 构建一次刷新）、`~/.miyu`→`~/.gqy`、`MIYU_*`→`GQY_*`、`state/miyu/core.sock`、shell hook 标记与 `fish/conf.d/miyu.fish`、MCP 服务名、`clientInfo.name`、`window.Miyu*`、`packaging/**`、`.github`、README / AGENTS.md / docs
- 注意：工具描述有「不含中文」的测试规则（commit 69ae92ba），英文里只能写 GQY
- web 前端待一起改的硬编码：`web/settings-schema.js:2167`（人格模式选项值 `"miyu"`/「内置 Miyu」，与后端 route persona mode 同步改）、`web/settings.js:2403,2499`（同上）、`web/app.js:10563`（登录用户名占位 `miyu`→`gqy`）
- ☐ 文件/目录改名 ☐ 标识符 ☐ 环境变量 ☐ 路径/命令/协议字符串 ☐ 文案 ☐ 文档 ☐ 打包/CI

### P5 构建与测试
- ☐ `cargo build`（先不带 `--locked` 刷新 lock）☐ `cargo test --lib` 全绿 ☐ `cargo test write_registry_shape_fixture -- --ignored` 重生夹具后复测 ☐ release 构建 `gqy` 与 `--features voice --bin gqy-voice`

### P6 数据迁移脚本（写到 `~/.miyu/bin-backup/migrate-to-gqy.sh`，由用户执行）
1. 停旧 daemon（`miyu daemon stop`），确认无进程
2. 桌面备份：`~/Desktop/顾清影数据备份-<时间>/`，含 §三.10 全部内容 + 整个 `conversation.db(-wal,-shm)` + `config.jsonc`
3. `mv ~/.miyu ~/.gqy`；改写 `config.jsonc` 与 `artifact_assets.source_key` 里的 `/.miyu/` → `/.gqy/`
4. 默认人格迁移：旧 `default` 目录挪进 `~/.gqy/bin-backup/miyu-legacy/`；库内 `default`→`miyu-legacy`（`rename_persona_scope` 同款 SQL，补 REPL 指针与表情包引用）；`persona-a37fae32f007`→`default`（目录 6 类 + 库）；表情包目录改到新默认库名；删 `personas/default/persona.toml`（默认人格不需要）；`config.jsonc` 里 `prompt.active_persona` 置空
5. 完整性检查 + 计数核对

### P7 安装切换
- ☐ 安装 `~/.cargo/bin/gqy`、`gqy-voice`，移除旧 `miyu`、`miyu-voice`（旧的留备份）
- ☐ 资源目录 `~/.cargo/share/miyu` → `~/.cargo/share/gqy`（字体/模型/脚本，**不含**已删的 miyu 表情包）
- ☐ 用户执行 P6 脚本 ☐ 重装 shell hook（清掉旧 miyu hook）☐ `gqy daemon start`

### P8 验收
- ☐ 默认人格是顾清影（说话风格、名字）☐ 22 内置脚本 + 自有脚本可用 ☐ 图库 22 张 ☐ 43 个会话可见 ☐ `gqy embed status` 语义检索可用 ☐ TUI logo 显示 GQY ☐ WebUI 头像/看板 ☐ 脚本面板不再 panic

## 六、进度日志

- 2026-09-14 P0：写本文档；D1–D12 已确认，D13 待用户澄清。
- 2026-09-14 P1：D13 = 本地提交不推送。两批修复分别提交（Mac RLIMIT、迁移补全），均**未跑测试**，留待 P5 一并验证。
- 2026-09-14 P2+P3：默认人格内容换成顾清影、logo GQY、默认头像看板、删内置 Miyu 表情包、脚本面板越界修复。仅 rustfmt 检查，**未编译未测试**。
