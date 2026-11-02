{
  const TEXT = ".penna-title-bar-text";

  function snapTitleBar(root = document) {
    const spans = [...root.querySelectorAll(TEXT)];
    spans.forEach((span) => span.style.removeProperty("width"));
    const widths = spans.map((span) => span.getBoundingClientRect().width);
    spans.forEach((span, index) => {
      span.style.width = `${Math.ceil(widths[index])}px`;
    });
  }

  function tooltips(root = document) {
    let tip = null;
    let timer = 0;
    const hide = () => {
      clearTimeout(timer);
      tip?.remove();
      tip = null;
    };
    root.querySelectorAll(".penna-title-bar [data-tooltip]").forEach((target) => {
      target.addEventListener("pointerenter", () => {
        hide();
        timer = setTimeout(() => {
          tip = document.createElement("div");
          tip.className = "penna-title-bar-tip";
          tip.setAttribute("data-penna", "tooltip");
          tip.setAttribute("role", "tooltip");
          const label = document.createElement("span");
          label.textContent = target.dataset.tooltip;
          tip.append(label);
          document.body.append(tip);
          const box = target.getBoundingClientRect();
          const size = tip.getBoundingClientRect();
          const left = Math.min(Math.max(8, box.left + box.width / 2 - size.width / 2), document.documentElement.clientWidth - size.width - 8);
          tip.style.left = `${left + scrollX}px`;
          tip.style.top = `${box.bottom + 8 + scrollY}px`;
        }, 500);
      });
      target.addEventListener("pointerleave", hide);
      target.addEventListener("pointerdown", hide);
    });
  }

  snapTitleBar();
  tooltips();
  document.fonts.ready.then(() => snapTitleBar());
}
