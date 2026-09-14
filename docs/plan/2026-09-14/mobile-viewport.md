# 移动端软键盘顶屏与整页可拖

> 总览:`main-tasks.md`。设计稿:`../../design/2026-09-14/main-tasks.md`(小节编号与设计稿、总览一致,拆分时保留)。
> 未跑测试,下文是读码推理;`未实测` / `未核实` 处施工时补数字。

## 6. 移动端软键盘顶屏 / 整页可拖

### 现状

已做过两轮:2026-09-10 `6973d8da`、2026-09-11 `d1dd2b7c`(Android Chrome 到底后再拖被推出屏幕)。
`html, body` 已 `overflow:hidden; overscroll-behavior:none`;`syncAppHeight` 已在键盘弹出后 `scrollTo(0,0)`(`app.js:12961`)。

### 设计稿里不能照做的

- `html, body { touch-action: none }` + 内层 `pan-y`:**生效的 touch-action 是祖先链的交集**,
  子元素写 `pan-y` 放不出来——整个聊天区会滚不动。
- `focus` 里 `preventScroll`:只对脚本调用 `el.focus({preventScroll:true})` 生效,用户手指点输入框不走这条。
- `body { position: fixed }`:iOS Safari 上已知会引起光标位置与输入框错位,需要真机验。

### 改法

1. **先要复现环境**(见下方问题),然后抓 `visualViewport.height/offsetTop`、`scrollY` 的时序。
2. 候选:viewport meta 加 `interactive-widget=resizes-content`(Chrome 108+ 让键盘直接缩布局视口,
   绕开「先滚再缩」);`syncAppHeight` 同时处理 `visualViewport.offsetTop`;
   排查是否有未设 `overscroll-behavior: contain` 的内层滚动容器把滚动链传到 body。

> **问:** 出问题的是哪台手机、哪个浏览器(iOS Safari / Android Chrome / 微信内置)?是否「添加到主屏」的 PWA 形态?三者病因不同,不定下来没法改。
> **问:** 「页面可以拖动」是整页橡皮筋,还是横向能拖出一截?后者通常是某个元素宽度溢出,和键盘无关。
