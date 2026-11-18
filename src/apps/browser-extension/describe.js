// What a pick says about its element: a selector, its words, its HTML and
// the styles it is drawn with. The HTML leaves out the marks a build adds to
// scope a component's styles (Astro's data-astro-cid-…, Vue's data-v-…,
// Svelte's svelte-… classes): no source file holds them, so the pick names
// them once instead of on every tag. Where a dev server marks elements with
// their place in the source, that place comes along by itself.
// Put into the page before pick.js; assigned each time, so a reloaded
// extension never runs an old copy left in the tab.
globalThis.__agentHudDescribe = (() => {
  const SCOPES = [
    { by: "Astro", attribute: /^data-astro-cid-[\w-]+$/ },
    { by: "Vue", attribute: /^data-v-[0-9a-f]{8}$/ },
    { by: "Svelte", className: /^svelte-[a-z0-9]{5,}$/ },
  ];
  // Astro's dev server, vite-plugin-vue-inspector and code-inspector-plugin.
  const PLACES = [
    "data-astro-source-file",
    "data-astro-source-loc",
    "data-v-inspector",
    "data-insp-path",
  ];

  // The nearest place in the source, from the element or one around it.
  function sourceOf(element) {
    for (let node = element; node; node = node.parentElement) {
      const file = node.getAttribute("data-astro-source-file");
      if (file) {
        const at = node.getAttribute("data-astro-source-loc");
        return at ? `${file}:${at}` : file;
      }
      const place =
        node.getAttribute("data-v-inspector") ??
        node.getAttribute("data-insp-path");
      // code-inspector-plugin ends with the tag: file:line:column:div.
      if (place) return place.replace(/:[A-Za-z][\w-]*$/, "");
    }
    return null;
  }

  // The element's HTML without scope marks or source places, and which
  // scope marks were left out, by the tool that adds them.
  function htmlOf(element) {
    const copy = element.cloneNode(true);
    const omitted = new Map();
    const omit = (by, mark) => {
      if (!omitted.has(by)) omitted.set(by, new Set());
      omitted.get(by).add(mark);
    };
    for (const node of [copy, ...copy.querySelectorAll("*")]) {
      for (const { name } of [...node.attributes]) {
        const scope = SCOPES.find((s) => s.attribute?.test(name));
        if (scope) omit(scope.by, name);
        if (scope || PLACES.includes(name)) node.removeAttribute(name);
      }
      const classes = [...node.classList];
      for (const token of classes) {
        const scope = SCOPES.find((s) => s.className?.test(token));
        if (!scope) continue;
        omit(scope.by, token);
        node.classList.remove(token);
      }
      if (classes.length && !node.classList.length) {
        node.removeAttribute("class");
      }
    }
    return {
      html: copy.outerHTML.slice(0, 20000),
      omitted: [...omitted].map(([by, marks]) => ({
        by,
        marks: [...marks].slice(0, 5),
      })),
    };
  }

  // A path of tags, ids and classes from the nearest element with an id,
  // at most five steps, with :nth-of-type where siblings share a tag.
  function selectorOf(element) {
    const steps = [];
    for (let node = element; node && node.nodeType === 1 && steps.length < 5;) {
      if (node.id) {
        steps.unshift(`#${CSS.escape(node.id)}`);
        break;
      }
      let step = node.tagName.toLowerCase();
      const classes = [...node.classList]
        .slice(0, 2)
        .map((c) => `.${CSS.escape(c)}`);
      step += classes.join("");
      const parent = node.parentElement;
      if (parent) {
        const same = [...parent.children].filter(
          (c) => c.tagName === node.tagName,
        );
        if (same.length > 1) step += `:nth-of-type(${same.indexOf(node) + 1})`;
      }
      steps.unshift(step);
      if (node === document.body) break;
      node = parent;
    }
    return steps.join(" > ");
  }

  const NAMES = [
    "display",
    "position",
    "box-sizing",
    "width",
    "height",
    "margin",
    "padding",
    "gap",
    "flex-direction",
    "align-items",
    "justify-content",
    "grid-template-columns",
    "font-family",
    "font-size",
    "font-weight",
    "line-height",
    "letter-spacing",
    "text-align",
    "color",
    "background-color",
    "background-image",
    "border",
    "border-radius",
    "box-shadow",
    "opacity",
    "overflow",
    "z-index",
  ];
  const QUIET = new Set([
    "none",
    "normal",
    "auto",
    "0px",
    "rgba(0, 0, 0, 0)",
    "static",
    "visible",
  ]);
  // A value that draws nothing: a default, or a border 0px wide.
  const quiet = (value) => QUIET.has(value) || /^0px none\b/.test(value);

  // The computed styles that make an element's look, leaving out defaults
  // (but always saying how it is laid out and placed).
  function stylesOf(element) {
    const style = getComputedStyle(element);
    return NAMES.map((name) => [name, style.getPropertyValue(name)])
      .filter(
        ([name, value]) =>
          value && !(quiet(value) && !["display", "position"].includes(name)),
      )
      .map(([name, value]) => `${name}: ${value};`)
      .join("\n");
  }

  return (element) => ({
    selector: selectorOf(element),
    source: sourceOf(element),
    ...htmlOf(element),
    text: (element.innerText ?? "").trim().slice(0, 4000),
    styles: stylesOf(element),
  });
})();
