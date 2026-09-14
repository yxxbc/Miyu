"use strict";

/*
 * 围栏预览:聊天正文里的 ```svg / ```html / ```mermaid 围栏画成图,块头「预览 / 源码」切换
 * (docs/plan-is-true/2026-09-14/webui-delivery.md §9)。
 *
 * **活性内容一律不进主文档**(2026-09-11 定案,`docs/plan-is-true/2026-09-11-artifact-delivery.md`):
 *   · svg → blob URL 进 `<img>`。浏览器对 `<img>` 里的 SVG 强制禁脚本、禁外链,与 artifact
 *     预览同一安全等级。主页面 CSP 是 `img-src 'self' blob:`,data: URL 会被挡,所以用 blob。
 *     代价:SVG 里的文字选不中(2026-09-14 裁定接受)。
 *   · html → 沙箱 iframe 装 `/fence-frame.html`(后端给它 artifact 同款 CSP),正文用
 *     postMessage 送进去 document.write。**不用 srcdoc / blob iframe**:它们继承主页面的
 *     `script-src 'self'`,内联脚本一行都跑不了,图表全是死的。这条通道就是给后续
 *     「画个图」用的——页面里 `<script src="/vendor/echarts/...">` 能取到本机的库。
 *
 *   · mermaid → 同一个沙箱 iframe,拼一份载入 `/vendor/mermaid/mermaid.min.js` 的页面去画
 *     (2026-09-14 改判接入,gzip 后 1.6 MB,只在框里按需下载,主页面不载)。主题色在拼页面
 *     那一刻从主页面取计算值写死——mermaid 不认 `var(--…)`;切主题后旧图保持原色,重开会话即更新。
 *
 * 流式期间:svg 围栏闭合就画(blob URL 按源码缓存,每帧重建也不重新解码);html / mermaid
 * 推迟到回合结束那次重画——流式每帧整段重建,iframe 会跟着一帧一重载。
 */
window.GqyFencePreview = (() => {
  const FRAME_URL = "/fence-frame.html";
  const MIN_HEIGHT = 80;
  const MAX_HEIGHT = 720;
  const DEFAULT_HEIGHT = 320;
  const CACHE_LIMIT = 64;
  const KINDS = { svg: "svg", html: "html", htm: "html", mermaid: "mermaid" };
  const MERMAID_URL = "/vendor/mermaid/mermaid.min.js";
  // mermaid themeVariables ← MD3 色板。只收 #hex(khroma 要能解析),取不到的键就交给 base 主题推导。
  const MERMAID_COLORS = {
    background: "--md-sys-color-surface-container",
    primaryColor: "--md-sys-color-primary-container",
    primaryTextColor: "--md-sys-color-on-primary-container",
    primaryBorderColor: "--md-sys-color-primary",
    secondaryColor: "--md-sys-color-secondary-container",
    secondaryTextColor: "--md-sys-color-on-secondary-container",
    tertiaryColor: "--md-sys-color-tertiary-container",
    tertiaryTextColor: "--md-sys-color-on-tertiary-container",
    lineColor: "--md-sys-color-on-surface-variant",
    // 不给的话 darkMode 下连线标签是黑底块。
    edgeLabelBackground: "--md-sys-color-surface-container",
    textColor: "--md-sys-color-on-surface",
    noteBkgColor: "--md-sys-color-secondary-container",
    noteTextColor: "--md-sys-color-on-secondary-container",
    errorBkgColor: "--md-sys-color-error-container",
    errorTextColor: "--md-sys-color-on-error-container",
  };

  // 高度上报附在正文末尾一起写进去。它与模型写的脚本同处一个文档,数字不可信,
  // 父页面只拿来夹在 [MIN, MAX] 里定高,不做别的。
  const HEIGHT_REPORTER =
    "<script>(()=>{const post=()=>parent.postMessage({type:'gqy-fence-height'," +
    "height:Math.ceil(document.documentElement.scrollHeight)},'*');" +
    "new ResizeObserver(post).observe(document.documentElement);addEventListener('load',post);post();})()<\/script>";

  const svgUrls = new Map();
  // 用户在某段源码上选过「源码」,流式重画 / 切会话回来仍保持。
  const modes = new Map();
  const frameSources = new WeakMap();
  let listening = false;

  function remember(map, key, value, onEvict) {
    map.set(key, value);
    if (map.size <= CACHE_LIMIT) return;
    const [oldKey, oldValue] = map.entries().next().value;
    map.delete(oldKey);
    onEvict?.(oldValue);
  }

  function svgUrl(source) {
    const cached = svgUrls.get(source);
    if (cached) return cached;
    const url = URL.createObjectURL(new Blob([source], { type: "image/svg+xml" }));
    // 已解码显示着的 <img> 不受 revoke 影响,只是之后不能再用这个地址加载。
    remember(svgUrls, source, url, (old) => URL.revokeObjectURL(old));
    return url;
  }

  function listen() {
    if (listening) return;
    listening = true;
    window.addEventListener("message", (event) => {
      const data = event.data;
      if (!data || typeof data !== "object" || !event.source) return;
      // 沙箱 iframe 是不透明源,event.origin 恒为 "null",只能按窗口身份认。
      const frame = [...document.querySelectorAll("iframe.fence-frame")]
        .find((item) => item.contentWindow === event.source);
      if (!frame) return;
      if (data.type === "gqy-fence-ready") {
        const source = frameSources.get(frame);
        if (source === undefined) return;
        // 目标只能写 "*":不透明源没有可写的 targetOrigin。送出去的就是这段围栏本身,
        // 框里的脚本本来就拿得到,不构成额外泄露。
        event.source.postMessage({ type: "gqy-fence-html", html: source + HEIGHT_REPORTER }, "*");
      } else if (data.type === "gqy-fence-height") {
        const height = Number(data.height);
        if (!Number.isFinite(height)) return;
        frame.style.height = `${Math.round(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, height)))}px`;
      }
    });
  }

  function htmlFrame(source) {
    listen();
    const frame = document.createElement("iframe");
    frame.className = "fence-frame";
    frame.title = "HTML 预览";
    // 与 HTML artifact 同一套(app.js renderArtifactPreview 的注释):不给 same-origin、
    // popups、forms、top-navigation。出站由 fence-frame.html 响应头的 CSP 掐。
    frame.setAttribute("sandbox", "allow-scripts allow-modals");
    frame.setAttribute("referrerpolicy", "no-referrer");
    frame.style.height = `${DEFAULT_HEIGHT}px`;
    frameSources.set(frame, source);
    frame.src = FRAME_URL;
    return frame;
  }

  function mermaidTheme() {
    // 主题变量挂在 body[data-theme] 上,从 body 取。
    const style = getComputedStyle(document.body);
    const variables = {};
    for (const [key, name] of Object.entries(MERMAID_COLORS)) {
      const value = style.getPropertyValue(name).trim();
      if (/^#[0-9a-f]{3,8}$/i.test(value)) variables[key] = value;
    }
    const surface = style.getPropertyValue("--md-sys-color-surface").trim();
    const rgb = /^#([0-9a-f]{6})$/i.exec(surface);
    if (rgb) {
      const n = parseInt(rgb[1], 16);
      variables.darkMode = ((n >> 16) * 299 + ((n >> 8) & 255) * 587 + (n & 255) * 114) / 1000 < 128;
    }
    const font = style.getPropertyValue("--font-ui").trim();
    if (font) variables.fontFamily = font;
    return variables;
  }

  /// 拼一份在沙箱里画 mermaid 的页面。源码与配置经 JSON 嵌进脚本,`<` 转义防 `</script>` 提前闭合;
  /// securityLevel strict:标签里的 HTML 被净化、click 回调不生效。画挂了显示报错原文,不留空框。
  function mermaidDocument(source) {
    const config = JSON.stringify({ source, themeVariables: mermaidTheme() }).replace(/</g, "\\u003c");
    const errorColor = getComputedStyle(document.body).getPropertyValue("--md-sys-color-error").trim() || "#c0392b";
    return "<!doctype html><html><head><meta charset=\"utf-8\"><style>" +
      "html,body{margin:0;background:transparent}" +
      "#out{display:flex;justify-content:center;padding:4px 0}#out svg{max-width:100%;height:auto}" +
      `#err{margin:0;padding:8px;font:12px/1.5 monospace;white-space:pre-wrap;color:${/^#[0-9a-f]{3,8}$/i.test(errorColor) ? errorColor : "#c0392b"}}` +
      "</style></head><body><div id=\"out\"></div><pre id=\"err\" hidden></pre>" +
      `<script src="${MERMAID_URL}"><\/script>` +
      `<script>(async()=>{const c=${config};const err=document.getElementById("err");` +
      "try{if(!window.mermaid)throw new Error('mermaid 库没加载上');" +
      // suppressErrorRendering:语法错时 mermaid 默认往 body 塞一张「炸弹」SVG,和下面的报错原文重复。
      "mermaid.initialize({startOnLoad:false,securityLevel:'strict',suppressErrorRendering:true,theme:'base',themeVariables:c.themeVariables,fontFamily:c.themeVariables.fontFamily});" +
      "const r=await mermaid.render('gqy-mermaid',c.source);document.getElementById('out').innerHTML=r.svg;}" +
      "catch(e){err.hidden=false;err.textContent='Mermaid 渲染失败:'+(e&&e.message||e);}})()<\/script>" +
      "</body></html>";
  }

  function note(text) {
    const node = document.createElement("div");
    node.className = "fence-preview-note";
    node.textContent = text;
    return node;
  }

  function modeButton(label) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    return button;
  }

  /// 给一个已经搭好的代码块加预览。不认识的语言、空围栏原样返回 false,代码块不受影响。
  function decorate({ wrapper, toolbar, pre, language, source, streaming = false }) {
    const kind = KINDS[String(language || "").toLowerCase()];
    const text = String(source || "");
    if (!kind || !text.trim()) return false;

    const view = document.createElement("div");
    view.className = `fence-preview is-${kind}`;
    const switcher = document.createElement("div");
    switcher.className = "fence-mode-switch";
    switcher.setAttribute("role", "group");
    switcher.setAttribute("aria-label", "查看方式");
    const previewButton = modeButton("预览");
    const sourceButton = modeButton("源码");
    switcher.append(previewButton, sourceButton);

    const setMode = (mode, persist = true) => {
      const preview = mode === "preview";
      if (persist) remember(modes, text, mode);
      view.hidden = !preview;
      pre.hidden = preview;
      previewButton.classList.toggle("active", preview);
      sourceButton.classList.toggle("active", !preview);
      previewButton.setAttribute("aria-pressed", String(preview));
      sourceButton.setAttribute("aria-pressed", String(!preview));
    };
    previewButton.addEventListener("click", () => setMode("preview"));
    sourceButton.addEventListener("click", () => setMode("source"));

    if (kind === "svg") {
      const image = document.createElement("img");
      image.alt = "SVG 预览";
      image.decoding = "async";
      // 写坏的 SVG 解码失败:退回源码,不留一个裂图(不记进偏好,修好了还能画)。
      image.addEventListener("error", () => {
        view.replaceChildren(note("SVG 解析失败,见源码"));
        setMode("source", false);
      });
      image.src = svgUrl(text);
      view.appendChild(image);
    } else if (streaming) {
      view.appendChild(note("回复完成后渲染"));
    } else if (kind === "mermaid") {
      const frame = htmlFrame(mermaidDocument(text));
      frame.title = "Mermaid 预览";
      view.appendChild(frame);
    } else {
      view.appendChild(htmlFrame(text));
    }

    // 复制按钮是工具栏最后一个元素,切换器放它前面。
    toolbar.insertBefore(switcher, toolbar.lastElementChild);
    wrapper.insertBefore(view, pre);
    wrapper.classList.add("has-fence-preview");
    setMode(modes.get(text) || "preview", false);
    return true;
  }

  return { decorate };
})();
