(() => {
  const SECTIONS = "section[data-section]";

  function fit() {
    const sections = [...document.querySelectorAll(SECTIONS)];
    sections.forEach((section) => section.style.removeProperty("min-height"));
    const heights = sections.map((section) => section.getBoundingClientRect().height);
    sections.forEach((section, index) => {
      const height = heights[index];
      const whole = Math.ceil(height - 0.01);
      if (whole > height) section.style.minHeight = `${whole}px`;
    });
  }

  let queued = false;
  function queue() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      fit();
    });
  }

  const resized = new ResizeObserver(queue);

  function watch() {
    document.querySelectorAll(`${SECTIONS} > *`).forEach((child) => resized.observe(child));
    fit();
  }

  if (document.readyState === "loading") addEventListener("DOMContentLoaded", watch, { once: true });
  else watch();
  document.fonts?.ready.then(queue);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && queue(), true);

  window.pennaStory = { fit };
})();
