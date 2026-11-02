(() => {
  const TEXT = ".penna-dialog-text";
  const STEP = 16;
  const FOCUSABLE = "button:not(:disabled), a[href], input:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

  function ceil(root = document) {
    const spans = [...root.querySelectorAll(TEXT)];
    spans.forEach((span) => span.style.removeProperty("width"));
    const widths = spans.map((span) => span.getBoundingClientRect().width);
    spans.forEach((span, index) => {
      span.style.width = `${Math.ceil(widths[index])}px`;
    });
  }

  function number(filter, name, fallback) {
    const match = filter.match(new RegExp(`${name}\\(([-\\d.]+)(%?)\\)`));
    if (!match) return fallback;
    return match[2] ? parseFloat(match[1]) / 100 : parseFloat(match[1]);
  }

  function glaze(root = document) {
    root.querySelectorAll(".penna-dialog").forEach((dialog) => {
      dialog.style.removeProperty("--penna-backdrop");
      const filter = getComputedStyle(dialog).getPropertyValue("--penna-backdrop").trim();
      if (!filter || filter === "none" || !/brightness|contrast/.test(filter)) return;
      const scale = 1 - 2 * (number(filter, "brightness", 1) - 1);
      if (scale <= 0) return;
      const rest = filter.replace(/\s*(brightness|contrast)\([^)]*\)/g, "").trim();
      dialog.style.setProperty("--penna-backdrop", `${rest} brightness(${(1 / scale).toFixed(4)}) contrast(${(number(filter, "contrast", 1) * scale).toFixed(4)})`.trim());
    });
  }

  function layers(stage) {
    return [...stage.querySelectorAll(":scope > .penna-dialog-layer")].filter((layer) => !layer.hasAttribute("data-closed"));
  }

  function restack(stage) {
    const open = layers(stage);
    stage.querySelectorAll(":scope > .penna-dialog-layer[data-closed]").forEach((layer) => layer.removeAttribute("data-top"));
    open.forEach((layer, index) => {
      layer.style.setProperty("--dialog-offset", `${index * STEP}px`);
      layer.toggleAttribute("data-top", index === open.length - 1);
      layer.inert = index !== open.length - 1;
    });
    stage.toggleAttribute("data-open", open.length > 0);
    return open;
  }

  function top(stage) {
    const open = layers(stage);
    return open[open.length - 1] ?? null;
  }

  function close(layer) {
    const stage = layer.closest(".penna-dialog-stage");
    layer.setAttribute("data-closed", "");
    const open = restack(stage);
    open[open.length - 1]?.querySelector(".penna-dialog").focus({ preventScroll: true });
    layer.dispatchEvent(new CustomEvent("penna-dialog-close", { bubbles: true }));
  }

  function open(layer) {
    const stage = layer.closest(".penna-dialog-stage");
    layer.removeAttribute("data-closed");
    stage.append(layer);
    restack(stage);
    window.pennaKit?.roundAll?.();
    ceil(layer);
    layer.querySelector(".penna-dialog").focus({ preventScroll: true });
  }

  function trap(dialog, event) {
    const items = [...dialog.querySelectorAll(FOCUSABLE)].filter((item) => item.offsetParent !== null);
    if (!items.length) return;
    const at = items.indexOf(document.activeElement);
    const next = event.shiftKey ? (at <= 0 ? items.length - 1 : at - 1) : at < 0 || at === items.length - 1 ? 0 : at + 1;
    items[next].focus();
    event.preventDefault();
  }

  function setup(layer) {
    const dialog = layer.querySelector(".penna-dialog");
    dialog.tabIndex = -1;
    layer.addEventListener("mousedown", (event) => {
      if (event.button !== 0 || dialog.contains(event.target) || !layer.hasAttribute("data-top")) return;
      event.preventDefault();
      if (!layer.hasAttribute("data-static")) close(layer);
    });
    layer.querySelectorAll("[data-dialog-close], [data-dialog-ok], [data-dialog-cancel]").forEach((button) => {
      button.addEventListener("click", () => close(layer));
    });
    dialog.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close(layer);
      } else if (event.key === "Enter" && !event.target.closest("button, a, textarea")) {
        const ok = layer.querySelector("[data-dialog-ok]");
        event.preventDefault();
        if (ok) ok.click();
        else close(layer);
      } else if (event.key === "Tab") {
        trap(dialog, event);
      }
    });
  }

  function init(root = document) {
    root.querySelectorAll(".penna-dialog-layer").forEach(setup);
    root.querySelectorAll(".penna-dialog-stage").forEach(restack);
    ceil(root);
    glaze(root);
  }

  init();
  document.fonts.ready.then(() => ceil());
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && glaze(), true);
  window.pennaDialog = { open, close, top, ceil, restack };
})();
