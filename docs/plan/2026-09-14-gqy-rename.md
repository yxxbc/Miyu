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
| P4 | 源码：全量改名 miyu→gqy（含命令、目录、环境变量、crate、打包、文档） | ☑ |
| P5 | 构建 + 全量测试（重新生成工具注册表夹具） | ☑ |
| P6 | 写数据迁移脚本（桌面备份、~/.miyu→~/.gqy、默认人格迁移） | ☑ |
| P7 | 用户执行迁移脚本；安装 gqy / gqy-voice；重装 shell hook；启动 daemon | ☑ |
| P8 | 验收：人格、脚本、图库、会话、语义检索、TUI logo、WebUI | ☐（数据/服务侧已验，TUI logo、WebUI 头像、人格说话风格待用户目测） |

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
- **实测细化（P4 执行口径）**：用脚本 `scratchpad/rename_to_gqy.py`（会话临时目录，丢了按下述规则重写）一次替换全部 tracked 文本文件：
  - `MIYU`→`GQY`（环境变量 111 个名，`MIYU_HOME` 245 处）；`miyu`→`gqy`（含复合写法 `miyu-voice`、`.miyu`、`miyuwallpaper`、`miyu_session`…）；`Miyu` 后接字母数字下划线（标识符前缀，`MiyuPaths` 884 处、JS `MiyuDash` 等 19 个）→`Gqy`
  - 独立单词 `Miyu`：**按字符串字面量判断**，不按整行——同一行常见 `t("Miyu daemon stopped", "Miyu daemon 已停止")`。所在引号字面量含中文 → 顾清影；不在字面量里、所在行含中文（注释/文档）→ 顾清影；其余 → GQY。实测含中文行 747 处、不含 1332 处
  - **不改**：外部 URL（GitHub/AUR 等，改了链接失效）、非 UTF-8 文件与第三方 vendor 文件
  - **不改**：不带 URL 的仓库名 `owner/Miyu`、`owner/miyu-agent`、`owner/miyu-bangumi`（实测命中 SHORiN-KiWATA/miyu-agent 32、github/Miyu 10、shorinkiwata/Miyu 6、yxxbc/Miyu 4、SHORiN-KiWATA/Miyu 4、Pictures/Miyu 4、Documents/Miyu 2 等）；**本文档自身排除**（否则规则描述会被改坏）；`test_scripts/__pycache__/` 排除
  - 执行后已知需手修：README 里 clone 地址被保护（仍是 …/Miyu.git），下一行 `cd Miyu` 会被改成 `cd GQY`，两者对不上
  - dry-run 终版：内容改动 706 文件（miyu→gqy 3037、Miyu 标识符前缀 992、MIYU→GQY 928、Miyu→顾清影 733、Miyu→GQY 396，保护 74）；改名 19 文件
  - 文件/目录名同规则 `git mv`（19 个，含 `packaging/arch/miyu*`、`src/prompts/miyu*`、`web/assets/miyu*`、`pics/`、`resources/matugen/miyu-theme.css`、`testkit/**/miyu-package.toml`、`testkit/voice/samples/miyumiyu-ja.wav`）
  - 用户配置里的枚举值（如路由人格模式 `"miyu"`）改名后旧值不认 → P6 迁移脚本要一并改写 `config.jsonc`
- 必改清单：Cargo.toml 包名与两个 `[[bin]]`、`Cargo.lock`（去掉 `--locked` 构建一次刷新）、`~/.miyu`→`~/.gqy`、`MIYU_*`→`GQY_*`、`state/miyu/core.sock`、shell hook 标记与 `fish/conf.d/miyu.fish`、MCP 服务名、`clientInfo.name`、`window.Miyu*`、`packaging/**`、`.github`、README / AGENTS.md / docs
- 注意：工具描述有「不含中文」的测试规则（commit 69ae92ba），英文里只能写 GQY
- web 前端待一起改的硬编码：`web/settings-schema.js:2167`（人格模式选项值 `"miyu"`/「内置 Miyu」，与后端 route persona mode 同步改）、`web/settings.js:2403,2499`（同上）、`web/app.js:10563`（登录用户名占位 `miyu`→`gqy`）
- ☑ 文件/目录改名 ☑ 标识符 ☑ 环境变量 ☑ 路径/命令/协议字符串 ☑ 文案 ☑ 文档 ☑ 打包/CI（脚本一次完成：697 修改、19 改名；待抽查与手修后提交）

### P5 构建与测试
- ☑ `cargo test --lib --no-run`（不带 `--locked`，刷新 Cargo.lock）：**0 错误**，唯一警告是 debug 链接器 `__eh_frame` 过大（改名前就有）☐ `cargo test --lib` 全绿 ☐ `cargo test write_registry_shape_fixture -- --ignored` 重生夹具后复测 ☐ release 构建 `gqy` 与 `--features voice --bin gqy-voice`

### P6 数据迁移脚本（写到 `~/.miyu/bin-backup/migrate-to-gqy.sh`，由用户执行）
- ☑ 脚本已写（默认预演，`--apply` 执行）。☑ 用户预演 ☑ 用户执行（17:31）
- 执行结果：integrity ok；default 44 会话、miyu-legacy 7、残留 persona-a37 0；表情包引用 gqy 3；指针已换；4 类目录到位；config 残留 `/.miyu/` 0、`active_persona` 空；3 个用户脚本已改 GQY_*；桌面备份 `~/Desktop/顾清影数据备份-20260914-173124`（209M）
- 已知小问题：脚本最后一行汇总报 `DESK：未绑定的变量`（`$DESK` 后紧跟全角分号被当成变量名一部分），**只影响收尾提示**，前面各步均已完成
- 迁移后发现：MCP 服务器 **Python venv 里的绝对路径**（`bin/` 下脚本 shebang、activate、`pyvenv.cfg` 的 command）仍指 `~/.miyu`，mi-fitness 启动报 ENOENT → 已对 mi-fitness、bilibili-summary 两个 venv 做路径替换（42 个文件）；替换后 mi-fitness 仍秒退（`ModuleNotFoundError: mi_fitness_mcp`）——它是 **editable 安装**，`site-packages/_editable_impl_mi_fitness_mcp.pth` 与 `direct_url.json` 指向 `~/.miyu/mcp-servers/mi-fitness/` 源码，同样替换后修复。以后搬目录要连 site-packages 里的 `.pth` 一起查
- 实测库内 scope 分布（写脚本时）：顾清影 `persona-a37fae32f007` 会话 44 / 绑定 5 / 表情包引用 3 / 指针 2；旧 Miyu `default` 会话 7 / 绑定 3 / 指针 2
- **陷阱**：`sessions.session_id` 有一行 id 就叫 `default`（88 轮），`app_state` 里有 2 个 value 是 `default`——它们是**会话 id 不是 scope**，脚本只改 `sessions.persona`、`platform_session_bindings.persona`、`platform_meme_refs.library` 与 `*_session_persona:<scope>` 的 key
- 默认人格表情包库名是 `gqy`（不是 `default`），表情包引用 library 与目录 `data/memes/gqy` 都按它；图库、人格脚本目录用 scope `default`
- `config.jsonc`：`active_persona`（第 494 行 `顾清影.md`）、`persona_libraries` 为空；shell rc 与 LaunchAgents 无 miyu 引用 → **不需要重装 hook**；`state/miyu` 运行时目录归档
- **用户侧脚本也要改**（不在仓库里，P4 碰不到）：`iching_divination.py`、`afu_scale.py` 读 `MIYU_ARGS_JSON`（后者还写死 `~/.miyu/data/health`），全局 `macos_news` 读 `MIYU_ARGS_JSON`、`MIYU_SCRIPT_CACHE_DIR`。脚本第 5b 步对 `extensions/{scripts,skills}` 做 `MIYU_`→`GQY_`、`~/.miyu`→`~/.gqy`；桌面备份含整个 `extensions/`
1. 停旧 daemon（`miyu daemon stop`），确认无进程
2. 桌面备份：`~/Desktop/顾清影数据备份-<时间>/`，含 §三.10 全部内容 + 整个 `conversation.db(-wal,-shm)` + `config.jsonc`
3. `mv ~/.miyu ~/.gqy`；改写 `config.jsonc` 与 `artifact_assets.source_key` 里的 `/.miyu/` → `/.gqy/`
   - 实测 `config.jsonc` 需改：MCP 服务器命令/参数与 `output_dir` 的 `/.miyu/` 绝对路径（6 处）；provider 配置**字段名** `miyu_tools`、`miyu_tools_eager` → `gqy_tools`、`gqy_tools_eager`（P4 改了 serde 字段名，旧键会被忽略）；以及任何枚举值 `"miyu"`（如路由人格模式）
4. 默认人格迁移：旧 `default` 目录挪进 `~/.gqy/bin-backup/miyu-legacy/`；库内 `default`→`miyu-legacy`（`rename_persona_scope` 同款 SQL，补 REPL 指针与表情包引用）；`persona-a37fae32f007`→`default`（目录 6 类 + 库）；表情包目录改到新默认库名；删 `personas/default/persona.toml`（默认人格不需要）；`config.jsonc` 里 `prompt.active_persona` 置空
5. 完整性检查 + 计数核对

### P7 安装切换
**⚠ 阻塞（2026-09-14 预演发现）**：`~/.gqy` **已经存在**，不是本轮测试建的——文件时间 2026-08-16 至 08-24，含 `config.jsonc`、`gqy.2026-08-*.log`、剪贴板图片缓存、zsh hook 等（18M、85 文件），像是 8 月一次以 GQY 为名的旧安装。**可能有用户数据，不得擅自删除**；需用户决定（建议先整体改名归档为 `~/.gqy-old-<日期>` 再迁移）。
  - 细节：内含 22 会话 / 105 轮、11 个供应商配置、表情包库 gqy、身份文件 Black Cat.md；`~/.zshrc:467-469` 仍 source 其中的 `zsh-hook.zsh`（调 `gqy --shell-intercept`）；LaunchAgent `com.gqy.qq-tunnel` 只是 autossh 反向隧道 8300→gqy-server，与数据目录无关
  - **用户决定：移到废纸篓**（Finder 删除，清空前可恢复）
  - 迁移后需用新 gqy **重新生成 zsh hook**（`~/.zshrc` 的 hook 块指向 `~/.gqy/config/shell/zsh-hook.zsh`，迁移后该文件不存在，`[ -r ]` 守卫会跳过）
- 旁注（非本次原因）：`src/tools/scripts/tests/index.rs:382`、`tests/mod.rs:17`、`src/tools/jobs/tests/shared.rs:26`、`src/config/tests/provider.rs:1008` 在测试里直接调 `GqyPaths::new()`（随后覆盖字段），存在碰真家目录的隐患，后续可改用临时目录

**执行顺序（关键：迁移前绝不能启动 gqy，否则它会建出空的 `~/.gqy`，迁移脚本守卫会拒绝）**
1. release 构建完成：`target/release/gqy`、`target/release/gqy-voice`
2. 装二进制（只拷文件，不运行）：`cp target/release/gqy target/release/gqy-voice ~/.cargo/bin/`
   - ☑ `gqy` 已装（release 10m41s、63M、`gqy 0.6.0`、只依赖系统库）☑ `gqy-voice` 已装（6m15s、24M、只依赖系统库）
   - 下一步交给用户：第 3、4 步（停 miyu daemon → 预演 → `--apply`）
3. 用户：`miyu daemon stop`
4. 用户：`bash ~/.miyu/bin-backup/migrate-to-gqy.sh`（预演，核对计数）→ `bash ~/.miyu/bin-backup/migrate-to-gqy.sh --apply`
5. 旧二进制挪进备份：`~/.cargo/bin/miyu`、`miyu-voice` → `~/.gqy/bin-backup/`；删 `~/.cargo/share/miyu`
6. 重新生成 zsh hook：`gqy zsh-init`（覆盖 `~/.gqy/config/shell/zsh-hook.zsh` 并刷新 `~/.zshrc` 的 hook 块；旧 `~/.gqy` 已进废纸篓）
7. `gqy daemon start` → P8 验收
- 状态：旧 `~/.gqy` 已移到废纸篓；迁移脚本预演仅剩「旧 miyu daemon 在运行」提示
- **17:11–17:25 意外（已处置，勿重复踩）**：
  - 17:10 装好 `~/.cargo/bin/gqy` 后，17:11:45 有东西跑了一次 gqy（未查明：zsh 的 command_not_found hook 或 Zed 里的 Antigravity 代理）。首次运行的**旧 XDG 布局迁移**把 `~/Library/Application Support/gqy`（08-13）与 `~/Library/Caches/gqy`（08-05）搬进了新建的 `~/.gqy`（178M，含 lover-md 人格、知识库、备份 git 仓库）→ 已原样归档为 `~/.gqy-archive-appsupport-20260914-172451`（未删）
  - 为防再触发，`~/.cargo/bin/gqy` **暂时改名为 `gqy.pending`**；迁移完成后改回。教训：应先完成迁移再把 gqy 放进 PATH
  - 17:18 起仓库被会话外依次切到 `test-pr-origin`（合并冲突）、重置到 origin/main、新建 `feat/webui-selection-assist`（暂存了拆给上游的划词功能 2 个文件）。经用户同意：那 2 个文件存入 `stash@{0}`（"selection-assist PR 草稿"），已切回 gqy（489b10ce，工作区干净）。恢复：`git switch feat/webui-selection-assist && git stash pop`
  - 本地分支名 `gqy` 与远程 `gqy`（yxxbc/gqy-agent）重名，`git checkout gqy` 会报 ambiguous，用 `git switch gqy` 或 `refs/heads/gqy`
  - 迁移脚本预演守卫已全部通过；`miyu mcp-serve`（Zed Antigravity 代理起的）仍握着 `~/.miyu` 日志文件，执行 `--apply` 前需关掉该代理
- ☐ 安装 `~/.cargo/bin/gqy`、`gqy-voice`，移除旧 `miyu`、`miyu-voice`（旧的留备份）
- ☑ 资源目录 `~/.cargo/share/gqy` 已预装（字体 29M、模型 23M、改名后的内置脚本 748K，无表情包）；旧 `~/.cargo/share/miyu` 待切换后删除
- ☐ 用户执行 P6 脚本 ☐ 重装 shell hook（清掉旧 miyu hook）☐ `gqy daemon start`

### P8 验收
- ☐ 默认人格是顾清影（说话风格、名字，待用户实聊）☑ 22 内置脚本 + 自有 2 个脚本到位（用户脚本已用 GQY_ARGS_JSON）☑ 图库 22 张 ☑ 44 个会话归入 default ☑ `gqy embed status` 语义检索 available（Mac RLIMIT 修复生效；记忆向量 969 条缺失，需 `gqy embed reindex`）☐ TUI logo 显示 GQY（待用户目测）☐ WebUI 头像/看板（待用户目测）☐ 脚本面板不再 panic（待用户点开）
- 切换记录：`gqy.pending` 改回 `gqy`；旧 `miyu`、`miyu-voice`、`~/.cargo/share/miyu` → `~/.gqy/bin-backup/miyu-0.6.0-binaries/`；`gqy zsh-init` 已重生成 hook（改前 `~/.zshrc` 备份在 `~/.gqy/bin-backup/zshrc.before-gqy-zsh-init`）；`gqy daemon start` 成功，`gqy-voice` 进程已起；`gqy paths` 全部指向 `~/.gqy`
- 日志核查：`voice-worker.log` 里的 "unsupported config version 3" 与 `daemon.log` 里的 RLIMIT 报错均是切换前旧二进制留下的，本次启动后无新增
- **fish hook 路径 bug（验收时由 `gqy paths` 发现，已修源码）**：`src/paths/mod.rs` 用 `BaseDirs::config_dir().join("fish/conf.d/gqy.fish")`，macOS 上落到 `~/Library/Application Support/fish/…`，fish 只读 `$XDG_CONFIG_HOME/fish` 或 `~/.config/fish`，`gqy fish-init` 装完静默无效（Linux 两者相同所以没暴露，上游同样存在）。改为 `fish_hook_path()`：XDG_CONFIG_HOME（绝对路径）优先，否则 `~/.config`；加单元测试。**不影响本机**：用户默认 zsh、未装 fish。已安装的 release 二进制不含此修复，需重新构建才生效

## 六、进度日志

- 2026-09-14 P0：写本文档；D1–D12 已确认，D13 待用户澄清。
- 2026-09-14 P1：D13 = 本地提交不推送。两批修复分别提交（Mac RLIMIT、迁移补全），均**未跑测试**，留待 P5 一并验证。
- 2026-09-14 P2+P3：默认人格内容换成顾清影、logo GQY、默认头像看板、删内置 Miyu 表情包、脚本面板越界修复。仅 rustfmt 检查，**未编译未测试**。
- 2026-09-14 P4：脚本全量替换（706 文件内容、19 文件改名）。抽查通过：Cargo 包名/两个 bin、build.rs 引 gqy*.md、`default_gqy_*`、`~/.gqy`/`GQY_HOME`、hook 标记、MCP 服务名 gqy。手修 3 类误伤：`src/tools/alarm.rs` 英文工具描述里的「顾清影」→ GQY（字面量含中文示例被误判）；README `cd GQY` → `cd miyu-agent`；`src/web/turns/mod.rs`、`src/web/tests/turn.rs` 注释里引用的「顾清影 is busy」→「GQY is busy」。**未编译**，下一步 P5。
- 2026-09-14 P5：编译 0 错误。首轮测试 2333 过 / 6 败，均为改名或本轮改动的连带：config_tui 3 个（输入字面量含中文被改成顾清影、期望值字面量无中文被改成 GQY，统一为顾清影）；pm 1 个（`shorin/miyu-bangumi` 被仓库名保护、期望值被改名，示例包统一为 gqy-bangumi）；脚本面板 1 个（P3 把 `personas/default` 标成了 builtin，原义是 builtin-persona，已按原义修正）；工具注册表夹具（描述文字变化，已重生成）。复测 2338 过 / 1 败（`config_tui/tests/plugins.rs:207` 同类：`vec!["晚安", "GQY"]` → 顾清影）。另：改名后标识符变短，28 个文件需 `cargo fmt` 重排折行，已统一格式化（fmt 共动 34 文件）。第三轮：**2339 过 / 0 败**，`cargo fmt --check` 通过。随后开始 release 构建 gqy 与 gqy-voice。
