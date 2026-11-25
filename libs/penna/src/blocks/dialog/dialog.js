(() => {
  const STEP = 16;
  const FOCUSABLE = "button:not(:disabled), a[href], input:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

  const stages = new WeakMap();

  function layers(stage) {
    return [...stage.querySelectorAll(":scope > .penna-dialog-layer")];
  }

  function restack(stage) {
    const open = layers(stage);
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
    if (!layer || layer.hasAttribute("data-still")) return;
    const stage = stages.get(layer);
    if (!stage) return;
    layer.remove();
    const open = restack(stage);
    open[open.length - 1]?.querySelector(".penna-dialog")?.focus({ preventScroll: true });
    stage.dispatchEvent(new CustomEvent("penna-dialog-close", { bubbles: true, detail: layer }));
  }

  function open(layer) {
    if (!layer) return;
    const stage = stages.get(layer);
    if (!stage) return;
    stage.append(layer);
    restack(stage);
    window.pennaKit?.roundAll?.();
    window.pennaKit?.ceilText?.(layer);
    layer.querySelector(".penna-dialog")?.focus({ preventScroll: true });
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
    if (!dialog) return;
    stages.set(layer, layer.closest(".penna-dialog-stage"));
    if (layer.hasAttribute("data-still")) return;
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

  const templates = new Map();

  function init(root = document) {
    root.querySelectorAll(".penna-dialog-layer").forEach((layer) => {
      if (layer.id) templates.set(layer.id, layer);
      setup(layer);
    });
    root.querySelectorAll(".penna-dialog-stage").forEach(restack);
  }

  document.addEventListener("click", (event) => {
    const trigger = event.target.closest("[data-dialog-open]");
    if (!trigger) return;
    const layer = templates.get(trigger.dataset.dialogOpen);
    if (layer && !layer.isConnected) open(layer);
  });

  init();
  window.pennaDialog = { open, close, top, restack };
})();
