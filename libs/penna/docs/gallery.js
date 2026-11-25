(() => {
  const root = document.querySelector("[data-docs-gallery]");
  if (!root) return;

  const input = root.querySelector("[data-gal-search]");
  const count = root.querySelector("[data-gal-count]");
  const catButtons = [...root.querySelectorAll("[data-gal-cat]")];
  const cards = [...root.querySelectorAll("[data-gal-item]")];
  let activeCat = "all";

  function apply() {
    const q = (input?.value || "").trim().toLowerCase();
    let shown = 0;
    for (const card of cards) {
      const catOk = activeCat === "all" || card.dataset.galCat === activeCat;
      const terms = (card.dataset.galTerms || "").toLowerCase();
      const text = card.textContent.toLowerCase();
      const qOk = !q || terms.includes(q) || text.includes(q);
      const visible = catOk && qOk;
      card.hidden = !visible;
      if (visible) shown += 1;
    }
    if (count) count.textContent = `${shown} of ${cards.length}`;
  }

  catButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      activeCat = btn.dataset.galCat || "all";
      catButtons.forEach((other) => {
        if (other === btn) other.dataset.state = "selected";
        else delete other.dataset.state;
      });
      apply();
    });
  });

  input?.addEventListener("input", apply);
  apply();
  requestAnimationFrame(() => {
    window.pennaKit?.roundAll?.(root);
    window.pennaSegmented?.wire?.(root);
    window.pennaKbd?.fill?.(root);
  });
})();
