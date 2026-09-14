# 上下文圆环:点击查看分项占用

> 总览:`main-tasks.md`。来源:2026-09-14 用户追加。遵循 `docs/理念.md`。
> 未跑测试,下文是读码推理;`未实测` / `未核实` 处施工时补数字。
>
> 结论先行:
>
> 1. 现在圆环只拿到**一个总数 + 窗口大小**,分项数据后端根本没有产出,前端做不出来——主体工作在后端。
> 2. 总数有两种来源:**供应商实测**(上一次请求的真实占用)与 **o200k 估算**。分项只能从估算里拆出来,
>    两者之差**不能按比例摊进各分项**——那是编数字。差额单列一行「分词器差异」。
> 3. claude-code / codex / agy 这类中转后端,Miyu 看不到 CLI 自己的系统提示词和原生工具;能给的是
>    「Miyu 发过去的部分」分项 + 「CLI 自带」**推算**一行,标明是推算，但是市面上有这类中转后段的真实数据获取。

## 0. 用户裁定(2026-09-14)

| 问题 | 裁定 |
|---|---|
| 圆环 | 目前没有交互,**点击后查看详细分项**(参照 Claude Code `/context` 的样式:Messages / 工具 / Skills / System prompt / 自动压缩缓冲 / 剩余空间 / 未加载工具) |
| 中转后端 | agy、claude-code、codex 这类**也要支持查看** |

## 1. 现状(读码)

| 事实 | 位置 |
|---|---|
| 圆环是一个 `div role=progressbar`,没有点击处理 | `web/index.html:287`、`web/app.js:1753` |
| 后端只下发 `context_tokens` / `context_window` / `context_window_assumed` | `src/web/sessions.rs:1039`、`src/web/turns/task.rs:840` |
| 占用 = 有锚点用锚点(上一回合最后一次请求的供应商实测 prompt + completion),否则 o200k 估算 | `src/agent/context_meter.rs:50` |
| 估算 = 渲染整份请求的消息 + 工具定义,再数 token;**不分项** | `src/agent/context_meter.rs:23` |
| 自动压缩两档水位:`trim_at_ratio` 0.8 压缩、`compact_force_ratio` 0.9 强制 | `src/config/defaults.rs:432`、`:436` |
| 工具按 `LoadPolicy` 分 Summary(stub 常驻)/ Group / Hidden | `src/tools/registry/mod.rs:582` |
| MCP 工具单独登记 | `src/tools/mcp.rs` |
| 已有「tools 数组真实 o200k」量尺测试 | `src/tools/mod.rs:1453`(`token_diet_baseline`) |
| 中转后端解析了单次请求用量,但只有 input / output / cache 总数 | `claude_code/stream.rs:237`、`codex/stream.rs:462`、`antigravity/stream.rs:442` |

## 2. 设计

### A. 分项口径(后端)

**只从发给模型的那份请求里拆,不另写一套渲染。** 分项函数吃 `chat_messages("", "")` 已经组装好的消息与 `tool_definitions`,
按角色与标记归类后逐段计数——AGENTS.md §1.1「序列化路径唯一」:分项与真实请求出自同一份字节,不会各说各话。

| 分项 | 内容 | 在不在上下文里 |
|---|---|---|
| 系统提示词 | system 侧:人格 / dev 提示词、system 侧注入 | 在 |
| 工具 · 常驻 | 完整 schema 常驻的工具 | 在 |
| 工具 · stub | 懒工具的「真名 + 摘要 + 宽松参数壳」 | 在 |
| MCP 工具 | MCP 服务器登记进来的工具定义 | 在 |
| 技能 | 已加载的技能正文 | 在 |
| 压缩摘要 | compact 产生的摘要行 | 在 |
| 化石化瞬态 | runtime、联想记忆、元数据等回放的 `context_messages` | 在 |
| 消息历史 | 用户消息、她的回复、工具调用与结果 | 在 |
| 自动压缩缓冲 | `窗口 × (1 − trim_at_ratio)`,到 80% 就开始压缩,这段实际用不到 | 占位 |
| 剩余空间 | 窗口 − 已用 − 缓冲 | — |
| 未加载工具(完整契约) | `load_tools` 能展开但还没展开的完整 schema | **不在**,显示为 `—` |

**实测与估算不一致时**:

- 顶部总数用实测(与圆环一致),标「实测」。
- 各分项用估算,标「估算 · o200k」。
- 多一行 **「分词器差异」= 实测 − 估算合计**,可正可负。不按比例摊进各分项。
- 没有锚点(刚压缩完、回合被打断)时总数也是估算,差异行不显示。

### B. 接口

`GET /api/sessions/{id}/context` →

```
{ window, window_assumed, measured_tokens?, estimate_tokens,
  categories: [{ key, tokens, in_context }],
  thresholds: { trim_at_ratio, compact_force_ratio },
  backend: { kind: "native" | "claude_code" | "codex" | "antigravity", cli_overhead_tokens? } }
```

- **点开才算**,不随每轮事件推:渲染整份请求 + 数二十万 token 有成本。`未实测` 耗时,施工第 1 步量;超过 200ms 就按回合序号缓存。
- 会话归属校验:与 `job-control.md` §A 抽出的判定函数同一个。

### C. 中转后端

Miyu 通过 MCP 把工具交给 CLI,CLI 再叠上自己的系统提示词与原生工具,Miyu 看不到这部分。

- 分项照 A 算「Miyu 发过去的部分」。
- 多一行 **「CLI 自带(推算)」= 单次请求实测 input − Miyu 估算合计**,标明推算、包含分词器差异。
- 窗口大小:CLI 实际窗口与 Miyu 配置可能不同。能从事件里拿到就用事件里的,拿不到沿用配置并标「按配置」。
- 能不能拿到权威分项,逐个核实,**核实前不写进 UI**:
  - claude-code:CLI 有 `/context`。`未实测`:在 `-p` + stream-json 的持久会话里发 `/context` 是否返回分项、**会不会进会话历史或改变缓存前缀**。会动前缀就不用。
  - codex:`未核实` `exec --json` 是否有带窗口信息的 token 事件;现在只解析了 `usage`。
  - agy:`未核实`。

### D. 前端

- 圆环改成 `<button>`,点击开弹窗;手机上是底部面板。
- 头部:`222.9k / 1M` 为主,`22.3%` 为辅——理念 §八:绝对值不需要解释,百分比会说谎。旁边标「实测」或「估算」。
- 一条堆叠条 + 分项列表(名称 · token · 占窗口比例);不在上下文里的行显示 `—`。
- 底部一行:`到 80% 自动压缩 · 90% 强制`,读 `thresholds`,不写死。
- 中转后端时列表上方一行说明:「CLI 自带部分为推算,含分词器差异」。

## 3. 理念对照

| 理念 | 落到这里 |
|---|---|
| 前缀即契约 · 序列化路径唯一 | 分项从真实请求的同一份消息拆出,只读,不改变任何请求字节 |
| 可测量才可优化 | 绝对值为主;实测 / 估算 / 推算三种来源各自标注 |
| 不编数字(报错不是证词的同一种纪律) | 实测与估算之差单列,不按比例摊;中转后端 CLI 部分明说是推算 |
| 先证明不修时现象出现 | 第 1 步的测试断言「各分项之和 = `context_tokens_estimate()`」,先让它在不完整归类时报红 |

## 4. 裁定(2026-09-14)

1. **弹窗**,贴圆环上方。
2. **加「立即压缩」按钮**。
3. **列「最占空间的前 5 条」**,与分项列表之间要有**明确分界线**(它们是分项内部的明细,不能和分项并列求和)。
4. 中转后端:用户记得这类客户端本地有**准确数据**,且已有人做了读取工具。施工第 4 步改为先调研本地数据源,不先实测 `/context`:
   - claude-code:`~/.claude/projects/<项目>/<会话>.jsonl` 每条 assistant 消息带 `usage`(ccusage 等工具读的就是它)。`未核实` 是否有分项。
   - codex:`~/.codex/sessions/**/rollout-*.jsonl` 里的 `token_count` 事件,`未核实` 是否带 `model_context_window`。
   - agy:`未核实`。
   读本地文件不发请求,天然不动前缀——比发 `/context` 安全。

## 5. 前端原型(2026-09-14)

用户要先看前端调试:单文件原型放在 `~/Desktop/miyu-context-panel.html`(**不进仓库**),假数据,样式取自 `web/styles.css` 的真实 token。
调试定稿后再把结构与样式搬进 `web/`(独立文件),接第 1 步的真接口。

## 6. 施工拆分

| 步 | 内容 | 动的地方 | 验证 |
|---|---|---|---|
| 1 | 分项函数 + 接口 | `src/agent/context_breakdown.rs`、`src/web/context_panel.rs`、`src/tools/registry/`(`presented_definitions`) | 各分项之和 = 估算总数;量耗时 |
| 2 | 圆环按钮 + 弹窗 + 立即压缩 | `web/contextpanel.js`、`web/index.html`、`web/app.js`、`web/styles.css` | 原生会话 / 刚压缩完 / 窗口未知三种状态 |
| 3 | 中转后端推算行 | `src/web/context_panel.rs`、`web/contextpanel.js` | claude-code、codex、agy 各跑一轮,推算值量级合理 |
| 4 | 中转后端本地准确数据(见 §4 第 4 条) | `src/llm/openai_compatible/*` | 先调研本地文件格式,读文件不发请求 |

日期见总览 §2:1–3 于 **2026-09-14** 施工(用户裁定提前),2026-09-15 验收;4 未排期。

## 7. 施工记录(2026-09-14)

**做了什么**

- 后端 `Agent::context_breakdown`(`src/agent/context_breakdown.rs`):
  - 消息从 `chat_messages("", "")` 逐条归类:system 与预设对话 → 系统提示词;`transient_context` → 化石化瞬态;`load_skill` 的结果 → 技能;`<conversation-checkpoint>` 开头 → 压缩摘要;其余 → 消息历史。
  - 工具从 `ToolRegistry::presented_definitions` 按形态分 full / stub / MCP。`definitions` 与 `stub_definitions` 改成从它出,分项与真实工具数组同一份字节。
  - MCP 识别靠展示名前缀,前缀抽成常量 `MCP_DISPLAY_NAME_PREFIX`,注册与识别共用。
  - 「占用最多」取历史里单条最大的 5 条;回合归属用同一个 `push_history_turn` 重渲对位,对不上就留空。
- 接口 `GET /api/sessions/{id}/context/breakdown`(`src/web/context_panel.rs`):按会话装配 Agent(同 `session_state_for`),返回分项、实测、窗口、水位、后端类型、中转推算值。耗时写 `context breakdown` 日志行。
- 前端 `web/contextpanel.js`:原型定稿版。圆环从 `div` 改成按钮;弹窗挂 dock;「立即压缩」走现有 `/api/conversation/compact`(回合进行中禁用,先确认再压),成功后与 `/compact` 命令同一条重拉路径。

**验证(已跑)**

- `cargo check --all-targets` 通过。
- 新测试 `context_breakdown_sums_to_the_estimate`:分项之和 = `context_tokens_estimate()`;技能、MCP、回合归属、排序各一条断言。通过。
- `agent::tests::context` 与 `tools::registry` 全部 50 条通过。
- `node --check` 两个 JS 文件通过。

**没验证(`未实测`)**

- 真 daemon 上的弹窗渲染与耗时:要重新构建二进制(web 资源编进二进制,AGENTS.md §7.1)。
- 中转后端的推算值量级。
- `test_scripts/refactor-check.sh` 全量门禁:2026-09-14 跑到全量测试阶段时被停(当时有另一份同名门禁并行抢资源,日志里 `origin_tty_gates_and_writeback_against_real_pty` 失败一次,是否由并行导致 `未核实`)。全量结果没拿到,验收前单独重跑一次。

**相关文档(2026-09-14 已同步)**

- `docs/wiki/03-使用方式.md` §4 WebUI:圆环可点、分项弹窗、立即压缩。
- `docs/wiki/10-系统设计与架构.md` §6:上下文怎么计量、分项口径。
- `next-release-note.md`:按 AGENTS.md 开头「验收成功才写进 next release note」,**2026-09-15 验收通过后再写**。

**验收流程(2026-09-15)**

1. 重新构建并重启 daemon,打开 WebUI 任一有历史的会话。
2. 点输入框下方的上下文圆环:弹窗出现在圆环上方;按 Esc、点弹窗外、点 ✕ 都能关。
3. 头部总数应与圆环数字一致;有实测时标「实测」,并出现「分词器差异」一行。
4. 分项里「消息历史」应是大头;调用过技能的会话「技能」非零;配了 MCP 的「MCP 工具」非零。
5. 分界线下列出最多 5 条,标出回合号;数值不应超过「消息历史 + 技能」。
6. 点「立即压缩」→ 出现确认 → 确定:显示「已压缩 X → Y」,圆环数字同步下降。回合进行中时按钮是灰的。
7. 切到另一个会话:弹窗自动关闭。
8. 手机宽度打开:变成底部面板。
9. 切到 claude-code / codex / agy 会话:出现后端徽标、说明条与「CLI 自带(推算)」一行。
10. daemon 日志里找 `context breakdown` 行,记下 `elapsed_ms`,回填本节。
