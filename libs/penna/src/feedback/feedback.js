{
  const TONES = ["primary", "success", "warning", "danger"];

  const inks = () => {
    const found = {};
    TONES.forEach((tone) => {
      const probe = document.createElement("i");
      probe.dataset.penna = "button.quiet";
      probe.dataset.tone = tone;
      probe.hidden = true;
      document.body.append(probe);
      found[tone] = getComputedStyle(probe).color;
      probe.remove();
    });
    return found;
  };

  const tint = (root = document) => {
    const found = inks();
    root.querySelectorAll(".penna-callout[data-tone]").forEach((callout) => {
      callout.style.setProperty("--penna-callout-ink", found[callout.dataset.tone]);
    });
  };

  const snap = (root = document) => {
    const runs = [...root.querySelectorAll(".penna-empty-state > span, .penna-empty-state-title > span")];
    runs.forEach((run) => run.style.removeProperty("width"));
    const widths = runs.map((run) => run.getBoundingClientRect().width);
    runs.forEach((run, index) => {
      run.style.display = "inline-block";
      run.style.width = `${Math.ceil(widths[index])}px`;
    });
  };

  const FILTER = /(brightness|contrast)\(([^)]*)\)/g;

  const glass = (root = document) => {
    root.querySelectorAll(".penna-callout").forEach((callout) => {
      callout.style.removeProperty("--penna-backdrop");
      const backdrop = getComputedStyle(callout).getPropertyValue("--penna-backdrop").trim();
      const found = Object.fromEntries([...backdrop.matchAll(FILTER)].map(([, name, value]) => [name, parseFloat(value)]));
      if (!("brightness" in found) || !("contrast" in found)) return;
      const lift = found.brightness - 1;
      const spread = found.contrast - 2 * lift;
      const scaled = backdrop
        .replace(/brightness\([^)]*\)/, `brightness(${found.contrast / spread})`)
        .replace(/contrast\([^)]*\)/, `contrast(${spread})`);
      callout.style.setProperty("--penna-backdrop", scaled);
    });
  };

  const settle = () => {
    glass();
    tint();
    snap();
  };

  settle();
  document.fonts.ready.then(settle);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settle(), true);
}
