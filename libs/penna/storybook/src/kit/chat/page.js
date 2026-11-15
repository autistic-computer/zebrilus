(() => {
  const LABELS = ["Thinking", "Reading agent/view.rs", "Searching for popover"];
  const EVERY = 2400;

  function resolve(label, text) {
    const shown = [...`${text}…`];
    label.style.removeProperty("width");
    label.style.setProperty("--penna-chat-stagger", `${Math.min(18, 400 / shown.length)}ms`);
    label.replaceChildren(
      ...shown.map((character, index) => {
        const glyph = document.createElement("i");
        glyph.style.setProperty("--i", index);
        glyph.textContent = character;
        return glyph;
      }),
    );
  }

  for (const label of document.querySelectorAll('[data-row="activity-live"] .penna-chat-resolve')) {
    let at = 0;
    resolve(label, LABELS[at]);
    setInterval(() => {
      at = (at + 1) % LABELS.length;
      resolve(label, LABELS[at]);
    }, EVERY);
  }
})();
