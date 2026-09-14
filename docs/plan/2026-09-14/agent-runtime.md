# Agent 运行时:子代理命名 · Goal 轮数 · 子代理侧栏

> 总览:`main-tasks.md`。设计稿:`../../design/2026-09-14/main-tasks.md`(小节编号与设计稿、总览一致,拆分时保留)。
> 未跑测试,下文是读码推理;`未实测` / `未核实` 处施工时补数字。

## 2. 子代理改名「开发代理 / 任务代理」+ token 贴读秒

### 现状

- 三处文案:工具卡回放 `app.js:7946`、实时工具卡 `app.js:8201`、后台任务条 `app.js:9428`。
- 每步 stats 已有:`subagent_runner.rs:662` 每轮模型调用后 `report_stats`,
  前端 `renderSubagentProgress` 解析后写 `tool-task-token`(`app.js:6294`、`8230`)。
- stats 是**本地化人话**(`工具调用 N 次　消耗词元 ≈1.4k`),前端按前缀剥字符串解析。

### 改法

1. 三处统一:`dev === true` → `开发代理`,否则 `任务代理`。抽一个 `subagentRoleLabel(args)`,别再三处各写一遍。
2. 头部顺序:`[图标] [开发代理] [12s · ≈1.4k] [窥视] [状态]`——读秒与 token 合成一个 span,中间 `·`。
3. 回放:确认 `call.sub_trace` 里有没有存最后一条 stats。有 → 播种时取最后一条填 token;
   没有 → 后端落 sub_trace 时带上(这是唯一可能动后端的地方)。

### 验证推理

- 改名只动显示字符串,`realName`(`tool-technical-name`)仍显示裸名,排障不受影响。
- `report_stats` 的重试提示也借 `__subagent_stats__` 通道(`subagent_runner.rs:588`),
  前端若按「最后一条 stats = token」取值,会取到「连接中断,重试第 1 次…」。回放取值要过滤。

> **问:** stats 要不要改成结构化 JSON(`{"tool_calls":3,"tokens":1400,"estimated":true}`)?现在前端靠剥中文前缀,中英切换或文案一改就断。建议改,顺带解决上一条。
> **问:** 「任务的效果好丑」这句 2026-09-14 从 todolist 删了——是已经满意了,还是挪去别处?
> **拓展:** 后台任务条可以加「本任务占本轮总 token 的比例」,多个子代理并跑时一眼看出谁在烧钱。

## 3. 移除 Goal 轮数限制(2026-09-16 施工)

### 现状

`DEFAULT_MAX_GOAL_ROUNDS = 0`(`goals.rs:135`),驱动器 `goal_driver.rs:255` 已跳过判定,
WebUI 状态行只显「第 N 轮」(`app.js:5214`)。残留:

| 位置 | 残留 |
|---|---|
| `goal/prompt.rs:50`、`:62` | `Round {} of {}` → max=0 时出 `Round 3 of 0` |
| `descriptions/goal.json:33` | `max_goal_rounds`「Optional positive limit」 |
| `goal/mod.rs:140` | 内联 schema 里同一个参数(两份 schema,施工时确认哪份是实际下发的) |
| `goal/mod.rs:168` 起 | 仍解析、仍写库 |
| `goals.rs:123` | `RoundsExhausted` 文案叫人「raise max_goal_rounds」 |
| `goal/command.rs:28`、`goal/mod.rs:73` | 输出里仍带 `max_rounds` |

### 改法

1. 提示词:`Round {n}`,不带分母。不加「(autonomous)」这类新词——现有 full 版已经讲清是自动轮。
2. 两份 schema 删掉 `max_goal_rounds`;`mod.rs` 不再解析,旧调用传了就忽略(不报错)。
3. 库字段 `max_rounds` **保留不迁移**,只读;`RoundsExhausted` 变体与分支删掉,旧库里 >0 的值不再生效。
4. `command.rs` / `render_goal` 输出去掉 max 字段。

### 验证推理

- 改工具描述 = 一次计划内冷启动,AGENTS.md §1.6 认可;提示词里的轮次数字本来就每轮变,
  它在 `<goal_round>` 块里、不在稳定前缀,去掉分母不影响缓存。
- 防跑飞靠:连续空转暂停(`goal_driver.rs` `finish_run`)、`BLOCKED_AFTER_CONSECUTIVE_ROUNDS = 3`
  (`goal/mod.rs:37`)。设计稿说的第三条「armed 只存内存、重启即暂停」**`未核实`**,施工前读一眼。
- 注意 `BLOCKED_AFTER_CONSECUTIVE_ROUNDS` 用的是 `rounds_started`(`mod.rs:255`),它是「允许报 blocked 的最低轮数」,不是上限——删轮数限制不碰它。

> **问:** 轮数不限之后,要不要换一个**花费上限**兜底(本 goal 累计 token / 美元,超了转 paused 等人)?轮数是个坏代理变量,钱才是真正怕跑飞的东西。
> **问:** `/goal` 命令侧要不要保留一个手动 `--max` 给想要上限的人?倾向不留,少一个旋钮。

## 4. 子 Agent 详细行为在侧栏展开

### 现状

- `isSubagentTool` 只认 `subagent` / `task`(`app.js:7738`)。`deep_research` 只拿到了 bot 图标(`app.js:8104`),不是 `is-task`,点开就是参数 JSON——正是用户截图里 `thinking_depth: medium topic: …` 那段。
- 前台子代理已有「四行活区域」(`fd07a99d`,2026-09-12),数据源是 `renderSubagentProgress` 的 sink。

### 改法(分两段)

- **A(前端)**:侧栏新增 `trace` 视图,不算 artifact(不进下拉、不落库),直接订阅同一个 sink。
  卡片头加「在侧栏查看」;四行活区域保持不动。
- **B(后端)**:`deep_research` 目前有没有逐步 progress 事件 **`未核实`**。有 → 接进同一个 sink;
  没有 → 先把 Thinker/Critic 每轮的草稿与工具调用作为 progress 发出来。
- 设计稿里的 `linux_input_method_diagnose` 在 `src/` 里没搜到对应文件,**先不列入**。
- 「左侧点一步、右侧定位」往后放。

### 验证推理

- 复用 sink 意味着侧栏与四行区是同一数据的两个视图,不会出现两边对不上。
- 手机端 `layoutViewportWidth() <= 760` 不自动开侧栏(`app.js:5140`),trace 视图在手机上需要全屏抽屉形态。

> **问:** 侧栏 trace 里要不要显示子代理的**完整 system prompt 与初始 prompt**?对排障有用,但 dev 子代理的提示词里有项目上下文,截图外传时会一起出去。
> **问:** 深度研究的每轮「审视意见」要不要进 trace?这是最有信息量的部分,但也最长。
> **拓展:** trace 视图加「导出为 markdown」,长调研的过程本身就是交付物。

