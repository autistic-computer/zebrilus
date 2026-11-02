const SHEET_TEXT = ".penna-sheet-text";
const COMMIT = 0.48;
const FLING = 2;
const SHADOW_ROOM = 32;

function sheetSnap(root = document) {
  const spans = [...root.querySelectorAll(SHEET_TEXT)];
  spans.forEach((span) => span.style.removeProperty("width"));
  const widths = spans.map((span) => span.getBoundingClientRect().width);
  spans.forEach((span, index) => {
    span.style.display = "inline-block";
    span.style.width = `${Math.ceil(widths[index])}px`;
  });
}

function sheetExtent(sheet) {
  const panel = sheet.querySelector(".penna-sheet-panel");
  const rest = parseFloat(getComputedStyle(sheet).getPropertyValue("--penna-sheet-rest")) || 0;
  return panel.offsetHeight + rest + SHADOW_ROOM;
}

function sheetMove(sheet, offset) {
  const extent = sheetExtent(sheet);
  sheet.style.setProperty("--penna-sheet-offset", `${offset}px`);
  sheet.style.setProperty("--penna-sheet-fade", `${Math.max(0, 1 - Math.max(0, offset) / extent)}`);
}

function sheetClose(sheet) {
  sheetMove(sheet, sheetExtent(sheet));
  sheet.setAttribute("data-closed", "");
}

function sheetOpen(sheet) {
  sheet.removeAttribute("data-closed");
  sheetMove(sheet, 0);
}

function sheetDrag(sheet) {
  const panel = sheet.querySelector(".penna-sheet-panel");
  const content = sheet.querySelector(".penna-sheet-content");
  let drag = null;
  panel.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    drag = { id: event.pointerId, y: event.clientY, offset: 0, samples: [[event.timeStamp, event.clientY]], moved: false, inContent: content.contains(event.target) };
  });
  window.addEventListener("pointermove", (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    let delta = event.clientY - drag.y;
    if (!drag.moved) {
      if (Math.abs(delta) < 4) return;
      if (drag.inContent && (delta < 0 || content.scrollTop > 0)) {
        drag = null;
        return;
      }
      drag.moved = true;
      panel.setPointerCapture(event.pointerId);
      sheet.setAttribute("data-dragging", "");
    }
    const height = panel.offsetHeight;
    const offset = delta >= 0 ? delta : -height * 0.1 * (1 - 1 / (1 - delta / (height * 0.1) / 3));
    drag.offset = offset;
    drag.samples.push([event.timeStamp, event.clientY]);
    if (drag.samples.length > 6) drag.samples.shift();
    sheetMove(sheet, offset);
  });
  const release = (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const moved = drag.moved;
    const [[t0, y0]] = drag.samples;
    const [t1, y1] = drag.samples[drag.samples.length - 1];
    const velocity = t1 > t0 ? ((y1 - y0) / (t1 - t0)) * 1000 : 0;
    const offset = drag.offset;
    drag = null;
    if (!moved) return;
    sheet.removeAttribute("data-dragging");
    const height = panel.offsetHeight;
    if (event.type === "pointerup" && (offset > COMMIT * height || velocity > FLING * height)) sheetClose(sheet);
    else sheetMove(sheet, 0);
  };
  window.addEventListener("pointerup", release);
  window.addEventListener("pointercancel", release);
  panel.addEventListener("click", (event) => event.stopPropagation());
}

function sheetSetup(sheet) {
  sheetDrag(sheet);
  sheet.querySelector(".penna-sheet-scrim").addEventListener("click", () => sheetClose(sheet));
  sheet.querySelectorAll("[data-sheet-close]").forEach((button) => button.addEventListener("click", () => sheetClose(sheet)));
  sheet.addEventListener("keydown", (event) => {
    if (event.key === "Escape") sheetClose(sheet);
    if (!["ArrowDown", "ArrowUp"].includes(event.key)) return;
    const rows = [...sheet.querySelectorAll(".penna-sheet-row")];
    const at = rows.indexOf(document.activeElement);
    const next = rows[Math.min(rows.length - 1, Math.max(0, at + (event.key === "ArrowDown" ? 1 : -1)))];
    next?.focus();
    event.preventDefault();
  });
  sheet.querySelectorAll(".penna-sheet-row").forEach((row) => {
    const choose = () => {
      if (row.dataset.lead !== "check") return;
      row.parentElement.querySelectorAll('.penna-sheet-row[data-lead="check"]').forEach((other) => {
        const on = other === row;
        other.setAttribute("aria-checked", `${on}`);
        other.querySelector(".penna-sheet-check").toggleAttribute("data-checked", on);
      });
    };
    row.addEventListener("click", choose);
    row.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      choose();
    });
  });
}

function sheetGlass(root = document) {
  root.querySelectorAll(".penna-sheet-panel").forEach((panel) => {
    panel.style.removeProperty("backdrop-filter");
    const backdrop = getComputedStyle(panel).getPropertyValue("--penna-backdrop").trim();
    panel.toggleAttribute("data-glass", backdrop !== "" && backdrop !== "none");
    const brightness = backdrop.match(/brightness\(([\d.]+)\)/);
    if (!brightness) return;
    const lift = parseFloat(brightness[1]) - 1;
    const spread = 1 - 2 * lift;
    if (!lift || spread <= 0) return;
    const parts = backdrop.replace(brightness[0], "").replace(/\s+/g, " ").trim();
    panel.style.backdropFilter = `${parts} brightness(${1 / spread}) contrast(${spread})`;
  });
}

function sheetSettle() {
  sheetSnap();
  sheetGlass();
}

document.querySelectorAll(".penna-sheet").forEach(sheetSetup);
sheetSettle();
document.fonts.ready.then(sheetSettle);
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && sheetGlass(), true);
