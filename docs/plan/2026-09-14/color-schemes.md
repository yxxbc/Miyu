# 配色方案可添加

> 总览:`main-tasks.md`。设计稿:`../../design/2026-09-14/main-tasks.md`(小节编号与设计稿、总览一致,拆分时保留)。
> 未跑测试,下文是读码推理;`未实测` / `未核实` 处施工时补数字。

## 7. 配色方案可添加

### 现状

`setColorScheme` 只认 `madobe` / `matugen`(`app.js:681`),matugen 走 `/theme.css` + `link.disabled` 切换;
`ui_prefs.rs` 的 `colorScheme` 值上限 64 字节,放方案 id 够用。

### 改法(推荐)

- 方案文件落 `~/.miyu/config/color-schemes/<id>.json`,**后端编译成 `/color-schemes/<id>.css`**,
  前端复用 matugen 那条 `<link>` 切换路径。不走注入 `<style>`(CSP 挡),也不走逐个 `setProperty`(几十个 token,切换时会闪)。
- 规范:`dark` / `light` 两套 MD3 token 必填;`--link` 与 `--tok-*` 不可覆盖——后端编译时直接丢弃这些键,不靠约定。
- 编译时校验 `on-*` 对 `*` 的对比度 ≥ 4.5,不达标拒收并说明哪一对。

### 验证推理

- 与 matugen 同路径 → 缓存、禁用、回落逻辑都现成;删掉当前方案时回落 `madobe`,与 matugen 探测失败同一分支。

> **问:** 「输入主色自动生成全套 token」要不要做?要引 Material Color Utilities(几十 KB),且和 matugen 做的是同一件事。倾向:第一版只做「导入 JSON」+「从当前 matugen 输出另存一份」。
> **问:** 方案与主题(`graphite` / `linen`,`app.js:653`)是正交的两维,还是自定义方案要能连主题一起定?
> **拓展:** 方案 JSON 直接可分享(复制 / 粘贴),不做在线市场。
