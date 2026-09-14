"use strict";

/*
 * 上下文分项弹窗:输入框下方的上下文圆环点开后看到的分项占用。
 * 计划:docs/plan/2026-09-14/context-panel.md;接口:GET /api/sessions/{id}/context/breakdown。
 *
 * 口径跟后端走,前端不另算:
 * - 顶部总数有实测用实测,否则估算;分项一律是 o200k 估算。
 * - 实测 − 估算合计单列「分词器差异」,不按比例摊进各分项。
 * - 中转后端(claude-code / codex / agy)的差额是「CLI 自带(推算)」。
 * - 「占用最多」是分项内部的明细,在分界线以下,不参与合计。
 *
 * 单独成文件:app.js 已经上万行(与 todos.js / diff.js 同构)。弹窗挂在 dock 上而不是
 * .composer 里——后者 overflow:hidden 会把它裁掉,和模型菜单同一个原因。
 */
window.MiyuContextPanel = (() => {
  const LABELS = {
    system: "系统提示词",
    tools_full: "工具 · 常驻",
    tools_stub: "工具 · stub",
    mcp: "MCP 工具",
    skills: "技能",
    summary: "压缩摘要",
    fossil: "化石化瞬态",
    messages: "消息历史",
  };
  const LIST_ORDER = ["messages", "mcp", "tools_stub", "tools_full", "skills", "fossil", "system", "summary"];
  const BAR_ORDER = ["system", "tools_full", "tools_stub", "mcp", "skills", "summary", "fossil", "messages"];
  const HELP = {
    system: "system 消息与预设对话",
    tools_full: "以完整 schema 发给模型的工具定义",
    tools_stub: "懒工具常驻的「真名 + 摘要 + 宽松参数壳」,完整契约用 load_tools 才展开",
    mcp: "MCP 服务器登记进来的工具定义",
    skills: "load_skill 取回的技能正文",
    summary: "压缩产生的摘要行",
    fossil: "runtime、联想记忆、提醒等发送过的瞬态内容,落库后逐字节回放",
    messages: "用户消息、她的回复、工具调用与结果",
    diff: "供应商实测总数 − 本地 o200k 估算合计。分项只能按估算拆,差额不摊进各项",
    cli: "单次请求实测 − Miyu 发过去部分的估算。CLI 自己的系统提示词与原生工具 Miyu 看不到,这是推算,含分词器差异",
    buffer: "到自动压缩水位就开始压缩,这段实际用不到",
    deferred: "load_tools 能展开但还没展开的完整契约;展开前不占上下文",
  };
  const KIND_LABELS = { tool_result: "工具结果", tool_call: "工具调用", assistant: "她的回复", user: "用户消息" };
  const BACKEND_LABELS = { claude_code: "claude-code", codex: "codex", antigravity: "agy" };

  let ctx = null;
  let pop = null;
  let open = false;
  let data = null;
  let loadedSession = "";
  let loadError = "";
  let seq = 0;
  let refreshTimer = 0;
  // idle | confirm | running | done | empty | failed
  let compact = { phase: "idle", from: null, message: "" };

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text != null) node.textContent = text;
    return node;
  }
  const fmt = (value) => ctx.formatTokens(Math.abs(Number(value) || 0));
  const signed = (value) => `${value > 0 ? "+" : value < 0 ? "−" : ""}${fmt(value)}`;
  const pct = (value, windowSize) => (windowSize > 0 ? `${((Math.abs(value) / windowSize) * 100).toFixed(1)}%` : "");
  const usedOf = (payload) => payload.measured_tokens ?? payload.estimate_tokens;

  function mount(options) {
    ctx = options;
    pop = options.pop;
    if (!ctx?.trigger || !pop) return;
    ctx.trigger.addEventListener("click", () => (open ? close() : show()));
    document.addEventListener("keydown", (event) => {
      if (!open || event.key !== "Escape") return;
      if (compact.phase === "confirm") {
        compact.phase = "idle";
        render();
        return;
      }
      close({ restoreFocus: true });
    });
    document.addEventListener("pointerdown", (event) => {
      if (!open || pop.contains(event.target) || ctx.trigger.contains(event.target)) return;
      close();
    });
    window.addEventListener("resize", position, { passive: true });
  }

  function show() {
    open = true;
    pop.hidden = false;
    ctx.trigger.setAttribute("aria-expanded", "true");
    if (compact.phase !== "running") compact = { phase: "idle", from: null, message: "" };
    render();
    position();
    load();
    window.requestAnimationFrame(() => pop.querySelector(".ctx-close")?.focus({ preventScroll: true }));
  }

  function close({ restoreFocus = false } = {}) {
    if (!open) return;
    open = false;
    pop.hidden = true;
    ctx.trigger.setAttribute("aria-expanded", "false");
    window.clearTimeout(refreshTimer);
    if (compact.phase === "confirm") compact.phase = "idle";
    if (restoreFocus) ctx.trigger.focus();
  }

  /// 同模型菜单:按圆环实际位置算,右边对齐圆环、浮在上方,夹回视口。手机上是
  /// 底部面板,位置全交给 CSS。
  function position() {
    if (!open) return;
    if (window.matchMedia("(max-width: 640px)").matches) {
      pop.style.right = "";
      pop.style.bottom = "";
      pop.style.maxHeight = "";
      return;
    }
    const dock = ctx.dock.getBoundingClientRect();
    const button = ctx.trigger.getBoundingClientRect();
    const gap = 8;
    const margin = 8;
    const width = pop.offsetWidth * ctx.uiScale();
    const rightGap = Math.min(dock.right - button.right, dock.right - margin - width);
    pop.style.right = `${Math.max(0, ctx.toLayout(rightGap))}px`;
    pop.style.bottom = `${ctx.toLayout(dock.bottom - button.top + gap)}px`;
    pop.style.maxHeight = `${Math.min(660, ctx.toLayout(Math.max(200, button.top - gap - margin)))}px`;
  }

  async function load() {
    const sessionId = String(ctx.getSessionId() || "");
    const mine = ++seq;
    loadError = "";
    if (loadedSession !== sessionId) data = null;
    render();
    if (!sessionId) {
      loadError = "还没有会话";
      render();
      return;
    }
    try {
      const response = await ctx.apiRequest(`/api/sessions/${encodeURIComponent(sessionId)}/context/breakdown`);
      const payload = await response.json();
      if (mine !== seq) return;
      if (response.ok === false) throw new Error(payload?.error || `HTTP ${response.status}`);
      data = payload;
      loadedSession = sessionId;
    } catch (error) {
      if (mine !== seq) return;
      loadError = error?.message || "统计失败";
    }
    if (mine !== seq) return;
    render();
    position();
  }

  /// app.js 的 updateContext 每次刷新圆环都会叫这里:开着就跟着重算(防抖),
  /// 换了会话就关掉,不让旧会话的分项挂在新会话的圆环上。
  function contextChanged() {
    if (!open) return;
    const sessionId = String(ctx.getSessionId() || "");
    if (loadedSession && sessionId !== loadedSession) {
      close();
      return;
    }
    if (compact.phase === "running" || ctx.isRunning?.()) return;
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(load, 800);
  }

  function render() {
    if (!pop || !open) return;
    pop.replaceChildren(renderHead(), renderBody(), renderFoot());
  }

  function help(text) {
    const node = el("span", "ctx-help", "?");
    node.title = text;
    node.setAttribute("aria-label", text);
    return node;
  }

  function row({ name, sub, tokens, numText, windowSize, color, dotClass, dim, zero, helpText }) {
    const node = el("div", `ctx-row${dim ? " is-dim" : ""}${zero ? " is-zero" : ""}`);
    const dot = el("i", `ctx-dot${dotClass ? ` ${dotClass}` : ""}`);
    if (color) dot.style.background = color;
    const label = el("span", "ctx-name", name);
    if (sub) label.appendChild(el("small", null, sub));
    if (helpText) label.appendChild(help(helpText));
    node.append(
      dot,
      label,
      el("span", "ctx-num", numText ?? fmt(tokens)),
      el("span", "ctx-pct", tokens == null ? "" : pct(tokens, windowSize))
    );
    return node;
  }

  function groupLabel(left, right) {
    const node = el("div", "ctx-group-label");
    node.append(el("span", null, left), el("span", null, right || ""));
    return node;
  }

  function renderHead() {
    const head = el("header", "ctx-head");
    const top = el("div", "ctx-head-row");
    top.appendChild(el("span", "ctx-title", "上下文"));
    if (data) {
      const measured = data.measured_tokens != null;
      const badge = el("span", `ctx-badge ${measured ? "is-measured" : "is-estimate"}`, measured ? "实测" : "估算");
      badge.title = measured
        ? "上一回合最后一次请求,供应商报告的真实占用"
        : "本地 o200k 估算:刚压缩完、回合被打断或供应商没报用量时没有实测";
      top.appendChild(badge);
      const backend = BACKEND_LABELS[data.backend?.kind];
      if (backend) top.appendChild(el("span", "ctx-badge", backend));
    }
    const closeButton = el("button", "ctx-close", "✕");
    closeButton.type = "button";
    closeButton.setAttribute("aria-label", "关闭");
    closeButton.addEventListener("click", () => close({ restoreFocus: true }));
    top.appendChild(closeButton);
    head.appendChild(top);

    const fallback = ctx.getContext?.() || {};
    const used = data ? usedOf(data) : Number(fallback.tokens) || 0;
    const windowSize = data ? data.window : fallback.window;
    const total = el("div", "ctx-total");
    total.append(
      el("span", "ctx-used", fmt(used)),
      el("span", "ctx-win", windowSize ? `/ ${fmt(windowSize)}${data?.window_assumed ? " · 按配置" : ""}` : "/ 窗口未知"),
      el("span", "ctx-total-pct", windowSize ? pct(used, windowSize) : "")
    );
    head.appendChild(total);
    if (data && windowSize) head.appendChild(renderBar(data, used, windowSize));
    return head;
  }

  function renderBar(payload, used, windowSize) {
    const wrap = el("div", "ctx-bar-wrap");
    const bar = el("div", "ctx-bar");
    for (const key of BAR_ORDER) {
      const value = payload.categories?.[key] || 0;
      if (!value) continue;
      const segment = el("i");
      segment.style.width = `${(value / windowSize) * 100}%`;
      segment.style.background = `var(--ctx-cat-${key})`;
      segment.title = `${LABELS[key]} ${fmt(value)}`;
      bar.appendChild(segment);
    }
    const extra = used - payload.estimate_tokens;
    if (extra > 0) {
      const segment = el("i", "is-extra");
      segment.style.width = `${(extra / windowSize) * 100}%`;
      segment.title = payload.backend?.kind !== "native" ? "CLI 自带(推算)" : "分词器差异";
      bar.appendChild(segment);
    }
    const trim = Number(payload.thresholds?.trim_at_ratio) || 0.8;
    const force = Number(payload.thresholds?.compact_force_ratio) || 0.9;
    const buffer = el("i", "is-buffer");
    buffer.style.width = `${(1 - trim) * 100}%`;
    buffer.title = "自动压缩缓冲";
    bar.appendChild(buffer);
    wrap.appendChild(bar);
    for (const [ratio, label, className] of [[trim, "压缩", ""], [force, "强制", " is-force"]]) {
      const tick = el("i", `ctx-tick${className}`);
      tick.style.left = `${ratio * 100}%`;
      tick.dataset.label = `${Math.round(ratio * 100)}% ${label}`;
      wrap.appendChild(tick);
    }
    return wrap;
  }

  function renderBody() {
    const body = el("div", "ctx-body");
    if (loadError) {
      body.appendChild(el("div", "ctx-note is-error", `统计失败:${loadError}`));
      return body;
    }
    if (!data) {
      const note = el("div", "ctx-note ctx-loading");
      note.append(el("span", "ctx-spinner"), document.createTextNode("正在统计…"));
      body.appendChild(note);
      return body;
    }
    const relay = data.backend?.kind && data.backend.kind !== "native";
    const windowSize = data.window;
    const used = usedOf(data);

    if (relay) {
      body.appendChild(el("div", "ctx-note",
        "分项只覆盖 Miyu 发给 CLI 的部分。CLI 自己的系统提示词与原生工具看不到,那一行是推算(含分词器差异)。"));
    }
    if (!windowSize) {
      body.appendChild(el("div", "ctx-note", "窗口大小未知,算不出占比与剩余。在 设置 → 模型 里为这个模型填上下文窗口。"));
    }

    body.appendChild(groupLabel("在上下文里", "分项为估算 · o200k"));
    for (const key of LIST_ORDER) {
      const value = data.categories?.[key] || 0;
      body.appendChild(row({
        name: LABELS[key],
        tokens: value,
        windowSize,
        color: `var(--ctx-cat-${key})`,
        zero: value === 0,
        helpText: HELP[key],
      }));
    }
    if (relay && data.backend.cli_overhead_tokens != null) {
      const value = data.backend.cli_overhead_tokens;
      body.appendChild(row({
        name: "CLI 自带", sub: "推算", tokens: value, numText: value < 0 ? signed(value) : fmt(value),
        windowSize, dotClass: "is-cli", dim: true, helpText: HELP.cli,
      }));
    } else if (!relay && data.measured_tokens != null) {
      const diff = data.measured_tokens - data.estimate_tokens;
      if (diff !== 0) {
        body.appendChild(row({
          name: "分词器差异", tokens: diff, numText: signed(diff), windowSize, dotClass: "is-diff", dim: true, helpText: HELP.diff,
        }));
      }
    }

    if (windowSize) {
      const trim = Number(data.thresholds?.trim_at_ratio) || 0.8;
      const buffer = Math.round(windowSize * (1 - trim));
      body.appendChild(groupLabel("窗口余量"));
      body.appendChild(row({ name: "自动压缩缓冲", tokens: buffer, windowSize, dotClass: "is-buffer", dim: true, helpText: HELP.buffer }));
      body.appendChild(row({ name: "剩余空间", tokens: Math.max(0, windowSize - used - buffer), windowSize, dotClass: "is-free", dim: true }));
    }

    if (data.deferred_tools_tokens > 0) {
      body.appendChild(groupLabel("不在上下文里"));
      const deferred = row({ name: "未加载工具", sub: "完整契约", numText: "—", tokens: null, windowSize, dotClass: "is-free", dim: true,
        helpText: `${HELP.deferred}(约 ${fmt(data.deferred_tools_tokens)})` });
      body.appendChild(deferred);
    }

    const items = Array.isArray(data.top) ? data.top : [];
    if (items.length) {
      const divider = el("div", "ctx-divider");
      divider.append(el("strong", null, `占用最多的 ${items.length} 条`), el("span", null, "明细 · 已计入上方分项,不另算"));
      body.appendChild(divider);
      const max = Math.max(1, ...items.map((item) => item.tokens || 0));
      items.forEach((item, index) => {
        const node = el("div", "ctx-top-row");
        const main = el("div", "ctx-top-main");
        const title = el("div", "ctx-top-title");
        const kind = item.category === "skills" ? "技能" : KIND_LABELS[item.kind] || item.kind;
        const label = el("span", "ctx-top-label");
        if (item.label) label.appendChild(document.createTextNode(`${item.label} `));
        if (item.preview) label.appendChild(el("code", null, item.preview));
        label.title = `${item.label ? `${item.label} ` : ""}${item.preview || ""}`;
        title.append(el("span", "ctx-top-kind", kind), label);
        const where = item.turn_index ? `第 ${item.turn_index} 回合` : "回合未对上";
        const sub = el("div", "ctx-top-sub", `${where} · 属于「${LABELS[item.category] || item.category}」`);
        const meter = el("div", "ctx-top-meter");
        const fill = el("i");
        fill.style.width = `${((item.tokens || 0) / max) * 100}%`;
        fill.style.background = `var(--ctx-cat-${item.category})`;
        meter.appendChild(fill);
        main.append(title, sub, meter);
        node.append(el("span", "ctx-top-rank", String(index + 1)), main, el("span", "ctx-top-num", fmt(item.tokens)));
        body.appendChild(node);
      });
    }
    return body;
  }

  function renderFoot() {
    const foot = el("footer", "ctx-foot");
    if (compact.phase === "confirm") {
      const confirm = el("div", "ctx-confirm");
      confirm.appendChild(el("div", null,
        "用摘要替换较早的历史,最近的回合原样保留。压缩后缓存前缀会重建一次,下一轮输入按全价计。"));
      const actions = el("div", "ctx-confirm-actions");
      const cancel = el("button", "ctx-btn is-ghost", "取消");
      cancel.type = "button";
      cancel.addEventListener("click", () => {
        compact.phase = "idle";
        render();
      });
      const ok = el("button", "ctx-btn is-primary", "确定压缩");
      ok.type = "button";
      ok.addEventListener("click", runCompact);
      actions.append(cancel, ok);
      confirm.appendChild(actions);
      foot.appendChild(confirm);
      window.requestAnimationFrame(() => ok.focus({ preventScroll: true }));
    }

    const line = el("div", "ctx-foot-row");
    const trim = Number(data?.thresholds?.trim_at_ratio) || 0.8;
    const force = Number(data?.thresholds?.compact_force_ratio) || 0.9;
    line.appendChild(el("span", "ctx-thresholds", `到 ${Math.round(trim * 100)}% 自动压缩 · ${Math.round(force * 100)}% 强制`));
    if (compact.phase === "done" && data && compact.from != null) {
      line.appendChild(el("span", "ctx-result", `已压缩 ${fmt(compact.from)} → ${fmt(usedOf(data))}`));
    } else if (compact.phase === "empty" || compact.phase === "failed") {
      line.appendChild(el("span", `ctx-result${compact.phase === "failed" ? " is-error" : ""}`, compact.message));
    }

    const running = Boolean(ctx.isRunning?.());
    const button = el("button", "ctx-btn");
    button.type = "button";
    if (compact.phase === "running") {
      button.disabled = true;
      button.append(el("span", "ctx-spinner"), document.createTextNode("压缩中…"));
    } else {
      button.textContent = "立即压缩";
      button.disabled = running || compact.phase === "confirm" || !ctx.getSessionId();
      if (running) button.title = "回合进行中,两轮之间才能压缩";
      button.addEventListener("click", () => {
        compact = { phase: "confirm", from: null, message: "" };
        render();
      });
    }
    line.appendChild(button);
    foot.appendChild(line);
    return foot;
  }

  async function runCompact() {
    const sessionId = String(ctx.getSessionId() || "");
    compact = { phase: "running", from: data ? usedOf(data) : null, message: "" };
    render();
    try {
      const response = await ctx.apiRequest("/api/conversation/compact", {
        method: "POST",
        body: JSON.stringify({ session_id: sessionId }),
      });
      const payload = await response.json();
      if (response.ok === false) throw new Error(payload?.error || `HTTP ${response.status}`);
      // compact_now 返回空 = 整段对话都还在保留的尾巴以内,没有更早的回合可折(见 commands.js /compact)。
      if (payload?.result?.compacted === true) {
        compact.phase = "done";
        await ctx.onCompacted?.(sessionId);
        await load();
      } else {
        compact = { phase: "empty", from: null, message: "对话都还在保留的尾巴里,没有更早的回合可折叠" };
        render();
      }
    } catch (error) {
      compact = { phase: "failed", from: null, message: `压缩失败:${error?.message || "未知错误"}` };
      render();
    }
  }

  return { mount, contextChanged, close };
})();
