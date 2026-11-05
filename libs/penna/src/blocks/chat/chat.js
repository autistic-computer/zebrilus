(() => {
  const FIELD_RADIUS_CAP = 20;

  function roundFields(root = document) {
    root.querySelectorAll(".penna-chat-field[data-penna]").forEach((field) => {
      const style = getComputedStyle(field);
      const radius = Math.min(parseFloat(style.getPropertyValue("--penna-r")) || 0, FIELD_RADIUS_CAP);
      const cap = Math.min(field.offsetWidth, field.offsetHeight) * 0.45;
      if (!cap) return;
      field.style.borderRadius = `${cap * Math.tanh(radius / cap)}px`;
    });
  }

  addEventListener("load", () => roundFields());
  document.fonts?.ready.then(() => roundFields());
  document.addEventListener(
    "load",
    (event) => event.target.id === "penna-tokens" && requestAnimationFrame(() => roundFields()),
    true,
  );
  globalThis.pennaChat = { roundFields };
})();
