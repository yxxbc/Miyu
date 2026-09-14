# 相对路径链接 → 侧栏预览 · 打开所在文件夹 · 默认应用打开

> 总览:`main-tasks.md`。原为总览 §12,2026-09-14 拆出。遵循 `docs/理念.md`。
> 未跑测试,下文是读码推理;`未实测` / `未核实` 处施工时补数字。
>
> 起因:方案稿按 AGENTS.md §8.3 改成相对路径后,WebUI 渲染会漏源码。

## 0. 用户裁定(2026-09-14)

| # | 问题 | 裁定 |
|---|---|---|
| — | 点击相对路径 | **侧栏打开源码预览**,侧栏里再给「打开所在文件夹」「用本机应用打开」 |
| 1 | 局域网电脑用 VS Code Remote-SSH 链接 | 不要 |
| 2 | 手机上「打开所在文件夹」 | **只分享这一个文件**;**`plugins.file_sharing` 关闭时判定不可用**(按钮不下发) |
| 3 | 用哪个编辑器 | **系统默认应用**,不做编辑器配置 |
| 4 | 反向代理 | 现在没有;以后可能有「电脑在家、手机在外用」 |
| 5 | 复制格式 | `路径:行号` |
| 6 | WebUI 建的分享要不要自动过期、要不要藏出分享列表 | **都不要**:与普通分享完全一样,持久、进列表 |
| 7 | 远程访问方式 | 还不清楚;**先安全,后功能完整** |

## 1. 现状(读码)

| 事实 | 位置 |
|---|---|
| 链接只认 `http/https/file`,相对路径识别失败 → `[文字](../../x)` 原样漏出 | `web/app.js:3964`、`:4199` |
| `file://` 点击是复制路径,复制内容带着 `#L5540-L5575` | `web/app.js:3977`、`:3988` |
| 产物落库时 `source_key` = canonical 绝对路径,但下发的 `SafeArtifactAsset` 不带它 | `src/state/assets.rs:125`、`src/web/assets.rs:935` |
| daemon 默认绑全部网卡,已开 `ConnectInfo` | `src/web/server.rs:46`、`:194` |
| 成员账号工作区固定 `home/<用户>/workspace` 且套 Landlock;管理员无作用域 | `src/web/sandbox_scope.rs` |
| `origin_is_allowed` 校验已存在 | `src/web/security.rs:131` |
| `share_file` 按 daemon 的各个访问地址拼 `/api/shared/{id}?download=1`,**只支持单个文件** | `src/tools/share_file.rs:18`、`:103` |
| 分享开关 `plugins.file_sharing.enabled`,关闭时 `/api/shared` 上传直接报错 | `src/config/tool_plugins.rs:186`、`src/web/shared_files.rs:86` |
| 现有「是不是手机」只有一条 `(hover: none), (pointer: coarse)` | `web/app.js:3199` |
| 仓库里没有任何「在主机上打开文件/文件夹」的代码 | — |

## 2. 两个判断要分开

| 判断 | 决定什么 | 谁来判 | 依据 |
|---|---|---|---|
| **看的人是不是坐在主机屏幕前** | 能不能在主机上弹 Finder / 开应用 | **后端** | 对端地址 + 转发头 |
| **屏幕长什么样** | 侧栏 vs 全屏抽屉、按钮大小 | **前端** | 视口宽度、指针类型 |

局域网里的另一台笔记本和手机,在第一个判断上是同一类(都不在主机屏幕前);iPad 接键盘和手机,在第二个判断上不是同一类。
按钮的**行为**由后端下发的能力决定,**样子**由前端决定,前端不自己猜是不是本机。

## 3. 设计

### A. 链接识别(前端)

- 认作相对路径:`./`、`../` 开头,或无协议、含 `/`、末段带扩展名;可带 `#L10` / `#L10-L20`。
- 前端不拼绝对路径,只把 `{base, href}` 交给后端。`base`:聊天消息 = `session:<id>`;侧栏 md 产物 = `artifact:<id>`(后端取 `source_key` 的父目录)。
- 点击 → 侧栏源码视图,滚到并高亮行区间;悬停显示相对工作区的路径。
- 复制一律 `路径:行号`(区间取起始行);顺手修 `file://` 复制带 `#L…`。

### B. 读文件接口

`GET /api/workspace-file?base=…&href=…` → `{display_path, line_start, line_end, mime, kind, text | url, host_actions}`

- 路径判定:join → canonicalize → `starts_with(根)`,复用 `src/web/assets.rs:925`。根 = 成员 `member_scope.workspace` / 管理员会话工作区;`artifact:` base 还要求 `source_key` 本身在根内。
- 只读,上限沿用产物 20 MiB;不进产物列表、不落库,与 `agent-runtime.md` §4 的 trace 视图、`dev-workbench.md` 的改动视图、`job-control.md` 的任务视图共用「侧栏临时视图」槽位。
- `display_path` 相对工作区,不向成员下发主机绝对路径。

### C. 能力下发:`host_actions`

```
host_actions: { reveal: "host" | "share" | null, open: "host" | null }
```

| 条件 | reveal | open |
|---|---|---|
| 管理员 · 对端 loopback · **无转发头** | `host`(主机上弹文件管理器) | `host`(主机默认应用) |
| 管理员 · 其他(局域网、手机、以后的远程) · 分享开启 | `share` | `null` |
| 成员账号 · 分享开启 | `share`,只限自己工作区内的文件 | `null` |
| 非本机 · **分享关闭** | `null`(按钮不出现) | `null` |

**安全优先(裁定 7)的落法**:

- 转发头(`X-Forwarded-For` / `Forwarded` / `CF-Connecting-IP`)任一存在就**降级**为非本机。以后经 Tailscale Serve / cloudflared / frp 进来的请求对端全是 `127.0.0.1`,只看 loopback 会把在外面的手机当成坐在主机前。头只能降级、不能升级。
- 判不准就往弱的方向判:对端地址解析失败、IPv4-mapped IPv6 看不清,一律当非本机。
- `未实测`:Tailscale 直连时对端是 `100.x`,自然落到非本机,施工时验一次。

### D. `reveal = host`:主机上打开所在文件夹

独立模块 `src/host_open/`,按平台分文件,不进 `server.rs`。进程参数走 argv,不经 shell。

| 平台 | 命令 | 兜底 |
|---|---|---|
| macOS | `open -R <file>` | — |
| Linux | D-Bus `org.freedesktop.FileManager1.ShowItems` | `xdg-open <父目录>`(不选中) |
| Windows | `explorer /select,<file>` | — |

### E. `open = host`:系统默认应用打开

| 平台 | 命令 |
|---|---|
| macOS | `open <file>` |
| Linux | `xdg-open <file>` |
| Windows | `explorer <file>` |

**默认应用打开 = 对可执行类型就是执行。** `open x.command` 会在终端里跑,`xdg-open x.desktop` 会启动它声明的程序,Windows 上 `.bat/.exe/.lnk` 直接运行。
而要打开的文件是**模型写的**。所以:

- 只放行白名单扩展名(文本/代码/文档/图片/PDF/表格)。其余类型按钮变成「打开所在文件夹」,不给直接打开。
- 白名单内还要看可执行位:Unix 上带 `x` 位的一律不直接打开。
- 默认应用接不了行号,这是裁定 3 的已知代价——要看具体行就用侧栏源码视图。

### F. `reveal = share`:分享这一个文件

- 复用 `shared_files` 的存储与 `/api/shared/{id}` 路由,**不经模型**:WebUI 直接调接口建分享,**与 `share_file` 建出来的分享完全同一种**(持久、进分享列表、可手动取消)。
- 默认引用模式(不复制);同一文件已有分享就复用,不重复建。
- 点击后:移动端优先调 Web Share API(系统分享面板),不支持时显示下载链接 + 复制链接。
- 分享关闭时后端不下发 `share`,前端不画按钮——不做「点了才报错」。

### G. 前端形态判断(只管样子)

- 视口宽度 ≤ 760(已有口径)。
- `(pointer: coarse)` 且 `(hover: none)`。
- `navigator.userAgentData?.mobile`(Chromium 系有,Safari 没有)。
- iPadOS 桌面模式会伪装成 Mac:`platform` 像 Mac 但 `maxTouchPoints > 1` 时按平板处理。

## 4. 理念对照

| 理念 | 落到这里 |
|---|---|
| 权限由代码承担 | `host_actions` 由后端按真实 principal + 对端 + 插件开关判定,前端只渲染 |
| 前缀即契约 | 纯 HTTP 与 WebUI,不碰工具描述与请求前缀。WebUI 建分享**不走** `share_file` 工具,不往上下文里加东西 |
| 报错要说真原因 | reveal/open 失败原样回报(没有 `DISPLAY`、D-Bus 不通、类型不在白名单),不装作成功 |
| 先证明不修时现象出现 | 转发头降级:先用 `curl -H 'X-Forwarded-For: 1.2.3.4' http://127.0.0.1…` 证明修前拿得到 `host`、修后拿不到 |

## 5. 默认决定(不另问)

1. 分享 ID 现在的长度与生成方式 `未核实`。持久分享 + 以后可能暴露到外网,链接能被猜到就等于公开文件——施工第 6 步前先读 `src/state/shared_files.rs`,不够 128 bit 随机就先补。这条按「先安全」默认做,不另问,除非你有异议。

## 6. 施工拆分

| 步 | 内容 | 动的地方 | 验证 |
|---|---|---|---|
| 1 | 相对路径识别 + 复制 `路径:行号` + `file://` 修正 | `web/app.js` | 桌面 + 手机浏览器 |
| 2 | 读文件接口 + 侧栏源码视图定位行 | `src/web/`、`web/app.js` | 越界(`../../..`、符号链接、成员读管理员目录)全部 403 |
| 3 | `host_actions` 判定 + 转发头降级 + 分享开关 | `src/web/` | curl 带/不带转发头对照;关掉分享后按钮消失 |
| 4 | `reveal = host` 三平台 | `src/host_open/` | macOS 本机 + Arch 真机;systemd user service 下验 `DISPLAY`/D-Bus |
| 5 | `open = host` + 白名单与可执行位 | `src/host_open/` | 先造 `.command` / 带 `x` 位的脚本,证明被拒 |
| 6 | 分享 ID 熵核查 + `reveal = share` + Web Share API + 形态判断 | `src/state/shared_files.rs`、`src/web/`、`web/app.js` | iOS Safari、Android Chrome 真机 |

每步单独 commit。4、5 在主机上拉起进程,合并前过 `/security-review`。日期见总览 §2:1、2 排 2026-09-20,3 排 2026-09-24,4–6 排 2026-09-28 批次。
