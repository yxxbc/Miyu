# WebUI 交付:图片拖拽 · 产物按钮 · 围栏预览(SVG / HTML)

> 总览:`main-tasks.md`。设计稿:`../../design/2026-09-14/main-tasks.md`(小节编号与设计稿、总览一致,拆分时保留)。
> 2026-09-14 按用户要求提前施工,**未编译、未跑测试**;下文「验证推理」是读码推出来的,`未实测` 处验收时补。

## 0. 用户裁定(2026-09-14)

1. §1:加**双击「适应 ↔ 原始尺寸」**与**键盘 `+ - 0`**;手机双指捏合**不做**。
2. §5:chip 挂**气泡底部**,只显示类型徽标 + 截断名,带「下载」小按钮。
3. §9:~~不引 mermaid(3.4 MB 不接受)~~ **同日改判:接 mermaid**(npm `mermaid@12.0.0` 的 `dist/mermaid.min.js`,gzip 后 1.6 MB 进二进制,
   只在沙箱框里按需下载,主页面不载);「```html 围栏进沙箱 iframe」照做,为后续数据可视化(ECharts)铺路;```svg 走 `<img>`。

## 1. Artifact 图片拖拽迟滞与错位

### 现状

`renderArtifactImage`(施工前 `app.js:5524`):`pointerdown` 加 `is-dragging` + `setPointerCapture`,
`pointermove` 同步写 `state.artifactPanX/Y` 并 `applyTransform()`,`pointerup/cancel` 摘类名。
`transform-origin: center top`,滚轮缩放带 160ms 过渡。

todolist 这条的原话早于 2026-09-12 那次修复,**不能确定 2026-09-14 的构建上还复现**。
本次没有先复现(未跑),把计划里列的四个可疑点一并改掉,验收时对照。

### 改法(已施工)

1. **滚轮缩放以指针为锚**:原点改 `0 0`,缩放按 `pan' = pan + (指针 − 图框 − pan)·(1 − 新/旧)` 补偿。
   按钮、键盘缩放以 stage 中心为锚。最小缩放统一为 1(原先 0.25 下限实际会被复位成 1,缩小按钮永远不灰)。
2. **`pointermove` 合并进 rAF**,位移只除 `UI_SCALE`、不除 zoom;松手时把未画的最后一帧当场补上。
3. **监听 `lostpointercapture`** 一起收尾。
4. **不再无谓重建 stage**:`renderArtifactWorkspace` 记 `id|mode|url|updated_at`,相同就跳过重建
   (回合同步、全屏切换都会走到这里)。需要重建的地方(注册 / 切换 / 移除导致缩放归零)统一走 `resetArtifactImageView()` 作废记号。

### 验证推理

- translate 在 scale 前:元素上一个 CSS px 的 translate = 屏幕上 `UI_SCALE` 个像素,与 zoom 无关。
  所以 `Δclient / UI_SCALE` 正好让抓取点跟手;设计稿的 `/zoom` 在 zoom=2 时让图只走一半。
- `.app-shell` 带 `zoom: var(--ui-scale)`(`styles.css:316`),`#artifactWorkspace` 在它里面,所以除 `UI_SCALE` 的前提成立。
- 锚点公式:原点 `0 0` 时屏上位置 = 图框 + pan + zoom·q;令锚点下的 q 不变即得上式。图框取 `img.offsetLeft/Top`(stage 是 `position: relative`,即 offsetParent)。
- `未实测`:Safari 对 `zoom` 下 `clientX` 与 `getBoundingClientRect` 的换算是否与 Chromium 一致(锚点要两者同一套单位)。
- `未实测`:滚轮缩放时 translate 与 scale 一起走 160ms 过渡,中途锚点会略漂、终点正确;若肉眼可见再对滚轮关过渡。

## 5. 回复气泡里的产物按钮

### 现状

后端 `server.rs:780` 已按 `turn_id` 分组 `ArtifactAsset`,前端已遍历 `turn.artifacts`
收进全局列表——**数据全在,只是没画到对应回合上**。关闭侧栏后只剩顶栏小图标(`artifactToggleButton`)。
设计稿的 `message.artifacts` 不存在,实际字段是 `turn.artifacts`。

### 改法(已施工)

新模块 `web/artifactchips.js`(`MiyuArtifactChips.sync`),实时与刷新后走同一个函数:

1. 刷新后:`renderPersistedTurn` 把 `turn.artifacts` 传给 `createAssistantMessage`,画在 `.assistant-content` 底部。
   只有产物、没有正文的回合也会建气泡。
2. 实时:`tool.artifact` 事件更新 `live.artifacts` 后同步到当前 run 的气泡。
3. chip = 图标 + 类型徽标 + 截断名(中间截断、保留扩展名,14 字符),全名只在悬停提示;右侧「下载」走 `?download=1`。
4. 点击 = `registerArtifact(source)` + `setArtifactWorkspaceOpen(true)`;被「移除」过的也能再调出。

### 验证推理

- 纯前端,不改接口、不改库。
- 同一回合多次改写同一文件:**已核实**不会出多个 chip——`attachments.rs:375` 按 `(turn_id, source_key)` upsert,id 不变,前端按 id 去重。

> **拓展(未做):** `dev-workbench` 3.2 的改动行与 chip 同一位置,样式以本节为准。

## 9. 围栏预览:SVG 与 HTML

### 现状与既有定案

- markdown 里的围栏一律当代码块渲染;SVG artifact 走 `<img>`,浏览器强制禁脚本禁外链。
- 2026-09-11 定案:「活性内容一律进沙箱」。设计稿的「净化后注入原生 `<svg>`」「主页面 `mermaid.initialize`」越过这条线,作废。
- 主页面 CSP:`img-src 'self' blob:`(无 `data:`)、`script-src 'self'`(`assets.rs` `finish_asset_response`)。

### 与原计划不同的地方

- mermaid 按裁定 3(同日改判)接入,**不照设计稿进主页面**,走与 html 同一个沙箱 iframe(见改法 6)。
- 原计划 SVG 用 `data:` URL——**主页面 CSP 不放 `data:`,会被挡**,改用 blob URL。
- 原计划 HTML 复用 artifact 那条 CSP——srcdoc / blob iframe **继承主页面 CSP**,内联脚本跑不了;
  所以新增一个宿主页 `/fence-frame.html`,响应头给 artifact html 同款 CSP,正文由父页面 postMessage 送进去。
- 「在侧栏打开」没做:侧栏只收 `/api/artifacts/` 下的地址(`safeArtifactUrl`),围栏内容没有落库地址。

### 改法(已施工)

1. 新模块 `web/fencepreview.js`(`MiyuFencePreview.decorate`),`codeBlock` 在围栏闭合后调用;块头加「预览 / 源码」切换,选择按源码记住。
2. ```svg:blob URL 进 `<img>`(按源码缓存 64 条);解析失败自动退回源码。
3. ```html:`<iframe sandbox="allow-scripts allow-modals" src="/fence-frame.html">`,宿主就绪后父页面按窗口身份认它、送正文;
   正文末尾附一段高度上报脚本,父页面把高度夹在 80–720px。
4. 流式期间 html 只显示「回复完成后渲染」;`finishLiveRun` 对含 html 围栏的文本块补画一次(流式每帧整段重建,iframe 会跟着重载)。
   施工中发现:`stabilizeStreamingMarkdown` 注释说「回合结束后按落库原文重画」,但 `finishLiveRun` 并不重画,所以补了 `rerenderLiveHtmlFences`。
5. 后端:`src/web/assets.rs` `fence_frame_asset`(artifact html CSP + `frame-ancestors <本机来源>`,不查登录——页面不含数据);
   `mod.rs` 两个 js + 一个 html 的 `include_str!`;`server.rs` 三条路由;`index.html` 两个 script 标签与版本号替换。
6. ```mermaid(改判后补):
   - vendor:`web/vendor/mermaid/mermaid.min.js.gz`(mermaid@12.0.0,5.6 MB → gzip 1.6 MB),`MERMAID_JS_GZ` + `mermaid_js_asset` + 路由
     `/vendor/mermaid/mermaid.min.js`,与 echarts 同走 `vendor_gzip_asset`(带私网放行头,不透明源的框才取得到)。
   - `fencepreview.js` `mermaidDocument`:拼一份载库 → `mermaid.render` → 写进 `#out` 的页面,交给 `htmlFrame`;
     `securityLevel: 'strict'`、`suppressErrorRendering: true`,失败显示报错原文。源码经 JSON 嵌入并转义 `<`。
   - 主题:拼页面时从 `body` 取 MD3 `#hex` 计算值写成 `themeVariables`(mermaid 不认 `var(--…)`),`darkMode` 按 surface 亮度判;
     **切主题后已画的图不跟着变**,重进会话才更新。框底透明(`styles.css` `.fence-preview.is-mermaid`)。
   - 流式期间与 html 一样只占位;`rerenderLiveHtmlFences` 的正则加了 `mermaid`。

### 验证推理

- 沙箱不给 `allow-same-origin`:框内是不透明源,拿不到 cookie / 父页面 DOM。出站由 CSP 的 `connect-src 'none'` 等掐。
- `document.open` 会清掉宿主页注册的监听,写进去的内容拿不到再次替换文档的入口。
- postMessage 目标只能写 `"*"`(不透明源):送出的就是这段围栏本身,框里的脚本本来就有,不构成额外泄露。
- 已知限制(与 HTML artifact 相同):沙箱不禁止框内自我导航,CSP 管不住导航外带。
- `未实测`:框内 `<script src="/vendor/echarts/...">` 能否加载——依赖 vendor 路由的 OPTIONS 分支(`server.rs` 注释),echarts 是否每条都配了。

## 6. 施工记录

2026-09-14,未编译、未跑测试(用户要求)。

| 文件 | 改动 |
|---|---|
| `web/app.js` | 图片缩放 / 平移重写(`zoomArtifactImage`、`resetArtifactImageView`、`handleArtifactImageKey`)、工作区不重建记号、chip 接线、围栏预览接线、`renderStreamingMarkdown` / `rerenderLiveHtmlFences` |
| `web/artifactchips.js` | 新增 |
| `web/fencepreview.js` | 新增 |
| `web/fence-frame.html` | 新增 |
| `web/styles.css` | 图片 `transform-origin: 0 0`;围栏预览与 chip 样式 |
| `web/index.html` | 两个 script 标签 |
| `src/web/mod.rs`、`assets.rs`、`server.rs` | 静态资源常量、`fence_frame_asset`、路由、版本号替换 |

没验证的地方:全部未编译;`cargo fmt` 未跑(`assets.rs` 新函数手工按 rustfmt 风格写的)。

### 验收流程

先 `cargo build`,重启 daemon(`MIYU_BUILD_ID` 变了会自动重启),浏览器强刷。

1. **拖拽**:让她 `create_artifact` 一张大图 → 侧栏滚轮放大,指针下的内容应停在指针下;按住拖动,图应严格跟手、无拖尾;
   拖到一半切走窗口(Cmd+Tab)再回来,不应卡在「抓取」光标。Chromium 与 Safari 各试一次。
2. **双击 / 键盘**:双击图 → 放大到原始尺寸(小图放大两倍),再双击 → 适应;点一下侧栏空白处,按 `+` `-` `0`。在输入框里敲 `0` 不应影响图。
3. **不重建**:放大一张图后让她再跑一轮别的对话,侧栏的图保持缩放不复位。
4. **chip**:一轮里生成两个文件 → 气泡底部出现两个 chip,名称被截断、悬停见全名;点 chip 打开侧栏对应文件;点下载直接落盘;
   在资源菜单里「移除」一个,再点它的 chip 能调回来;刷新页面,chip 仍在且与刷新前一致。
5. **SVG 围栏**:让她在回复里写一个 ```svg 围栏 → 显示为图,切「源码」看代码;写一个坏 SVG → 自动显示源码。
6. **HTML 围栏**:让她写一个带 `<script>` 按钮计数的 ```html 围栏 → 流式期间显示「回复完成后渲染」,结束后变成可点的页面;
   在框内脚本里 `fetch("https://example.com")` 应被 CSP 拦(控制台报错);`parent.document` 应 SecurityError。
7. **ECharts**:让她写一个 ```html 围栏引 `/vendor/echarts/echarts.min.js` 画柱状图,看能否画出(对应 §9 `未实测` 一条)。
8. **Mermaid**:让她写一个 ```mermaid 流程图 + 一个时序图 → 流式期间「回复完成后渲染」,结束后画成图、配色跟当前主题、连线标签不是黑底;
   写一个语法错的 → 框里显示 `Mermaid 渲染失败:Parse error…`,没有额外的炸弹图;切「源码」能看回原文。亮 / 暗主题各试一次。

### 已实测(2026-09-14,Mermaid 接入时)

in-app 浏览器面板对不透明源发出的一切请求报 `ERR_BLOCKED_BY_CLIENT`(iframe 本身、框里的 vendor 脚本都拦),
**在面板里验不了框**;以下是换路子测的:

- `cargo build` 通过;daemon 重启后 `/vendor/mermaid/mermaid.min.js` 200、`content-encoding: gzip`、带 `access-control-allow-private-network`。
- `/fence-frame.html` 顶层打开、写入 html 围栏:`window.origin === "null"`,内联脚本能跑(按钮计数正常),
  `fetch("https://example.com")` 被 `connect-src 'none'` 拦。
- 本机 headless Chrome 152 + 同款 CSP(不透明源)喂 `mermaidDocument` 生成的页面:流程图画出、中文标签正常、MD3 配色生效;
  标签里塞 `"</script><img onerror=…>"` 没有提前闭合脚本,`onerror` 被净化掉;语法错显示报错原文。
- 实测后补的两处:`suppressErrorRendering`(原先多一张炸弹图)、`edgeLabelBackground`(原先连线标签黑底)。
- `未实测`:真实聊天回合里 iframe 握手 + 高度上报(面板拦 iframe,需在 Chrome / Safari 里按上面第 8 条走一遍)。
