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

  tint();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && tint(), true);
}
