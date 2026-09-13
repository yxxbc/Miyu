/*
 * 图库面板(09-13)。
 *
 * 和表情包面板长得像,但**故意薄**:图库没有内置层、没有视觉识图、没有平台
 * 使用计数,所以这里只有「网格 → 抽屉 → 改名/描述/标签 → 删除」加一个上传。
 * 搜索也只是名称/描述/标签的关键词匹配,与工具层 `album action=search` 同口径,
 * 前端复算一遍,让你看到的顺序就是她会拿到的顺序。
 *
 * 库作用域不在这里选:图库按人格分库,用哪个库由后端按登录者的当前人格算
 * (`album_root`),面板跟着走——表情包那种「切库下拉」在这里没有对应物。
 * 视觉上复用 .dash-gallery / .dash-meme 那套瀑布流,不另起一套样式。
 */
(() => {
  const D = window.MiyuDash;
  if (!D) return;

  const state = {
    listing: null,
    q: "",
    tag: "",
    loadSeq: 0,
    selecting: false,
    selected: new Set(),
    galleryObserver: null
  };
  const ui = {};

  const imageUrl = (item) => `/api/dash/album/image?${new URLSearchParams({ id: item.id })}`;

  function formatSize(bytes) {
    const value = Number(bytes) || 0;
    if (value >= 1024 * 1024) return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
    if (value >= 1024) return `${(value / 1024).toFixed(0)} KiB`;
    return `${value} B`;
  }

  /* 与工具层 search 同一套打分:命中一个词算一分,名称/描述/标签同权。 */
  function score(item, query) {
    const words = query.toLowerCase().split(/[\s,，]+/).filter(Boolean);
    if (!words.length) return 0;
    const hay = `${item.name} ${item.description} ${(item.tags || []).join(" ")}`.toLowerCase();
    return words.filter((word) => hay.includes(word)).length;
  }

  /* ── 挂载 ─────────────────────────────────────────────── */
  function mount(root) {
    root.textContent = "";
    ui.stamp = D.el("small", { text: "" });
    const head = D.el("div.con-head", null,
      D.el("h2", { text: "图库" }),
      D.iconButton("refresh-cw", "刷新", () => load()),
      ui.stamp,
      D.el("span.dash-scope", null,
        D.el("button.dash-button.is-primary", { type: "button", onclick: openUpload }, D.icon("plus"), "上传")));

    ui.cards = D.el("div");
    ui.tags = D.el("div.dash-tag-cloud");
    ui.search = D.el("input.dash-search", { type: "search", placeholder: "按名称、描述、标签搜索…", oninput: () => {
      clearTimeout(ui.searchTimer);
      ui.searchTimer = setTimeout(() => { state.q = ui.search.value.trim(); renderGallery(); }, 200);
    } });
    const toolbar = D.el("div.dash-toolbar", null,
      D.el("label.dash-search-box", null, D.icon("search"), ui.search),
      D.el("button.dash-button", { type: "button", title: "进入选择模式,批量删除", onclick: () => setSelecting(!state.selecting) }, D.icon("check-square"), "选择"));
    ui.selectButton = toolbar.lastChild;
    ui.hint = D.el("p.dash-search-hint", { hidden: true });
    ui.bulk = D.el("div");
    ui.gallery = D.el("div.dash-gallery.is-masonry");
    root.append(head, ui.cards, toolbar, ui.tags, ui.hint, ui.bulk, ui.gallery);
    watchGalleryWidth();
    load();
  }

  async function load() {
    const seq = ++state.loadSeq;
    ui.stamp.textContent = "载入中…";
    try {
      const listing = await D.api("/api/dash/album/items");
      if (seq !== state.loadSeq) return;
      state.listing = listing;
      state.selected.clear();
      renderCards();
      renderTags();
      renderGallery();
      ui.stamp.textContent = `${listing.total} 张 · ${formatSize(listing.bytes)}`;
    } catch (error) {
      if (seq !== state.loadSeq) return;
      ui.gallery.replaceChildren(D.el("p.dash-empty", { text: `加载失败:${error.message}` }));
      ui.stamp.textContent = "";
    }
  }

  function renderCards() {
    const listing = state.listing;
    const described = listing.items.filter((item) => item.description.trim()).length;
    ui.cards.replaceChildren(D.statCards([
      { label: "总数", value: listing.total, hint: listing.root },
      { label: "占用", value: formatSize(listing.bytes), hint: "库目录里图片文件的总和" },
      // 没写描述的那些,`album action=search` 永远搜不到——面板上要能一眼看见。
      { label: "可被搜到", value: described, hint: described < listing.total ? `${listing.total - described} 张没有描述,她搜不到` : "每张都有描述" },
      { label: "文件缺失", value: listing.missing, hint: listing.missing ? "索引里有、盘上没有,点开可删掉条目" : "索引与文件一致" }
    ]));
  }

  function renderTags() {
    const counts = new Map();
    for (const item of state.listing.items) for (const tag of item.tags || []) counts.set(tag, (counts.get(tag) || 0) + 1);
    const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 24);
    ui.tags.textContent = "";
    for (const [tag, count] of top) {
      ui.tags.append(D.el(`button.dash-chip.is-clickable${state.tag === tag ? ".is-active" : ""}`, { type: "button", text: `${tag} ${count}`, onclick: () => { state.tag = state.tag === tag ? "" : tag; renderTags(); renderGallery(); } }));
    }
  }

  function visibleItems() {
    let items = state.listing.items;
    if (state.tag) items = items.filter((item) => (item.tags || []).includes(state.tag));
    if (state.q) {
      items = items.map((item) => ({ item, s: score(item, state.q) })).filter((entry) => entry.s > 0)
        .sort((a, b) => b.s - a.s).map((entry) => entry.item);
    }
    return items;
  }

  function renderGallery() {
    const items = visibleItems();
    ui.gallery.textContent = "";
    if (state.q) {
      ui.hint.hidden = false;
      const names = items.slice(0, 3).map((item) => item.name);
      ui.hint.textContent = names.length ? `她搜“${state.q}”会先拿到:${names.join(" · ")}` : `她搜“${state.q}”什么也拿不到`;
    } else {
      ui.hint.hidden = true;
    }
    if (!items.length) {
      ui.gallery.append(D.el("p.dash-empty", { text: state.listing.total ? "没有匹配的图片。" : "图库还是空的,上传几张吧。" }));
      return;
    }
    renderBulk(items);
    for (const item of items) {
      const activate = () => { if (state.selecting) toggleSelected(item, card); else openDetail(item); };
      const thumbImage = D.el("img", { src: imageUrl(item), alt: item.name, loading: "lazy", decoding: "async" });
      const card = D.el("figure.dash-meme", { tabindex: "0", onclick: activate, onkeydown: (event) => { if (event.key === "Enter" || (state.selecting && event.key === " ")) { event.preventDefault(); activate(); } } },
        D.el("div.dash-meme-thumb", null,
          state.selecting ? D.el("span.dash-meme-check", { "aria-hidden": "true" }, D.icon("check")) : null,
          thumbImage,
          item.missing ? D.el("span.dash-meme-badge", { text: "文件缺失" }) : null),
        D.el("figcaption.dash-meme-cap", null,
          D.el("span.dash-meme-name", { text: item.name }),
          item.description.trim() ? null : D.el("span.dash-chip.is-warn", { text: "无描述" })));
      card.classList.toggle("is-disabled", item.missing);
      card.classList.toggle("is-selectable", state.selecting);
      card.classList.toggle("is-selected", state.selected.has(item.id));
      ui.gallery.append(card);
      fitThumb(card, thumbImage);
    }
    relayout();
  }

  /* ── 瀑布流(与表情包面板同一套算法:1px 行高 + 按实际高度算跨行数) ── */
  const THUMB_MIN_RATIO = 0.55;
  const THUMB_MAX_RATIO = 1.9;

  function applyRatio(card, image) {
    const thumb = card.firstElementChild;
    if (!thumb || !image.naturalWidth || !image.naturalHeight) return;
    thumb.style.aspectRatio = String(Math.min(Math.max(image.naturalWidth / image.naturalHeight, THUMB_MIN_RATIO), THUMB_MAX_RATIO));
    layoutCard(card);
  }
  function fitThumb(card, image) {
    if (image.complete) applyRatio(card, image);
    else image.addEventListener("load", () => applyRatio(card, image), { once: true });
    image.addEventListener("error", () => layoutCard(card), { once: true });
  }
  function layoutCard(card) {
    if (!card.isConnected) return;
    const height = Math.ceil(card.getBoundingClientRect().height);
    if (height <= 0) return;
    const gap = Math.ceil(parseFloat(getComputedStyle(card).marginBottom) || 0);
    card.style.gridRowEnd = `span ${height + gap}`;
  }
  function relayout() {
    for (const card of ui.gallery.querySelectorAll(".dash-meme")) layoutCard(card);
  }
  function watchGalleryWidth() {
    if (!window.ResizeObserver || state.galleryObserver) return;
    let width = 0;
    state.galleryObserver = new ResizeObserver((entries) => {
      const next = Math.round(entries[0]?.contentRect?.width || 0);
      if (next === width) return;
      width = next;
      relayout();
    });
    state.galleryObserver.observe(ui.gallery);
  }

  /* ── 选择模式 / 批量 ────────────────────────────────── */
  function setSelecting(on) {
    state.selecting = on;
    if (!on) state.selected.clear();
    ui.selectButton.classList.toggle("is-primary", on);
    ui.selectButton.lastChild.textContent = on ? "退出选择" : "选择";
    renderGallery();
  }
  function toggleSelected(item, card) {
    if (state.selected.has(item.id)) state.selected.delete(item.id); else state.selected.add(item.id);
    card.classList.toggle("is-selected", state.selected.has(item.id));
    renderBulk(visibleItems());
  }
  function renderBulk(items) {
    if (!state.selecting) { ui.bulk.textContent = ""; return; }
    ui.bulk.replaceChildren(D.bulkBar({
      count: state.selected.size,
      total: items.length,
      noun: "张",
      onAll: () => { for (const item of items) state.selected.add(item.id); renderGallery(); },
      onNone: () => { state.selected.clear(); renderGallery(); },
      actions: [{ label: "删除所选", icon: "trash-2", danger: true, onClick: () => removeSelected(items) }]
    }));
  }
  async function removeSelected(items) {
    const picked = items.filter((item) => state.selected.has(item.id));
    if (!picked.length) return;
    const ok = await D.confirmAction(`删除选中的 ${picked.length} 张?条目从索引移除,图片文件保留在库目录里。`, "删除");
    if (!ok) return;
    await D.runBatch(picked, (item) => D.api(`/api/dash/album/items/${encodeURIComponent(item.id)}?hard=false`, { method: "DELETE" }), "删除");
    setSelecting(false);
    await load();
  }

  /* ── 详情抽屉 ────────────────────────────────────────── */
  function openDetail(item) {
    const form = {
      name: D.el("input.dash-select.dash-wide", { type: "text", maxlength: "80", value: item.name }),
      description: D.el("textarea.dash-textarea", { rows: "3", maxlength: "500", value: item.description }),
      tags: D.el("input.dash-select.dash-wide", { type: "text", value: (item.tags || []).join(", ") })
    };
    const meta = [
      ["id", item.id],
      ["文件", item.file],
      ["大小", item.bytes ? formatSize(item.bytes) : "—"],
      ["存入", item.saved_at],
      ["来源", item.source || "—"]
    ];
    const body = D.el("div", null,
      item.missing
        ? D.el("p.dash-empty", { text: "文件已经不在库目录里了。删掉这条,或者把图放回去。" })
        : D.el("div.dash-meme-hero", null, D.el("img.dash-meme-full", { src: imageUrl(item), alt: item.name })),
      D.field("名字", form.name),
      D.field("描述", form.description, "她只能按名称、描述、标签找图——描述空着等于这张图找不到"),
      D.field("标签", form.tags, "逗号分隔"),
      D.el("h4.dash-section", { text: "元数据" }),
      D.el("dl.dash-meta", null, meta.flatMap(([key, value]) => [D.el("dt", { text: key }), D.el("dd", { text: String(value) })])));

    const remove = D.el("button.dash-button.is-danger", { type: "button", text: "删除", onclick: () => removeItem(item) });
    const save = D.el("button.dash-button.is-primary", { type: "button", text: "保存", onclick: async () => {
      try {
        await D.api(`/api/dash/album/items/${encodeURIComponent(item.id)}`, { method: "PATCH", body: {
          name: form.name.value,
          description: form.description.value,
          tags: form.tags.value.split(/[,，、]/).map((tag) => tag.trim()).filter(Boolean)
        } });
        D.toast("已保存");
        D.closeDrawer();
        await load();
      } catch (error) {
        D.toast(`保存失败:${error.message}`, "error");
      }
    } });
    D.openDrawer(item.name, body, [remove, save]);
  }

  async function removeItem(item) {
    const ok = await D.confirmAction(`删除「${item.name}」?\n\n条目从索引移除,图片文件保留在库目录里。`, "删除");
    if (!ok) return;
    try {
      await D.api(`/api/dash/album/items/${encodeURIComponent(item.id)}?hard=false`, { method: "DELETE" });
      D.toast("已删除");
      D.closeDrawer();
      await load();
    } catch (error) {
      D.toast(`删除失败:${error.message}`, "error");
    }
  }

  /* ── 上传抽屉 ────────────────────────────────────────── */
  function openUpload() {
    const files = D.el("input", { type: "file", multiple: true, accept: "image/*" });
    const form = {
      description: D.el("textarea.dash-textarea", { rows: "2", maxlength: "500", placeholder: "图上是什么" }),
      tags: D.el("input.dash-select.dash-wide", { type: "text", placeholder: "逗号分隔" })
    };
    const log = D.el("ul.dash-upload-list");
    const body = D.el("div", null,
      D.field("图片", files, "PNG / JPEG / GIF / WebP / BMP / HEIC / TIFF,单张 ≤ 24 MiB"),
      D.field("描述", form.description, "多张共用同一段描述;传完可以逐张改"),
      D.field("标签", form.tags),
      D.el("h4.dash-section", { text: "结果" }), log);
    const submit = D.el("button.dash-button.is-primary", { type: "button", text: "开始上传", onclick: async () => {
      const list = Array.from(files.files || []);
      if (!list.length) { D.toast("先选图片", "error"); return; }
      submit.disabled = true;
      let added = 0;
      for (const file of list) {
        const row = D.el("li", null, D.el("span.dash-cell-mono", { text: file.name }), D.el("span.dash-chip", { text: "上传中…" }));
        log.append(row);
        const chip = row.lastChild;
        try {
          const params = new URLSearchParams({
            filename: file.name,
            // 名字留空 = 用文件名的主干,后端算;这里不替用户猜。
            name: "",
            description: form.description.value,
            tags: form.tags.value
          });
          const response = await fetch(`/api/dash/album/items?${params}`, { method: "POST", body: await file.arrayBuffer(), headers: { "content-type": "application/octet-stream" } });
          const payload = await response.json().catch(() => null);
          if (!response.ok) throw new Error(payload?.error?.message || `HTTP ${response.status}`);
          chip.textContent = `已入库:${payload.item?.name || ""}`;
          chip.className = "dash-chip is-active";
          added += 1;
        } catch (error) {
          chip.textContent = `失败:${error.message}`;
          chip.className = "dash-chip is-danger";
        }
      }
      submit.disabled = false;
      if (added) { D.toast(`入库 ${added} 张`); await load(); }
    } });
    D.openDrawer("上传到图库", body, [submit]);
  }

  D.register({ name: "album", root: "dashAlbumRoot", mount, refresh: () => load() });
})();
