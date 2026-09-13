"use strict";

/*
 * 地图卡片:`map_search` 的结果画成一张真地图,而不是一条网址。
 *
 * 以前她回答「XX 在哪」只能贴一条高德链接,链接卡片再把它升级成一张没有地图的
 * 缩略图卡——要看地图得点出去。现在工具返回坐标,这里把坐标画在瓦片上。
 *
 * **瓦片走 `/api/map/tile`。** WebUI 的 CSP 是 `img-src 'self'`,浏览器取不到
 * 第三方图片;放宽它等于让每张地图在用户浏览器上向瓦片服务器留下几十条带 IP
 * 的记录。所以瓦片由 daemon 代取并缓存(见 `src/web/map_api.rs`)。
 *
 * **自己写的切片地图,没有引第三方库。** 要的只有平移、缩放、几个标记点和一个
 * 放大视图;为这点功能引一个几百 KB 的地图库,还要连它的图标与 CSS 一起 vendored,
 * 不划算。整套算法就是 Web Mercator 的两个公式加一层瓦片网格。
 *
 * **坐标系按源走。** 高德的瓦片是 GCJ-02、OSM 的瓦片是 WGS-84,国内差几百米。
 * 工具两套坐标都给了,这里按 provider 取对应那套——取错的表现是标记稳定地落在
 * 隔壁街,而地图本身看起来完全正常,最难发现的那种错。
 */
window.MiyuMap = (() => {
  const SVG_NS = "http://www.w3.org/2000/svg";
  const TILE = 256;
  const MIN_ZOOM = 3;
  const MAX_ZOOM = 18;

  const PROVIDER_LABEL = { osm: "© OpenStreetMap 贡献者", amap: "© 高德地图" };

  function isMapTool(name) {
    return String(name || "") === "map_search";
  }

  /* ── Web Mercator ───────────────────────────────────────────────── */
  function project(lon, lat, zoom) {
    const scale = TILE * Math.pow(2, zoom);
    // 纬度夹在 ±85.05°:再往极地走 Mercator 的 y 趋于无穷。
    const clamped = Math.max(-85.05112878, Math.min(85.05112878, lat));
    const sin = Math.sin((clamped * Math.PI) / 180);
    return {
      x: ((lon + 180) / 360) * scale,
      y: (0.5 - Math.log((1 + sin) / (1 - sin)) / (4 * Math.PI)) * scale
    };
  }
  function unproject(x, y, zoom) {
    const scale = TILE * Math.pow(2, zoom);
    const lon = (x / scale) * 360 - 180;
    const n = Math.PI - 2 * Math.PI * (y / scale);
    const lat = (180 / Math.PI) * Math.atan(0.5 * (Math.exp(n) - Math.exp(-n)));
    return { lon, lat };
  }

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = String(text);
    return node;
  }

  function pinIcon() {
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("class", "map-pin-icon");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", "M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11z");
    const dot = document.createElementNS(SVG_NS, "circle");
    dot.setAttribute("cx", "12");
    dot.setAttribute("cy", "10");
    dot.setAttribute("r", "2.6");
    dot.setAttribute("class", "map-pin-dot");
    svg.append(path, dot);
    return svg;
  }

  /* ── 切片地图 ───────────────────────────────────────────────────── */
  /*
   * points: [{lon, lat, label}] —— 已经按 provider 选好坐标系的点。
   * 返回 { element, focus(index), destroy() }。
   */
  function createMap(provider, points, options = {}) {
    const viewport = el("div", "map-viewport");
    viewport.tabIndex = 0;
    viewport.setAttribute("role", "application");
    viewport.setAttribute("aria-label", "地图");
    const tileLayer = el("div", "map-tiles");
    const markerLayer = el("div", "map-markers");
    viewport.append(tileLayer, markerLayer);

    const tiles = new Map();
    let zoom = options.zoom || 15;
    let center = options.center || points[0] || { lon: 116.397, lat: 39.909 };
    let selected = 0;
    let frame = 0;

    function size() {
      const rect = viewport.getBoundingClientRect();
      // 还没上屏时 getBoundingClientRect 全是 0,拿一个体面的默认值,
      // 真正的尺寸等 ResizeObserver 回调再来一次。
      return { w: Math.round(rect.width) || 480, h: Math.round(rect.height) || 260 };
    }

    function draw() {
      frame = 0;
      const { w, h } = size();
      const world = project(center.lon, center.lat, zoom);
      const originX = world.x - w / 2;
      const originY = world.y - h / 2;
      const span = Math.pow(2, zoom);
      const minX = Math.floor(originX / TILE);
      const maxX = Math.floor((originX + w) / TILE);
      const minY = Math.floor(originY / TILE);
      const maxY = Math.floor((originY + h) / TILE);
      const live = new Set();
      for (let x = minX; x <= maxX; x += 1) {
        for (let y = minY; y <= maxY; y += 1) {
          // 纵向不环绕(极地之外没有瓦片),横向环绕:拖过国际日期变更线时
          // 不该出现一片空白。
          if (y < 0 || y >= span) continue;
          const wrapped = ((x % span) + span) % span;
          const key = `${zoom}/${wrapped}/${y}`;
          live.add(key);
          let tile = tiles.get(key);
          if (!tile) {
            tile = el("img", "map-tile");
            tile.src = `/api/map/tile?p=${encodeURIComponent(provider)}&z=${zoom}&x=${wrapped}&y=${y}`;
            tile.alt = "";
            tile.loading = "eager";
            tile.decoding = "async";
            tile.draggable = false;
            // 取不到的那块留空,不显示裂图图标——破洞比裂图好看,也更诚实。
            tile.addEventListener("error", () => { tile.classList.add("is-blank"); }, { once: true });
            tiles.set(key, tile);
            tileLayer.append(tile);
          }
          tile.style.left = `${Math.round(x * TILE - originX)}px`;
          tile.style.top = `${Math.round(y * TILE - originY)}px`;
        }
      }
      for (const [key, tile] of tiles) {
        if (live.has(key)) continue;
        tile.remove();
        tiles.delete(key);
      }
      for (const marker of markerLayer.children) {
        const point = points[Number(marker.dataset.index)];
        const at = project(point.lon, point.lat, zoom);
        marker.style.left = `${Math.round(at.x - originX)}px`;
        marker.style.top = `${Math.round(at.y - originY)}px`;
        marker.classList.toggle("is-selected", Number(marker.dataset.index) === selected);
      }
    }

    function schedule() {
      if (frame) return;
      frame = requestAnimationFrame(draw);
    }

    points.forEach((point, index) => {
      const marker = el("button", "map-marker");
      marker.type = "button";
      marker.dataset.index = String(index);
      marker.title = point.label || "";
      marker.setAttribute("aria-label", point.label || `地点 ${index + 1}`);
      marker.append(pinIcon());
      if (points.length > 1) marker.append(el("span", "map-marker-index", index + 1));
      marker.addEventListener("click", (event) => {
        event.stopPropagation();
        focus(index);
        options.onSelect?.(index);
      });
      markerLayer.append(marker);
    });

    function focus(index, keepZoom = true) {
      const point = points[index];
      if (!point) return;
      selected = index;
      center = { lon: point.lon, lat: point.lat };
      if (!keepZoom) zoom = 16;
      schedule();
    }

    /** 把所有点框进视野:先按经纬跨度估一个层级,再逐级收紧到真的装得下。 */
    function fitAll() {
      if (points.length === 1) {
        center = { lon: points[0].lon, lat: points[0].lat };
        zoom = 16;
        schedule();
        return;
      }
      const lons = points.map((point) => point.lon);
      const lats = points.map((point) => point.lat);
      center = {
        lon: (Math.min(...lons) + Math.max(...lons)) / 2,
        lat: (Math.min(...lats) + Math.max(...lats)) / 2
      };
      const { w, h } = size();
      let candidate = MAX_ZOOM;
      while (candidate > MIN_ZOOM) {
        const a = project(Math.min(...lons), Math.max(...lats), candidate);
        const b = project(Math.max(...lons), Math.min(...lats), candidate);
        // 四周留 48px,免得标记贴在边上被裁掉半个。
        if (b.x - a.x < w - 48 && b.y - a.y < h - 48) break;
        candidate -= 1;
      }
      zoom = candidate;
      schedule();
    }

    function zoomTo(next, anchor) {
      const clamped = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, next));
      if (clamped === zoom) return;
      if (anchor) {
        // 以光标为锚:缩放前后光标下的那个经纬度保持不动。
        const { w, h } = size();
        const before = unproject(
          project(center.lon, center.lat, zoom).x - w / 2 + anchor.x,
          project(center.lon, center.lat, zoom).y - h / 2 + anchor.y,
          zoom
        );
        zoom = clamped;
        const after = project(before.lon, before.lat, zoom);
        const world = { x: after.x - anchor.x + w / 2, y: after.y - anchor.y + h / 2 };
        center = unproject(world.x, world.y, zoom);
      } else {
        zoom = clamped;
      }
      schedule();
    }

    /* 拖拽平移。用 pointer 事件一套覆盖鼠标与触屏;setPointerCapture 让指针
       滑出地图也不断线(松手在别处是很常见的操作)。 */
    let dragging = null;
    viewport.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 && event.pointerType === "mouse") return;
      dragging = { x: event.clientX, y: event.clientY };
      viewport.classList.add("is-dragging");
      viewport.setPointerCapture(event.pointerId);
    });
    viewport.addEventListener("pointermove", (event) => {
      if (!dragging) return;
      const dx = event.clientX - dragging.x;
      const dy = event.clientY - dragging.y;
      dragging = { x: event.clientX, y: event.clientY };
      const world = project(center.lon, center.lat, zoom);
      center = unproject(world.x - dx, world.y - dy, zoom);
      schedule();
    });
    const endDrag = (event) => {
      if (!dragging) return;
      dragging = null;
      viewport.classList.remove("is-dragging");
      if (viewport.hasPointerCapture?.(event.pointerId)) viewport.releasePointerCapture(event.pointerId);
    };
    viewport.addEventListener("pointerup", endDrag);
    viewport.addEventListener("pointercancel", endDrag);

    /* 滚轮缩放。passive:false 才 preventDefault 得掉——不拦的话滚轮会连带把
       整个对话滚走,地图缩了一级、页面也跑了。 */
    viewport.addEventListener("wheel", (event) => {
      event.preventDefault();
      const rect = viewport.getBoundingClientRect();
      zoomTo(zoom + (event.deltaY < 0 ? 1 : -1), { x: event.clientX - rect.left, y: event.clientY - rect.top });
    }, { passive: false });

    viewport.addEventListener("dblclick", (event) => {
      const rect = viewport.getBoundingClientRect();
      zoomTo(zoom + 1, { x: event.clientX - rect.left, y: event.clientY - rect.top });
    });

    /* 键盘:方向键平移、+/- 缩放。地图是可聚焦元素,只用鼠标就等于把它关在
       键盘用户门外。 */
    viewport.addEventListener("keydown", (event) => {
      const step = 60;
      const world = project(center.lon, center.lat, zoom);
      if (event.key === "ArrowLeft") center = unproject(world.x - step, world.y, zoom);
      else if (event.key === "ArrowRight") center = unproject(world.x + step, world.y, zoom);
      else if (event.key === "ArrowUp") center = unproject(world.x, world.y - step, zoom);
      else if (event.key === "ArrowDown") center = unproject(world.x, world.y + step, zoom);
      else if (event.key === "+" || event.key === "=") zoomTo(zoom + 1);
      else if (event.key === "-" || event.key === "_") zoomTo(zoom - 1);
      else return;
      event.preventDefault();
      schedule();
    });

    const controls = el("div", "map-controls");
    const zoomIn = el("button", "map-control", "＋");
    zoomIn.type = "button";
    zoomIn.title = "放大";
    zoomIn.addEventListener("click", () => zoomTo(zoom + 1));
    const zoomOut = el("button", "map-control", "－");
    zoomOut.type = "button";
    zoomOut.title = "缩小";
    zoomOut.addEventListener("click", () => zoomTo(zoom - 1));
    const fit = el("button", "map-control", "⤢");
    fit.type = "button";
    fit.title = "全部装进视野";
    fit.addEventListener("click", () => fitAll());
    controls.append(zoomIn, zoomOut, fit);
    viewport.append(controls);

    const credit = el("span", "map-credit", PROVIDER_LABEL[provider] || provider);
    viewport.append(credit);

    let observer = null;
    if (window.ResizeObserver) {
      observer = new ResizeObserver(() => schedule());
      observer.observe(viewport);
    }

    fitAll();
    return {
      element: viewport,
      focus,
      fitAll,
      destroy() { observer?.disconnect(); if (frame) cancelAnimationFrame(frame); }
    };
  }

  /* ── 卡片 ───────────────────────────────────────────────────────── */
  function parse(output) {
    let payload = null;
    try {
      payload = JSON.parse(String(output || ""));
    } catch (_) {
      return null;
    }
    if (!payload || payload.ok === false) return null;
    if (!Array.isArray(payload.places) || !payload.places.length) return null;
    return payload;
  }

  /** 瓦片是哪套坐标系,标记就得用哪套——取错了标记稳定地偏到隔壁街。 */
  function pointOf(place, provider) {
    const lon = provider === "amap" ? place.lon_gcj02 : place.lon_wgs84;
    const lat = provider === "amap" ? place.lat_gcj02 : place.lat_wgs84;
    return {
      lon: Number(lon ?? place.lon),
      lat: Number(lat ?? place.lat),
      label: place.name || place.address || ""
    };
  }

  function coordText(place) {
    const lon = Number(place.lon);
    const lat = Number(place.lat);
    if (!Number.isFinite(lon) || !Number.isFinite(lat)) return "";
    return `${lon.toFixed(6)}, ${lat.toFixed(6)}`;
  }

  function distanceText(place) {
    const meters = Number(place.distance_m);
    if (!Number.isFinite(meters) || meters <= 0) return "";
    return meters >= 1000 ? `${(meters / 1000).toFixed(1)} km` : `${Math.round(meters)} m`;
  }

  function copyButton(text, title) {
    const button = el("button", "map-mini-button", "复制");
    button.type = "button";
    button.title = title;
    button.addEventListener("click", async (event) => {
      event.stopPropagation();
      try {
        await navigator.clipboard.writeText(text);
        button.textContent = "已复制";
      } catch (_) {
        button.textContent = "复制不了";
      }
      setTimeout(() => { button.textContent = "复制"; }, 1600);
    });
    return button;
  }

  function placeRow(place, index, provider, onPick) {
    const row = el("li", "map-place");
    row.dataset.index = String(index);
    const head = el("div", "map-place-head");
    head.append(el("span", "map-place-index", index + 1), el("strong", "map-place-name", place.name || "未命名地点"));
    const distance = distanceText(place);
    if (distance) head.append(el("span", "map-place-distance", distance));
    row.append(head);
    if (place.address) row.append(el("p", "map-place-address", place.address));
    const meta = el("div", "map-place-meta");
    if (place.category) meta.append(el("span", "map-chip", place.category));
    const coords = coordText(place);
    if (coords) {
      meta.append(el("code", "map-coords", coords));
      meta.append(copyButton(coords, "复制坐标"));
    }
    // 外链各用各的坐标系,由后端算好——前端不换算,换算错了的表现是点开在隔壁街。
    const external = provider === "amap" ? place.amap_url : place.osm_url;
    if (external) {
      const link = el("a", "map-mini-button", "在地图网站打开");
      link.href = external;
      link.target = "_blank";
      link.rel = "noreferrer noopener";
      link.addEventListener("click", (event) => event.stopPropagation());
      meta.append(link);
    }
    row.append(meta);
    row.addEventListener("click", () => onPick(index));
    return row;
  }

  /** 详情视图:同一份数据、更大的地图、列表全展开。开在一个模态层里。 */
  function openDetail(payload) {
    const provider = payload.provider === "amap" ? "amap" : "osm";
    const overlay = el("div", "map-detail-overlay");
    const panel = el("section", "map-detail");
    const head = el("header", "map-detail-head");
    head.append(el("strong", null, payload.query || "地图"), el("span", "map-detail-count", `${payload.places.length} 个结果`));
    const close = el("button", "map-mini-button", "关闭");
    close.type = "button";
    head.append(close);
    const body = el("div", "map-detail-body");
    const list = el("ul", "map-places");
    const map = createMap(provider, payload.places.map((place) => pointOf(place, provider)), {
      onSelect: (index) => highlight(index)
    });
    function highlight(index) {
      for (const row of list.children) row.classList.toggle("is-selected", Number(row.dataset.index) === index);
      map.focus(index);
    }
    payload.places.forEach((place, index) => list.append(placeRow(place, index, provider, highlight)));
    body.append(map.element, list);
    panel.append(head, body);
    overlay.append(panel);

    const dismiss = () => {
      map.destroy();
      overlay.remove();
      document.removeEventListener("keydown", onKey, true);
    };
    // 捕获阶段吃掉 Escape:app.js 也在 document 上听它,不拦的话按一下会把
    // 详情和它后面的东西一起关掉。
    function onKey(event) {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      event.preventDefault();
      dismiss();
    }
    close.addEventListener("click", dismiss);
    overlay.addEventListener("click", (event) => { if (event.target === overlay) dismiss(); });
    document.addEventListener("keydown", onKey, true);
    document.body.append(overlay);
    close.focus();
  }

  /* 卡片本体。解析不出来就返回 null,调用方原样保留工具签——卡片是增益。 */
  function renderCard(output) {
    const payload = parse(output);
    if (!payload) return null;
    const provider = payload.provider === "amap" ? "amap" : "osm";
    const card = el("section", "map-card");
    const head = el("header", "map-card-head");
    head.append(el("strong", "map-card-title", payload.query || "地图"));
    head.append(el("span", "map-card-sub", `${payload.places.length} 个结果 · ${provider === "amap" ? "高德" : "OpenStreetMap"}`));
    const detail = el("button", "map-mini-button", "详细地图");
    detail.type = "button";
    detail.title = "放大看,列表全展开";
    detail.addEventListener("click", () => openDetail(payload));
    head.append(detail);

    const list = el("ul", "map-places");
    const map = createMap(provider, payload.places.map((place) => pointOf(place, provider)), {
      onSelect: (index) => highlight(index)
    });
    function highlight(index) {
      for (const row of list.children) row.classList.toggle("is-selected", Number(row.dataset.index) === index);
      map.focus(index);
    }
    // 卡片里只列前三条:再多就把对话挤没了,想看全的点「详细地图」。
    payload.places.slice(0, 3).forEach((place, index) => list.append(placeRow(place, index, provider, highlight)));
    if (payload.places.length > 3) {
      const more = el("li", "map-place-more", `还有 ${payload.places.length - 3} 个结果,在详细地图里看`);
      list.append(more);
    }
    card.append(head, map.element, list);
    return card;
  }

  return { isMapTool, renderCard };
})();
