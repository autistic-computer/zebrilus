const DISPLAY_ICONS = ".penna-list-item [data-icon], .penna-spinner > [data-icon]";

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

displayPin();
document.fonts.ready.then(displayPin);

addEventListener(
  "pointermove",
  () => document.querySelectorAll('.penna-list-item[data-state="hover"]').forEach((row) => row.removeAttribute("data-state")),
  { once: true },
);
