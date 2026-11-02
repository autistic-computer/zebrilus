(() => {
  const sections = [...document.querySelectorAll("section[data-section]")];

  function ceilSections() {
    sections.forEach((section) => section.style.removeProperty("padding-bottom"));
    const heights = sections.map((section) => section.getBoundingClientRect().height);
    sections.forEach((section, index) => {
      const extra = Math.ceil(heights[index] - 0.01) - heights[index];
      if (extra > 0) section.style.paddingBottom = `calc(24px + ${extra}px)`;
    });
  }

  const observer = new ResizeObserver(() => requestAnimationFrame(ceilSections));
  document.querySelectorAll("section[data-section] > *").forEach((child) => observer.observe(child));
  ceilSections();
})();
