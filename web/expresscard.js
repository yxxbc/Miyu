"use strict";

/*
 * 快递卡片:`express_query` 的结果画成一条时间线。
 *
 * 卡片的重点不是好看,是**把「查到了」和「没查到」画成两种明显不同的东西**。
 * 工具在没有 key、认不出快递公司、顺丰缺手机号时都会成功返回,只是 `state`
 * 不是 `ok`、轨迹是空的——这几种情况下卡片画的是一条说明加官方查询入口,绝不
 * 摆出一个空时间线让人以为「就是没有物流信息」。
 *
 * 最新一条在最上面(上游按 desc 给),并且单独描粗:看快递就是看最新那条。
 */
window.MiyuExpress = (() => {
  const STATE_NOTE = {
    no_credentials: "这台机器没配快递 100 的 key,查不了实时轨迹。配置在 plugins.express。",
    unknown_company: "认不出这是哪家快递的单号,告诉她快递公司再查一次。",
    needs_phone: "顺丰要收件人手机号(后四位就行)才肯给轨迹。",
    provider_error: "快递 100 那边没给出结果。"
  };

  function isExpressTool(name) {
    return String(name || "") === "express_query";
  }

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }

  function parse(output) {
    let payload = null;
    try {
      payload = JSON.parse(String(output || ""));
    } catch (_) {
      return null;
    }
    // 有单号就画得出卡片:查不到也是一种要显示的结果。
    if (!payload || typeof payload !== "object" || !payload.number) return null;
    return payload;
  }

  /** 已签收给绿、疑难/退回给红,其余中性。 */
  function statusClass(payload) {
    const code = String(payload.status_code || "");
    if (code === "3") return "is-done";
    if (code === "2" || code === "4" || code === "6" || code === "14") return "is-warn";
    return "";
  }

  function copyButton(text) {
    const button = el("button", "express-mini-button", "复制单号");
    button.type = "button";
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(text);
        button.textContent = "已复制";
      } catch (_) {
        button.textContent = "复制不了";
      }
      setTimeout(() => { button.textContent = "复制单号"; }, 1600);
    });
    return button;
  }

  function renderCard(output) {
    const payload = parse(output);
    if (!payload) return null;
    const traces = Array.isArray(payload.traces) ? payload.traces : [];
    const card = el("section", `express-card${statusClass(payload) ? ` ${statusClass(payload)}` : ""}`);

    const head = el("header", "express-head");
    const title = el("div", "express-title");
    title.append(el("strong", null, payload.company_name || payload.company || "快递"));
    title.append(el("code", "express-number", payload.number));
    head.append(title);
    if (payload.state === "ok") {
      head.append(el("span", "express-status", payload.status || "查询完成"));
    } else {
      head.append(el("span", "express-status is-muted", "未查到轨迹"));
    }
    card.append(head);

    if (payload.state !== "ok") {
      const note = STATE_NOTE[payload.state] || payload.note || "这次没查到物流信息。";
      card.append(el("p", "express-note", note));
    } else if (!traces.length) {
      // 查通了但一条轨迹都没有:多半是刚下单还没揽收。这句话要说清楚,
      // 否则一个空卡片看起来就像坏了。
      card.append(el("p", "express-note", "快递公司那边还没有轨迹,通常是刚下单、还没揽收。"));
    }

    if (traces.length) {
      const list = el("ol", "express-traces");
      traces.forEach((trace, index) => {
        const item = el("li", `express-trace${index === 0 ? " is-latest" : ""}`);
        item.append(el("time", "express-trace-time", trace.time || ""));
        const body = el("div", "express-trace-body");
        body.append(el("p", "express-trace-text", trace.text || ""));
        if (trace.location) body.append(el("span", "express-trace-place", trace.location));
        item.append(body);
        list.append(item);
      });
      card.append(list);
    }

    const foot = el("footer", "express-foot");
    foot.append(copyButton(payload.number));
    if (payload.official_url) {
      const link = el("a", "express-mini-button", "官方查询页");
      link.href = payload.official_url;
      link.target = "_blank";
      link.rel = "noreferrer noopener";
      foot.append(link);
    }
    card.append(foot);
    return card;
  }

  return { isExpressTool, renderCard };
})();
