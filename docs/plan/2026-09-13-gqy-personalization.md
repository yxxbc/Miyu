# 2026-09-13 个人化分叉:macOS 移植 / 身份改造 / 图库内置化

> 分支 `gqy`,基于 `macos-port`(4078f5aa),后者基于 upstream main `cdf820f9`。
> `macos-port` 只装 macOS 编译修复一件事,保持通用、可单独 PR;个人化全部落在 `gqy`。
> 用户裁定:(1) 改名只改**对外身份**,二进制与 `~/.miyu` 路径不动——纯内部字符串,
> 改了无功能收益却让以后捡上游修复变难,948M 数据还要迁;(2) 代码推**新建的私有仓库**,
> 不推公开 fork `yxxbc/Miyu`;(3) 内置口令写死在源码(私有仓库下可接受,建完管理员即失效);
> (4) 图库**按人格分库**,与表情包同一套口径。
>
> 上游 PR #42(macOS 编译修复)开出 16 分钟后被静默关闭,零评论、零 review,上游 main
> 也没有等价修复。结论:macOS 补丁**不能指望回流**,由本分支长期自持。

## 状态

| 项 | 状态 | 验证 |
|---|---|---|
| 1 macOS 编译修复 | 施工完 | `cargo build --release` 通过、**零警告**;产物 Mach-O arm64 可运行(`miyu 0.5.0`);`tools::sandbox` 单测 3/3 过 |
| 2 高危命令名单扩充 | 施工完(配置层) | 5 条 → 44 条;`miyu config validate` 通过。**待 daemon 重启生效** |
| 3 数据快照与清理 | 施工完 | `miyu export` 259M/124 条目/0600,tar 校验通过;释放 `~/.miyu` 129M + `target/` 1326M;活库 45 会话完好 |
| 4 macos_news 脚本工具 | 施工完 | 四源 283 条,跨源日期格式混排排序正确;`miyu tool-call` 端到端 `success: true` |
| 5 album 图库(脚本版) | 施工完 | save/search/list/show/delete 全通;`MIYU-IMAGE:` 行被投递层摘走 |
| 6 身份改造 | 施工完 | `cargo build --release` 通过、零警告 |
| 7 macOS 语音三处修复 | 施工完 | 真机通过:`devices` 列得出设备、唤醒命中、TTS 回复;`miyu-voice` 新版已装 |
| 8 图库内置化(工具层) | 施工完,**已验证** | 五个动作在新二进制上全跑通(save/search/show/delete 含硬删),活库 22 张完好;见 §16.3 |
| 9 图库 WebUI 面板 | 施工完,**未验证** | `cargo check` 零警告;面板 API 五条 + `web/dash-album.js`;**浏览器里没点过**(要重启 daemon) |
| 10 `/init` → `GQY.md` | 施工完 | 命令进表、dev 提示词与 dev 子代理都注入工作区的 `GQY.md`;**没在真 REPL 里跑过** |
| 11 语音会话模型 | 已诊断,**用户今日裁定先不做** | A/B/C 仍在 §11.2,等下次拍板 |
| 12 提交与推送 | **未做** | 见 §13.4 |
| 13 地图插件 + 地图卡片 | 施工完,工具层已验 | 单测 8/8;`map_search` 正查与逆查**端到端跑通**(见 §16.3);两个瓦片源 curl 实测 200;**卡片没在浏览器里看过** |
| 14 快递插件 + 快递卡片 | 施工完,降级路已验 | 单测 5/5;无 key 时走 `no_credentials`、**不编轨迹**;单号形状守卫生效;真查询要用户配 key(见 §15) |

> **接手先读 §13**(未完成清单)。那里写了每项卡在哪、落点在哪、以及踩过的操作纪律。

---

## 1. macOS 编译修复(`macos-port` 分支)

**根因**:`src/tools/mod.rs:42` 的 `pub mod sandbox;` 无条件参与编译,而 `sandbox.rs`
里的 Landlock 包装用的是 Linux 专有 API,macOS 上 lib crate 8 个错误:

- `libc::prctl` / `libc::PR_SET_NO_NEW_PRIVS` / `libc::O_PATH` —— Linux 专有
- `libc::syscall` 在 BSD/macOS 上是 `syscall(c_int, ...)`,Linux 上是 `c_long`,
  Landlock 的系统调用号(444/445/446)塞不进 `i32`

**改法**:只给 Landlock 那几个 syscall 包装加 `#[cfg(target_os = "linux")]`,非 Linux
补一份等价语义的兜底——`probe() -> None`(与「内核没编 Landlock」同一条路)、
`Rules::apply() -> ENOSYS`(**失败关闭**)。这正是模块开头已经写下的规矩,非 Linux 走的
是既有路径,不是新语义。管理员没有策略,`confine` 压根不挂 `pre_exec`。

纯新增 35 行,原有代码一行未改,**Linux 分支逐字节不变**。测试无需改动:
`member_policy_confines_shell_writes` 本来就按 `probe()` 跳过。

**副产品**:macOS 上多用户成员会话的命令会被拒(无 Landlock → 失败关闭)。会话隔离、
面板 403、记忆隔离、外发禁令、进程内路径守卫全部照常——那些是纯 Rust 逻辑。
降级是干净的:朋友能聊天、能用记忆和知识库,只是不能执行命令。

## 2. 高危命令名单(`config.tools.command_deny`)

**根因**:默认 5 条(`rm -rf /`、`rm -rf ~`、`mkfs.`、`dd if=/dev/zero of=/dev/`、
fork bomb)是 Linux 口径,且是**纯子串匹配**——`rm -fr /`、`rm --recursive --force /`、
`rm -rf $HOME`、`rm -rf "/"` 全都绕得过去。`mkfs.` 在 macOS 上更是无的放矢
(macOS 是 `newfs_apfs` / `diskutil eraseDisk`)。

**改法**:扩到 44 条,分五组——rm 写法变体、macOS 磁盘(`diskutil eraseDisk` 等)、
macOS 安全开关(`csrutil disable`、`spctl --master-disable`、`nvram -c`)、裸写块设备
(`of=/dev/disk`、`of=/dev/rdisk`)、Linux 保留项。

**要记住的边界**:这个闸只管 `run_command` 一个工具(`tools/mod.rs:370` 直接 return),
脚本工具、后台 job、CLI 中转线自带的 Bash 都不过它。而且它防的是**模型手滑**,不是防人
——子串匹配对存心绕的人毫无意义。真正的纵深是「删除默认走回收站」和成员沙盒。

**已知副作用**:`rm -rf /` 作为子串会拦下 `rm -rf /任意绝对路径`,包括正常操作。
这是改动之前就有的行为。被拦了改用 `trash_path`。

## 3. 数据快照与清理

清理前先做全量快照:`miyu export` → 259M(会话 124.7M / 图片 76.2M / 表情包 57.3M /
人格素材 4.1M / 配置 / 知识库 / 脚本),排除 `state/models` 266M 与 `mcp-servers` 191M
(可重新下载的二进制)。**归档含明文 API key,权限 0600,不得外传**。

删除项(均已确认非活数据):

| 项 | 体积 |
|---|---|
| `home/<user>/conversation.db.bak` | 125M |
| `state/conversation.db.bak` | 3.8M |
| `target/debug` | 1326M |

保留:`state/models`、`mcp-servers`、活库、`pictures`、剪贴板缓存、旧布局残留
(`~/.miyu/prompts/`、`~/.miyu/scripts/`、`config/.miyu-pm-backups/`)——用户未勾选。

## 4. macos_news:Arch 新闻的 macOS 对位

`archlinux_news` 干的事(拉 RSS、解析、跟 `last_seen_url` 比对标 `is_new`、写回)
没有一件需要进程内能力,所以做成**脚本工具**,零 Rust。

四个源,单源失败降级不拖垮整体:

| key | 源 | 作用 |
|---|---|---|
| `applesec` | Apple 官方安全发布页 | 哪些版本修了安全问题、什么时候 |
| `eclecticlight` | Howard Oakley | macOS 圈最接近 Arch news 的:这次更新实际改了什么、已知坏在哪 |
| `mrmacintosh` | 企业 Mac 管理员视角 | 通常第一个说「这个先别装」 |
| `apple` | Apple Developer News | 官方发布与弃用 |

**两个坑**:(1) Eclectic Light 的**根 feed 混着大量艺术史长文**(前 5 条里 3 条是新印象派
绘画),`category/macos/feed/` 是空的,`category/technology/feed/` 才是纯 Mac 内容;
(2) 安全页日期是 `18 Aug 2026`,不是 RFC-2822,`sort_key` 要多格式兜底,否则跨源合并
排序全乱。

`is_new` 按源各自记住上次见过的最新一条,状态落 `MIYU_SCRIPT_CACHE_DIR`,与
`archlinux_news` 同一套口径。

## 5. album 图库(脚本版,过渡)

**缺口**:`print_image` 本来就能显示任意路径的图,所以「发图」不缺;缺的是**存和找**
——想留一张图,唯一的库是表情包库,而表情包在需要发反应时是**自动挑**的,塞非表情包
进去会污染它。

脚本版落 `home/<user>/pictures/album/` + `index.json`,五个动作,`show` 输出
`MIYU-IMAGE: <路径> | <说明>` 交投递层。**这是过渡方案**,内置版见第 7 节。

## 6. 身份改造(施工中)

改名只改对外身份。重要发现:**WebUI 品牌名、头像、看板图、输入框 placeholder 已经是
人格驱动的**(`app.js:1241-1256` 从 `state.persona` 填),当前 `active_persona` 就是
`顾清影.md`,所以这一层自动就对。真正要改的只有静态兜底:

| 位置 | 内容 |
|---|---|
| `src/web/security.rs:28-29` | 内置账号 `miyu`/`miyu` → `gqy`/`GQY520` |
| `src/web/persona.rs:166,171` | `active_persona` 为空时的兜底名 |
| `src/web/accounts_api.rs:81,86` | OOBE 里的 `shared.name`(填 `oobeSharedName`) |
| `src/web/voice_bridge.rs:370,639` | 语音通知标题 |
| `src/platforms/plugins/reply_processor/mod.rs:261` | QQ 转发节点显示名兜底 |
| `src/notify.rs` | `notify-send -a`(Linux 桌面通知应用名) |
| `web/index.html` 10 处 / `web/app.js` 2 处 | 首屏静态文案(JS 会覆盖,但有闪烁) |
| `web/assets/{miyu-logo,miyuwallpaper}.png` | 换成人格素材,`sips` 压到 256×256 / 1280×720 |
| `src/config/mod.rs:381` | 唤醒词 → 清影清影 / 顾清影 / qingying |

`index.html:18` 的 `MiyuCommands` 是内部 JS 命名空间,不动。

**唤醒词为什么用叠词**:原默认 `未有未有`/`密友密友` 都是「Miyu」的谐音**叠词**——
叠词音节长、声学特征明显,命中率高误触少。所以对位是 `清影清影`,不是 `清影`。

## 7. 图库内置化 + WebUI 面板(未开工)

用户要「和表情包同等待遇」。`memes` 是 2960 行 Rust + 305 行面板,那么大是因为带了
视觉自动识图、语义向量检索、内置/用户/人格三层库、平台使用计数、校验一整套。
album **不抄这些重装备**:描述自己写或让顾清影写,搜索先用关键词——将来要语义检索,
`embedding` 模块是现成的,再接不迟。

| 部分 | 内容 |
|---|---|
| `src/tools/album/mod.rs` | save / search / list / show / delete |
| `src/tools/album/store.rs` | 元数据进 `conversation.db`,不是 json 文件 |
| `src/tools/descriptions/album.json` | 契约 |
| 接线 | `compose_registry` 一个 `if plugin("album")`、`PLUGIN_IDS` 一行、`tools/mod.rs` 一行 |
| `src/web/dashboards/album.rs` | 面板 API:列表 / 上传 / 改描述 / 删除 |
| 前端 | `index.html` 面板 + `app.js` + `styles.css`,仿表情包网格 |

**按人格分库**(用户裁定):照 `memes::current_persona_library` 的口径,顾清影有自己的
图库;将来再建人格,图不会混。文件落 `home/<user>/album/<persona>/`。

## 8. `/init` → `GQY.md`(未开工)

**缺口**:dev 模式只读全局的 `config/dev-prompt.md`,**没有任何项目级上下文文件机制**。
本仓库根上的 `AGENTS.md` 是写给「开发这个项目的 agent」看的,miyu 自己并不读它。

**改法**:仿 Claude Code 的 `/init` —— 在 `src/slash_commands.rs`(314 行的静态表,
现有 23 条)加一条 `/init`,让 dev 模式扫当前工作区、生成 `GQY.md`(项目结构、构建与测试
命令、约定),之后每回合把工作区的 `GQY.md` 注入 dev 提示词。

**待定**:注入点要不要化石化(见 `docs/理念.md` 的 append-only 口径)——`GQY.md` 会变,
变了就是前缀分叉。倾向:进系统提示词尾部而不是前缀,或者跟 `dev-prompt.md` 同层拼接
后整体作为前缀,文件变更时接受一次计划内的冷启动。

## 9. 语音唤醒在 macOS 上(实测定位)

整套语音栈在 macOS arm64 上**是通的**:`miyu-voice` 已装(说明 `sherpa-onnx-sys` 的
预编译库路径可用——它是从 GitHub releases **下载**预编译库,不需要 cmake、不编 C++),
三个模型都在 `state/models`(KWS 36M + SenseVoice 229M + silero VAD 632K —— 这才是那
266M 的真相,不是 embedding 模型)。

`miyu-voice test` 实测:麦克风打得开(`MacBook Air麦克风 48000Hz 1ch F32`)、VAD 正常
(切得出 0.7~1.4s 的语音段),**卡在 KWS 永不命中**。压阈值到 0.12、加分提到 2.0 仍不中
——说明不是判定门槛的问题。

### 9.1 根因:48k→16k 的裸线性抽取毁掉了擦音

```
macOS: cpal 取 default_input_config() → 48000Hz
    → LinearResampler(48000, 16000) 真的开始工作
    → 3:1 抽取,纯线性插值,无抗混叠低通
    → 8kHz 以上能量整体折叠回语音带
    → q / x / sh 这类擦音的判别特征被毁
    → KWS 永不命中
```

**为什么 Linux 上没事**:走 PipeWire(`PIPEWIRE_NODE`)拿到的就是 16kHz,
`(ratio - 1.0).abs() < EPSILON` 时 `return input.to_vec()` 直通——**这段代码在 Linux
上根本不执行**。典型的「只有换平台才会踩到的路径」。

**改法**(两条一起,一次重编覆盖两种可能):

1. `preferred_input_config()`:开流前问 `supported_input_configs()` 支不支持 16kHz,
   支持就直接要,由 CoreAudio 的 HAL 做带正经滤波器的速率转换,重采样器退化成直通。
   同采样格式优先,免得为了采样率换掉样本类型。
2. 设备不支持 16kHz 时的兜底:抽取前加一道滑动平均(boxcar FIR),窗长取抽取比,
   窗口跨批保留,批边界不留断点。

**真机验证(09-13,MacBook Air 内置麦克风)**:

```
capturing from: MacBook Air麦克风 (48000Hz 1ch F32)
✔ wake word hit, listening…
» 轻盈轻盈。
```

采样率**仍是 48000** —— Apple 内置麦克风的 `supported_input_configs()` 不给 16kHz,
改法 1 没用上。**真正生效的是改法 2 的抗混叠滤波**:同一支麦克风、同一套阈值,
加滤波前 6 次全不中,加滤波后命中。反过来也坐实了根因判断——问题确实在混叠,
不在判定门槛。

**遗留**:命中率不是 100%(那次是第 2 句才中),boxcar 是很粗的低通,换成窗化 sinc
或多相 FIR 会更好;暂不做,够用。阈值/加分现在是 `0.12 / 2.0`,还有下调空间。

**STT 的同音字**:识别把「清影」写成「轻盈」「青影」。这是 SenseVoice 的词表问题,
不是唤醒问题;听写场景里她的名字会被写错。见 §11 待办 1。

### 9.4 唤醒后的连续对话:已有,不用做

`voice.follow_up_seconds`(默认 30)就是免唤醒追问窗口,而且口径是「**从她回复完
起算,每次回复都重新起算**」——只要对话在继续,窗口一直续着,比 Siri 的「每轮重新
唤醒」宽松。`0` = 每句都要唤醒词。

`miyu-voice test` 脱离 daemon,没有「回复」这个事件,所以那里的 `speech onset in
window` 是从唤醒起算的;接上 daemon 后才是回复后起算。

**注意**:`voice.enabled` 目前仍是 `false`,整套没启用。

## 10. 记忆库敏感内容触发供应商内容策略(已诊断,未处置)

**现象**:回合中途 HTTP 400,
`LLM stream failed after emitting output; endpoint failover was suppressed`,
上游文案是 Google 的 Generative AI Prohibited Use policy。

**根因**:文本模型池只有一条 `{"provider_id":"antigravity","model":"gemini-3.8-flash-high"}`
——`antigravity` 是 Google 的 CLI 中转线。人格对话里积累的私密内容随会话历史与记忆
逐字回放给 Gemini,撞它的内容策略。**与 miyu 无关,是选型问题。**

**为什么不回退,以及为什么这是对的**:`chat.rs:364` 的 `attempt_committed` 分支——
模型**已经吐出过 token** 才失败。此时切端点会拼出「半截 + 重复」的回复,所以故意
抑制 failover。改这里是错的方向。

推论:**模型池放多条也防不住这个**。多条只防「连接不上 / 请求未提交就被拒」,
防不了「流开始之后内容被拦」。唯一的解是不用会拦的那家。

**可选供应商**(本机已配 key):deepseek(`deepseek-v4-pro`)、anthropic、minimax、
openrouter、opencodego / 日日新(默认也是 `deepseek-v4-flash`)。

**状态**:用户决定自己挑,暂不改配置。切换入口 `miyu models` 或
`miyu config` →「全局文本模型」。

**要记住的**:敏感内容在会话库与记忆库里,换供应商是唯一的解——除非删历史。
只要还发给 Google,迟早再撞。

## 11. 语音回合的会话模型:三个抱怨,一个根因(已诊断,设计待定)

真机跑通后用户提了三点:回复只出现在 macOS 原生通知里、太长了展不开;语音**不继承
对话**;**记忆差**、也不进会话历史。

**这三件不是 bug,是设计如此。** `voice_bridge.rs` 开头写得很直白:

> 唤醒路:`voice.command` → 在「**语音会话**」lane 起回合 → 完成后**桌面通知回复摘要** + 提示音

数据库坐实:

```
user     45   ← 真正的对话
voice     1   ← 所有语音对话全挤在这一个会话里
subagent  2
```

`resolve_voice_session` 把 id 记在 `state/voice-session-id`,首次创建一个
`VOICE_SESSION_KIND`(`"voice"`)的专属会话,此后一直复用它。而 WebUI 的会话列表走
`resolve_local_session_ref_with_kinds(..., &[USER_SESSION_KIND], ...)`——**只认
`user`**,所以那个语音会话在界面里根本看不到,回复全文连个能读的地方都没有。

于是:历史不共享 → 她「记不住」;界面看不到 → 只剩通知;通知按
`notify_reply_chars`(默认 120)截断 → 长回复展不开。

### 11.1 TTS 其实是接了的,要先排除故障再谈改设计

`"completed"` 分支里先合成再通知,顺序是有讲究的(09-05 的结论:通知先弹会让用户
看到文字却等好几秒才出声,所以合成完再同时发):

```rust
let spoken = voice_tts::spoken_text(&reply, &tts);
let summary = clip(&spoken, reply_chars);
let synthesized = if tts.is_active() && !spoken.trim().is_empty() { … };
notify(state, "顾清影", &summary);
```

所以**播报本该有声**。用户只看到通知,先查 `voice-worker.log` / daemon 日志里有没有
`播报失败`——可能是 MiniMax 那边报错(key、额度、音色 id),而不是没接。

### 11.2 三个改法(未定,按侵入性排序)

**A. 只修输出(最小)**:调大 `notify_reply_chars`;把 `VOICE_SESSION_KIND` 加进
WebUI 会话列表认的 kinds,让语音会话在界面里可见可读。不动会话模型。

**B. 语音并入当前会话(用户想要的)**:`resolve_voice_session` 改成返回**当前活跃的
user 会话**而不是专属 lane。历史、上下文、记忆联想全部自然共享。

代价要说清楚:语音回合会**混进主对话**——短句、识别错字(「轻盈」「青影」)、被打断的
半截回合都会留在正式历史里,且这些都会进入后续每一轮的前缀。对陪伴型人格这是想要的;
对「主对话要干净」是退步。**倾向 B**,但建议同时做 §12 待办 1(识别纠错)降低脏数据。

**C. 折中**:仍用专属 lane,但起回合时把主会话最近 N 轮作为上下文注入。历史共享、
主对话不被污染,代价是多一份注入、且前缀每次都变(与 `docs/理念.md` 的 append-only
口径冲突,缓存会反复冷启动)。**不推荐**。

## 12. 待办

1. **STT 人格名纠正**:识别结果里「轻盈 / 青影 / 清盈」一类同音词纠回「清影」。
   落点在识别之后、进回合之前做一次替换表(人格名及其常见同音写法),别动 SenseVoice
   本身。范围要收紧——只纠人格名,不做通用纠错,否则会把用户真正想说的「轻盈」改掉。
   一个可行的收紧条件:只在该轮文本里没有其他更像人名的实体时纠。
2. **系统扫一遍 Linux 专有假设**:今天撞到三处(§1 Landlock syscall、§9.2 ALSA 的
   `CARD=`、§9.1 靠 PipeWire 给 16kHz 的隐含前提),共同点是**在 Linux 上根本不执行
   那条路径**,作者不可能发现。值得一次性扫 `pactl`、`notify-send`、`/proc`、
   `xdg-open`、`systemd`、ALSA 设备名这类调用,把 macOS 移植补齐。
3. **图库检索升级**:现在是关键词匹配。`embedding` 模块是现成的,要语义检索再接。
4. **重采样器换正经滤波**:boxcar 够用但粗,窗化 sinc 或多相 FIR 会更好(§9.1)。
5. **静音自检(macOS TCC)**:macOS 的麦克风授权按「责任进程」给。daemon 若由一个
   没有麦克风授权的进程拉起,它 fork 的 voice worker 就拿不到授权,而 **CoreAudio
   拒绝时不报错、只给一路静音**——现象是进程活着、流开着、日志照写「语音前端就绪」,
   但永远收不到声音,极难定位(09-13 实际踩到:daemon 由非终端环境重启,唤醒词全无反应,
   而同一台机器上 `miyu-voice test` 从终端跑一切正常)。
   **改法**:采集线程统计开头若干秒的样本能量,全零(或低于极小阈值)持续 N 秒就
   `tracing::warn!` 一条明确提示——「麦克风只收到静音,可能是系统未授予麦克风权限;
   macOS 请从已授权的终端重启 daemon」。只是 warn,不改变行为。
   注意:`听到 X.Xs 语音` 这类事件是 `tracing::debug!`,默认 info 级不落盘,所以
   「日志空白」并不能证明没收到音频——这也是当时难定位的原因之一。

## 13. 未完成清单(交接用)

按优先级排。每项都写了「卡在哪」,接手直接从那继续。

### 13.1 语音会话模型(设计已定型,实现未动)

见 §11.2。**等用户在 A/B/C 里拍板**。倾向 B(并入当前会话)。
落点:`src/web/voice_bridge.rs` 的 `resolve_voice_session`。
先做的前置检查:查日志确认 TTS 是不是真的失败了(§11.1)。

### 13.2 album 工具层与面板(**面板已补,运行时仍未验**)

> 09-13 晚更新:面板做完了(见 §16.1),但下面这段「一次没真跑过」**依然成立**
> ——daemon 还跑着旧二进制,五个动作与面板五条 API 都只编译过、没跑过。
> 装新二进制 + 重启 daemon 仍是接手第一件事,而重启只能由用户自己在终端做(§13.5)。



**已完成并编译通过**:`src/tools/album/{mod,store}.rs`、`descriptions/album.json`、
四处接线(`mod album` / `PLUGIN_IDS` / `include_str` 清单 / `compose_registry`)、
`workspace::expand_path`(新增的公共实现,`memes` 与 `knowledge_base` 各有一份重复,
新代码一律用这个)。**尚未做过任何运行时验证**——二进制编出来了但没装,daemon 还跑着
旧的,五个动作(save/search/list/show/delete)一次都没真跑过。**接手第一件事是验它。**

**面板已补**(09-13 晚):`src/web/dashboards/album.rs` + `web/dash-album.js`,
细节见 §16.1。

### 13.3 `/init` → `GQY.md`(**已实现,未在真 REPL 里跑过**,见 §16.2)

见 §8。`src/slash_commands.rs` 是 314 行的静态表(现有 23 条),加一条 `/init`。
dev 模式目前只读全局 `config/dev-prompt.md`,**没有任何项目级上下文文件机制**。
仓库根上的 `AGENTS.md` 是给「开发这个项目的 agent」看的,miyu 自己不读。

**待定**(§8 已记):注入点要不要化石化。倾向进系统提示词尾部,或与 `dev-prompt.md`
同层拼接后整体作为前缀、文件变更时接受一次计划内冷启动。

### 13.4 提交与推送(全部改动仍未 commit)

分支 `gqy`(基于 `macos-port`)。**今天所有改动都还在工作区,一个 commit 都没打。**

私有仓库已建好并验证是 PRIVATE:`yxxbc/gqy-agent`,remote 名 `gqy` 已加。
**注意**:公开 fork `yxxbc/Miyu` 仍然存在且是 PUBLIC,`GQY520` 这类东西绝不能推到那。

待提交的改动清单(09-13 晚已扩大):
- `src/tools/sandbox.rs`(在 `macos-port` 上已 commit)
- `src/voice/mic.rs` —— 三处 macOS 修复
- `src/config/mod.rs` —— 唤醒词默认值
- `src/web/{security,persona,accounts_api,voice_bridge}.rs`、
  `src/platforms/plugins/reply_processor/mod.rs`、`src/notify.rs` —— 身份改造
- `src/web/tests/session.rs` —— 跟着改的断言
- `web/{index.html,app.js}`、`web/assets/{miyu-logo,miyuwallpaper}.png`
- `src/tools/album/`(含新增的 `dashboard.rs` 与 `store::with_index` 锁)、
  `descriptions/album.json`、`workspace.rs`、四处接线
- `src/web/dashboards/album.rs` + `web/dash-album.js` + 面板接线(图库面板)
- `src/tools/map.rs`、`src/web/map_api.rs`、`web/mapcard.js`、`descriptions/map.json`
- `src/tools/express.rs`、`web/expresscard.js`、`descriptions/express.json`、
  `Cargo.toml` 的 `md-5`
- `src/config/{tool_plugins,defaults}.rs`(map / express 两份配置)、
  `web/settings-schema.js`
- `/init`:`src/slash_commands.rs`、`src/cli/repl/remote/interactive.rs`、
  `src/agent/prompt.rs`、`src/tools/subagent.rs`、`src/config/mod.rs`
- `src/tools/fixtures/registry-shapes.json`(三件新工具,已重新生成)
- `todolist.md`(地图与快递两条划掉)、`docs/wiki/06-内置工具与插件.md`、本文档

### 13.5 二进制安装状态(重要)

- `~/.cargo/bin/miyu-voice` = **新版已装**(14:01),三处修复在内
- `~/.cargo/bin/miyu` = **仍是旧版 0.5.0**,不含身份改造与 album。
  `target/release/miyu`(14:07)编好了但**故意没装**——装了要重启 daemon,
  而当时正在排查语音,重启会打断。
- 旧二进制备份:`~/.miyu/bin-backup/`

**操作纪律(踩过坑)**:`miyu daemon restart` **必须由用户在自己的终端跑**。
从别的进程(如 agent 的执行环境)重启会让 voice worker 拿不到 macOS 麦克风授权,
而 CoreAudio 拒绝时不报错、只给静音,现象是「进程活着、日志正常、就是没反应」。

### 13.6 其他未处置

- **模型池仍是 `antigravity/gemini-3.8-flash-high`**(§10)。用户决定自己挑,
  在换掉之前,人格对话随时可能再撞 Google 内容策略。
- **`command_deny` 44 条已生效**(daemon 已重启过)。
- **旧布局残留未清**(`~/.miyu/prompts/`、`~/.miyu/scripts/`、
  `config/.miyu-pm-backups/`)——用户当时没勾选,不是遗漏。
- **快照**:`~/miyu-export-miyu-20260913-131822.tar.gz`(259M,0600,**含明文
  API key,不得外传**)。

### 9.2 `miyu-voice devices` 在 macOS 上永远是空的

**根因**:`mic.rs` 退路过滤器 `name.contains("CARD=")`。`CARD=` 是 **ALSA 的设备名
约定**,只在 Linux 成立;macOS 的 CoreAudio 设备叫「MacBook Air麦克风」,永不含它 →
全被滤光。前置的 `list_pipewire_sources()` 要 shell 出 `pactl`,macOS 没这命令 → 返回空
→ 落到这个过滤器。

**影响面有限**:只影响「选一个非默认麦克风」;`config.microphone` 为 null 时不走枚举,
默认麦克风照常工作。**改法**:那条 ALSA 专用过滤只在 Linux 上套。

与 §1 的 Landlock 同一类:Linux 专有假设没加平台门。**这类问题大概率不止这两处**,
值得系统扫一遍。

### 9.3 唤醒词编码的坑

编码路径已逐单元核对,KWS 的 227 个建模单元里需要的全在:
`清 qīng → q + īng`、`影 yǐng → y + ǐng`、`顾 gù → g + ù`。**不是编码问题。**

`parse_explicit_pinyin` 要求 ASCII 里**带空格或声调数字**才走干净的单行路径。
`"qingyingqingying"` 这种无声调拉丁连写会掉进「按声调候选展开成多行」,而且音节切分
本身有歧义(`qin-gy-ing` vs `qing-ying`)。所以拼音条目一律写显式形式
`"qing1 ying3 qing1 ying3"`。

**叠词不是凑数**:原默认 `未有未有`/`密友密友` 都是叠词——音节长、声学特征明显,
命中率高误触少。所以对位是「清影清影」而不是「清影」。

---

## 14. 地图:从「一条高德链接」到一张真地图(今天做完)

**缺口(用户原话)**:「目前状态给的是高德地图 url 链接卡片,无法预览地图不方便。」
症结是**她手里没有坐标**——只能贴一条网址,链接卡片再把它升级成一张没有地图的
缩略图卡,要看地图得点出去。

### 14.1 三层

| 层 | 落点 | 干什么 |
|---|---|---|
| 工具 | `src/tools/map.rs` + `descriptions/map.json` | `map_search`:地址/POI → 坐标,坐标 → 地址 |
| 代理 | `src/web/map_api.rs`(`/api/map/tile`) | 瓦片由 daemon 代取 + 磁盘缓存 |
| 卡片 | `web/mapcard.js` + `styles.css` | 自己写的切片地图:拖、缩、标记点、详细视图 |

### 14.2 两条数据源,不是二选一(用户裁定:开源优先 + 高德可选)

`provider = "auto"`(默认):配了 `plugins.map.amap_key` 走高德,没配走
Nominatim + OSM 瓦片。显式写 `osm` / `amap` 就不再自动。

| | 开源(默认) | 高德 |
|---|---|---|
| 检索 | Nominatim | restapi.amap.com(place/text、place/around、geocode、regeo) |
| 瓦片 | tile.openstreetmap.org | webrd0{1-4}.is.autonavi.com(中文路网) |
| key | 不要 | 要 Web 服务 key(不是 JS API key) |
| 坐标系 | WGS-84 | GCJ-02 |

**坐标系是这件事里最容易错又最难发现的地方。** 两套差几百米,把一套的点画到另一套
的瓦片上,标记会稳定地落在隔壁街,而地图本身看起来完全正常。所以每个结果**两套
坐标都带**(`lon`/`lat` 是该源原生 datum,另有 `lon_wgs84`/`lon_gcj02`),卡片按
provider 选瓦片与对应那套,外链也各用各的。换算在 Rust 里(`wgs84_to_gcj02` /
`gcj02_to_wgs84`,反向迭代三次),前端一个字都不算——测试钉了「天安门来回 < 1 米」
与「东京不偏移」两条。

### 14.3 为什么瓦片必须过 daemon

WebUI 的 CSP 是 `img-src 'self'`。放开它意味着每打开一张地图,用户的浏览器就要向
瓦片服务器发几十个带 IP 与 Referer 的请求——**一屏就是十几二十块瓦片**,足够对面
画出「谁、什么时候、在看哪里」。这与链接卡片当初的结论是同一条,所以做法也一样:
daemon 代取,浏览器只跟本机说话。

缓存的两难:用户要「地图每次是最新状态」,而公共瓦片服务器(尤其 OSM 官方)的使用
条款不欢迎被当自家 CDN 刷。折中是磁盘缓存 + TTL(默认 72 小时,`tile_ttl_hours`,
0 = 每次回源),超 `tile_cache_mb` 按最旧的删;回源失败时**宁可发过期的那块**——
地图上少一块瓦片是很显眼的破洞,而三天前的路网几乎一定还对。

### 14.4 为什么没引地图库

要的只有平移、缩放、几个标记和一个放大视图。引 Leaflet 要连它的 CSS 与图标一起
vendored(本仓库 vendor 目录里已经有 echarts/katex/prism,每一份都是真的省不掉才
放进去的),而这里的全部算法就是 Web Mercator 两个公式加一层瓦片网格,`mapcard.js`
一个文件装得下,还顺手把键盘操作(方向键平移、+/- 缩放)做了。

### 14.5 实测

```
osm:200 31179B     # tile.openstreetmap.org/12/3415/1775.png(带 UA)
amap:200 14607B    # webrd02 style=8 同一块
nominatim: 上海图书馆(东馆) 31.2221296,121.5426325  category=amenity/library
```

**没验到的**:卡片本身没在浏览器里看过(要重启 daemon,而重启只能由用户在自己的
终端做——§13.5 的纪律)。

## 15. 快递查询(今天做完)

`src/tools/express.rs` + `descriptions/express.json` + `web/expresscard.js`。
数据源快递 100(用户裁定),`provider` 字段留着给第二家。

**这件工具最重要的设计不是查得到,是查不到时不假装查到了。** 一个语言模型手里拿着
单号,最容易发生的事就是编一条看起来很像的轨迹——「已到达上海转运中心」这种句子它
张口就来,而用户会当真。所以工具**成功返回**但带 `state`:

| state | 含义 | 卡片画什么 |
|---|---|---|
| `ok` | 真查到了 | 时间线,最新一条描粗 |
| `no_credentials` | 本机没配 key | 一句说明 + 官方查询入口 |
| `unknown_company` | 认不出哪家 | 同上,并请用户指明 |
| `needs_phone` | 顺丰要手机号后四位 | 同上 |
| `provider_error` | 上游拒了 | 同上,带上游原话 |

契约里对模型写死了一句:除 `ok` 外**没有任何物流信息**,照直说查不到,不许推断。

**号码识别**走快递 100 的识别口:配了 key 走 `auto`,没配走免费的 `autoComNum`。
两个口**返回形状不同**(裸数组 vs `{returnCode, data:[…]}` 信封),只认一种的话换条路
就静默识别不出来——`first_com_code` 两种都认,单测钉了三种形状(含 `returnCode=201`
的「不是有效的快递单号」)。

签名是 `MD5(param + key + customer)` 大写,新加 `md-5` 依赖(与已有的 sha1/sha2
同门);算错的表现是上游一句 `sign error`,分不清是自己算错还是 key 配错,所以
单测拿标准向量把 MD5 那一半钉死。

**没验到的**:真查询要用户在快递 100 后台申请 `customer` + `key` 填进
`plugins.express`。没配之前走的是 `no_credentials` 那条路。

## 16. 图库面板与 `/init`(今天做完)

### 16.1 图库面板(§13.2 的下半截)

后端 `src/web/dashboards/album.rs`(清单 / 图片直出 / 上传 / 改名描述标签 / 删除),
前端 `web/dash-album.js`,复用表情包那套瀑布流样式,**不抄**它的视觉识图、语义检索、
内置层与使用计数。

**顺手补的一件事**:索引是整份读-改-写的,而现在动它的有两条路(模型调 `album`
工具、人在面板上改)。没有锁就是后写的把先写的整份盖掉——她刚存进去的图会在你保存
描述的那一刻凭空消失。`store::with_index` 按库目录加了一把进程级锁,两条路都走它。

面板上特意留了一栏「可被搜到」:没写描述的条目 `action=search` 永远搜不到,这件事
在面板上要能一眼看见,否则存了等于没存。

### 16.2 `/init` → `GQY.md`

- `/init` 进 `slash_commands` 表(CLI only,`web: false`)。它是唯一一条**展开成
  消息**的命令:扫工作区、判断结构、写文件,每件都是她手里现成的工具干的活。
  非 dev 模式下拒绝并说明——`GQY.md` 只在 dev 提示词里注入,人格会话里生成等于
  让她白做一遍没人读的功课。
- 注入落在 `agent::prompt::with_project_context`:工作区根上的 `GQY.md` 拼在 dev
  提示词后面,dev 子代理(`build_dev_system_prompt`)读同一份——否则派出去的子代理
  会按通用惯例改代码,回来全是风格不对的补丁。
- **前缀那个两难按 §8 的倾向定了**:进系统提示词(= 缓存前缀),文件改了接受一次
  计划内冷启动。备选是进每轮瞬态尾巴,那样一份 3KB 的说明在几十轮会话里就是几十 KB
  的重复。改文件是人的动作、低频,划算。注入上限 32KB,超了截断并在块里说明。
- 没有工作区 / 没有这个文件 / 文件是空的:**一个字都不加**,dev 提示词逐字不变。

**没验到的**:`/init` 没在真 REPL 里跑过(要装新二进制)。


### 16.3 验收实录(09-13 晚)

**图库五个动作**——这是 §13.2 交接时点名的「接手第一件事」,现在跑过了。

活库(daemon 那份,22 张)上先跑读动作:

```
$ miyu tool-call album '{"action":"list","limit":1}'    → 1 of 22 pictures
$ miyu tool-call album '{"action":"search","query":"封面"}' → 命中「清影高定杂志封面」
```

写动作拿一张探针图在**新二进制**上走完整圈(隔离的 `MIYU_HOME`,不碰活库):

```
save   → saved to the album: 探针 (id 1ac3eb9a); 1 pictures total
search → 命中,描述与标签都对
show   → sent 探针 (id 1ac3eb9a)
delete → deleted entry 1ac3eb9a (file removed too); 0 pictures left
```

活库那边也做过一次 save→search→show→hard delete 的往返,删完回到 22 张、探针文件
不留痕。

**地图**(新二进制,直连无 daemon):

```
$ miyu tool-call map_search '{"query":"上海图书馆东馆","limit":2}'
provider osm datum wgs84 count 1
 - 上海图书馆(东馆) | 121.542633 31.22213 | gcj 121.54692 31.219989 | amenity/library
$ miyu tool-call map_search '{"reverse":"121.542633,31.222130"}'
上海图书馆(东馆), 迎春路, 花木街道, 浦东新区, 上海市, 200127, 中国
```

GCJ 偏移约 480 米,方向也对——这正是「拿 WGS-84 的点画在高德瓦片上」会错的量级。

**快递**(没配 key,验的是降级路):

```
$ miyu tool-call express_query '{"number":"SF1234567890"}'
state: no_credentials   traces: 0   note: …do NOT invent any shipment status.
$ miyu tool-call express_query '{"number":"帮我查查这个"}'
error: a tracking number is letters, digits and dashes only
```

**一个要记住的坑**:`miyu tool-call` 在 daemon 活着时**走 IPC**,调的是 daemon 里
那份 registry——也就是**旧二进制**。新加的工具在那条路上是 `unknown tool`,看起来
像没注册。验新工具要么装新二进制重启 daemon(只能用户自己在终端做,§13.5),要么像
这次一样用一个隔离的 `MIYU_HOME` 走直连回退。

**仍然没验的**:图库面板与两张卡片都要浏览器 + 新 daemon,一次都没点过。

## 17. 又一处 Linux 专有假设(今天撞到的第四处)

`render::math::pty_tests::halfblock_output_preserves_pty_termios` 在 macOS 上
**永远不返回**——`cargo test --lib` 因此跑不完(不是慢,是挂住)。这与 §1 的
Landlock、§9.2 的 ALSA `CARD=`、§9.1 的 PipeWire 16kHz 是同一类,进 §12 待办 2
的清单。绕过办法:`cargo test --lib -- --skip pty_tests`。
