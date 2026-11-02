const TEXT = ".icon-name, .story-label";

function snap() {
  const spans = [...document.querySelectorAll(TEXT)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
}

snap();
document.fonts.ready.then(snap);
