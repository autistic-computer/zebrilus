const DISPLAY_TEXT = ".penna-text";
const DISPLAY_ICONS = ".penna-list-item [data-icon], .penna-spinner > [data-icon]";

if (new URLSearchParams(location.search).has("bare")) document.documentElement.dataset.motion = "off";

function displaySnap() {
  const spans = [...document.querySelectorAll(DISPLAY_TEXT)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function displayPin() {
  const icons = [...document.querySelectorAll(DISPLAY_ICONS)];
  icons.forEach((icon) => icon.style.removeProperty("translate"));
  const scale = devicePixelRatio || 1;
  const tops = icons.map((icon) => icon.getBoundingClientRect().top + scrollY);
  icons.forEach((icon, index) => {
    const device = tops[index] * scale;
    const shift = (Math.floor(device + 0.01) - device) / scale;
    if (Math.abs(shift) > 0.001) icon.style.translate = `0 ${shift}px`;
  });
}

function displayFit() {
  document.querySelectorAll("section[data-section]").forEach((section) => {
    section.style.removeProperty("min-height");
    section.style.minHeight = `${Math.ceil(section.getBoundingClientRect().height - 0.01)}px`;
  });
}

function displaySettle() {
  displaySnap();
  displayPin();
  displayFit();
}

displaySettle();
document.fonts.ready.then(displaySettle);
