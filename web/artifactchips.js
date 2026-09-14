"use strict";

/*
 * 回复气泡底部的产物 chip(docs/plan-is-true/2026-09-14/webui-delivery.md §5)。
 *
 * 数据早就有:后端按回合分好 `turn.artifacts`,实时的 `tool.artifact` 事件带着同一份源对象。
 * 缺的只是画在产出它的那一轮上——侧栏一关,就只剩顶栏一个小图标。
 *
 * **只露类型 + 截断名。** 文件名本身可能敏感(「XX 体检报告.md」),气泡又是截图、投屏
 * 最常露出去的地方。全名只在悬停提示里。
 *
 * 同一回合改写同一文件,后端按 (turn_id, source_key) upsert、id 不变
 * (`src/state/conversation_db/attachments.rs` 的 ON CONFLICT),所以按 id 去重就够。
 */
window.GqyArtifactChips = (() => {
  const NAME_LIMIT = 14;

  /// 中间截断、保留扩展名:「体检报告2026版.md」→「体检报告20….md」。按字符数不按字节,中文不劈半。
  function shortName(name) {
    const text = String(name || "").trim();
    if ([...text].length <= NAME_LIMIT) return text;
    const dot = text.lastIndexOf(".");
    const extension = dot > 0 && text.length - dot <= 6 ? text.slice(dot) : "";
    const stem = [...text.slice(0, text.length - extension.length)];
    const keep = Math.max(4, NAME_LIMIT - [...extension].length - 1);
    return `${stem.slice(0, keep).join("")}…${extension}`;
  }

  function chip(artifact, options) {
    const item = document.createElement("div");
    item.className = "artifact-chip";
    item.dataset.artifactId = artifact.id;

    const open = document.createElement("button");
    open.type = "button";
    open.className = "artifact-chip-open";
    open.title = `在预览工作区打开 ${artifact.name}`;
    const type = document.createElement("span");
    type.className = "artifact-chip-type";
    type.textContent = options.typeLabel(artifact);
    const name = document.createElement("span");
    name.className = "artifact-chip-name";
    name.textContent = shortName(artifact.name);
    open.append(options.iconSlot(options.iconName(artifact)), type, name);
    open.addEventListener("click", () => options.onOpen(artifact));

    // 不开侧栏也能拿走文件:`?download=1` 后端强制 attachment(assets.rs artifact_asset)。
    const download = document.createElement("a");
    download.className = "artifact-chip-download";
    download.href = `${artifact.url}?download=1`;
    download.setAttribute("download", "");
    download.title = "下载";
    download.setAttribute("aria-label", `下载 ${artifact.name}`);
    download.appendChild(options.iconSlot("download"));

    item.append(open, download);
    return item;
  }

  /// 把 host 底部那一排 chip 同步成 artifacts:按 id 去重,后到的覆盖先到的、位置不变。
  /// 没有产物就拿掉整排。实时与刷新后走同一个函数,两边长得一样。
  function sync(host, artifacts, options) {
    if (!host) return;
    const byId = new Map();
    for (const source of Array.isArray(artifacts) ? artifacts : []) {
      const artifact = options.normalize(source);
      if (artifact) byId.set(artifact.id, artifact);
    }
    let row = host.querySelector(":scope > .assistant-artifacts");
    if (!byId.size) {
      row?.remove();
      return;
    }
    if (!row) {
      row = document.createElement("div");
      row.className = "assistant-artifacts";
      host.appendChild(row);
    }
    row.replaceChildren(...[...byId.values()].map((artifact) => chip(artifact, options)));
  }

  return { sync, shortName };
})();
