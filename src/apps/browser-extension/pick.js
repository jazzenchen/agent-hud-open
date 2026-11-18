// Pick mode, put into the page by the toolbar button: the element under the
// pointer is outlined with its tag and size; a click picks it, a drag takes
// an area, Esc lets go. A pick goes to Agent HUD with a picture cropped from
// the visible tab, and for an element what describe.js says of it (put into
// the page first). Pressing the button again lets go too.
(() => {
  if (window.__agentHudPick) {
    window.__agentHudPick.stop();
    return;
  }
  const say = (key, ...values) => chrome.i18n.getMessage(key, values) || key;
  const INK = "#0A84FF";
  const host = document.createElement("div");
  host.style.cssText =
    "position:fixed;inset:0;z-index:2147483647;cursor:crosshair;";
  const shadow = host.attachShadow({ mode: "closed" });
  shadow.innerHTML = `
    <style>
      .box { position: fixed; border: 2px solid ${INK}; background: rgba(10,132,255,0.12);
        border-radius: 3px; pointer-events: none; display: none; }
      .label { position: fixed; padding: 2px 6px; border-radius: 4px; background: ${INK};
        color: #fff; font: 600 11px -apple-system, sans-serif; pointer-events: none;
        display: none; white-space: nowrap; }
      .hint, .toast { position: fixed; left: 50%; transform: translateX(-50%);
        padding: 7px 14px; border-radius: 9px; background: rgba(28,28,30,0.92); color: #fff;
        font: 500 13px -apple-system, sans-serif; pointer-events: none; }
      .hint { top: 14px; }
      .toast { bottom: 24px; }
    </style>
    <div class="box"></div><div class="label"></div><div class="hint"></div>`;
  const box = shadow.querySelector(".box");
  const label = shadow.querySelector(".label");
  shadow.querySelector(".hint").textContent = say("hint");
  let hovered = null;
  let start = null;
  let area = null;

  const under = (x, y) =>
    document.elementsFromPoint(x, y).find((element) => element !== host) ??
    null;
  const show = (rect, text) => {
    Object.assign(box.style, {
      display: "block",
      left: `${rect.left}px`,
      top: `${rect.top}px`,
      width: `${rect.width}px`,
      height: `${rect.height}px`,
    });
    label.style.display = text ? "block" : "none";
    label.textContent = text ?? "";
    label.style.left = `${Math.max(4, rect.left)}px`;
    label.style.top = `${rect.top > 24 ? rect.top - 22 : rect.bottom + 4}px`;
  };
  const name = (element) => {
    const id = element.id ? `#${element.id}` : "";
    const classes = [...element.classList]
      .slice(0, 2)
      .map((c) => `.${c}`)
      .join("");
    return `${element.tagName.toLowerCase()}${id}${classes}`;
  };
  const move = (event) => {
    if (start) {
      const dx = event.clientX - start.x;
      const dy = event.clientY - start.y;
      if (area || Math.hypot(dx, dy) > 5) {
        area = {
          left: Math.min(start.x, event.clientX),
          top: Math.min(start.y, event.clientY),
          width: Math.abs(dx),
          height: Math.abs(dy),
        };
        area.right = area.left + area.width;
        area.bottom = area.top + area.height;
        show(area, `${Math.round(area.width)} × ${Math.round(area.height)}`);
      }
      return;
    }
    hovered = under(event.clientX, event.clientY);
    if (!hovered) return;
    const rect = hovered.getBoundingClientRect();
    show(
      rect,
      `${name(hovered)}  ${Math.round(rect.width)} × ${Math.round(rect.height)}`,
    );
  };
  const down = (event) => {
    event.preventDefault();
    event.stopPropagation();
    start = { x: event.clientX, y: event.clientY };
  };
  const up = (event) => {
    event.preventDefault();
    event.stopPropagation();
    const picked = area ? { area } : hovered ? { element: hovered } : null;
    start = null;
    area = null;
    if (picked) finish(picked);
  };
  const swallow = (event) => {
    event.preventDefault();
    event.stopPropagation();
  };
  const key = (event) => {
    if (event.key === "Escape") {
      swallow(event);
      stop();
    }
  };
  host.addEventListener("pointermove", move, true);
  host.addEventListener("pointerdown", down, true);
  host.addEventListener("pointerup", up, true);
  host.addEventListener("click", swallow, true);
  window.addEventListener("keydown", key, true);
  document.documentElement.append(host);

  function stop() {
    window.removeEventListener("keydown", key, true);
    host.remove();
    delete window.__agentHudPick;
  }
  window.__agentHudPick = { stop };

  async function finish({ element, area }) {
    stop();
    const rect = area ?? element.getBoundingClientRect();
    // Let the outline leave the screen before the tab is pictured.
    await new Promise((done) =>
      requestAnimationFrame(() => requestAnimationFrame(done)),
    );
    const pick = {
      kind: area ? "area" : "element",
      url: location.href,
      title: document.title,
      browser: browserName(),
      png: await crop(rect, area ? 0 : 8),
      ...(element ? globalThis.__agentHudDescribe(element) : {}),
    };
    const answer = await chrome.runtime.sendMessage({ type: "send", pick });
    toast(
      answer?.ok
        ? say("sent")
        : answer?.unreachable
          ? say("notRunning")
          : say("failed", answer?.error ?? ""),
    );
  }

  function browserName() {
    const brands = navigator.userAgentData?.brands ?? [];
    const brand = brands.find((b) => !/Not.?A.?Brand|Chromium/i.test(b.brand));
    return brand?.brand ?? "Chrome";
  }

  async function crop(rect, pad) {
    const shot = await chrome.runtime.sendMessage({ type: "capture" });
    if (!shot?.url) return null;
    const image = new Image();
    await new Promise((done, fail) => {
      image.onload = done;
      image.onerror = fail;
      image.src = shot.url;
    });
    const scale = image.naturalWidth / window.innerWidth;
    const left = Math.max(0, rect.left - pad);
    const top = Math.max(0, rect.top - pad);
    const right = Math.min(window.innerWidth, rect.right + pad);
    const bottom = Math.min(window.innerHeight, rect.bottom + pad);
    if (right - left < 2 || bottom - top < 2) return null;
    const canvas = document.createElement("canvas");
    canvas.width = Math.round((right - left) * scale);
    canvas.height = Math.round((bottom - top) * scale);
    canvas
      .getContext("2d")
      .drawImage(
        image,
        left * scale,
        top * scale,
        canvas.width,
        canvas.height,
        0,
        0,
        canvas.width,
        canvas.height,
      );
    return canvas.toDataURL("image/png").split(",")[1];
  }

  function toast(text) {
    const card = document.createElement("div");
    card.style.cssText =
      "position:fixed;inset:auto 0 0 0;z-index:2147483647;pointer-events:none;";
    const inner = card.attachShadow({ mode: "closed" });
    inner.innerHTML = `<div style="position:fixed;left:50%;bottom:24px;transform:translateX(-50%);
      max-width:70vw;padding:8px 14px;border-radius:9px;background:rgba(28,28,30,0.92);color:#fff;
      font:500 13px -apple-system,sans-serif;"></div>`;
    inner.firstElementChild.textContent = text;
    document.documentElement.append(card);
    setTimeout(() => card.remove(), 2600);
  }
})();
