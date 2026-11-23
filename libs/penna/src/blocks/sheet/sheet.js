(() => {
const COMMIT = 0.48;
const FLING = 2;
const SHADOW_ROOM = 32;

function sheetExtent(sheet) {
  const panel = sheet.querySelector(".penna-sheet-panel");
  if (!panel) return 0;
  const rest = parseFloat(getComputedStyle(sheet).getPropertyValue("--penna-sheet-rest")) || 0;
  return panel.offsetHeight + rest + SHADOW_ROOM;
}

function sheetMove(sheet, offset) {
  const extent = sheetExtent(sheet);
  if (!extent) return;
  sheet.style.setProperty("--penna-sheet-offset", `${offset}px`);
  sheet.style.setProperty("--penna-sheet-fade", `${Math.max(0, 1 - Math.max(0, offset) / extent)}`);
}

function sheetClose(sheet) {
  sheetMove(sheet, sheetExtent(sheet));
  sheet.setAttribute("data-closed", "");
  sheet.querySelectorAll("[data-id]").forEach((node) => {
    node.dataset.sheetId = node.dataset.id;
    delete node.dataset.id;
  });
}

function sheetOpen(sheet) {
  sheet.removeAttribute("data-closed");
  sheet.querySelectorAll("[data-sheet-id]").forEach((node) => {
    node.dataset.id = node.dataset.sheetId;
    delete node.dataset.sheetId;
  });
  sheetMove(sheet, 0);
}

function sheetDrag(sheet) {
  const panel = sheet.querySelector(".penna-sheet-panel");
  if (!panel) return;
  const content = sheet.querySelector(".penna-sheet-content");
  let drag = null;
  panel.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("button")) return;
    drag = {
      id: event.pointerId,
      y: event.clientY,
      offset: 0,
      samples: [[event.timeStamp, event.clientY]],
      moved: false,
      inContent: Boolean(content?.contains(event.target)),
    };
  });
  window.addEventListener("pointermove", (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const delta = event.clientY - drag.y;
    if (!drag.moved) {
      if (Math.abs(delta) < 4) return;
      if (drag.inContent && content && (delta < 0 || content.scrollTop > 0)) {
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
  if (sheet.hasAttribute("data-still")) return;
  sheetDrag(sheet);
  sheet.querySelector(".penna-sheet-scrim")?.addEventListener("click", () => sheetClose(sheet));
  sheet.querySelectorAll("[data-sheet-close]").forEach((button) => button.addEventListener("click", () => sheetClose(sheet)));
}

function modalClose(modal) {
  modal.setAttribute("data-closed", "");
  modal.querySelectorAll("[data-id]").forEach((node) => {
    node.dataset.sheetId = node.dataset.id;
    delete node.dataset.id;
  });
}

function modalOpen(modal) {
  modal.removeAttribute("data-closed");
  modal.querySelectorAll("[data-sheet-id]").forEach((node) => {
    node.dataset.id = node.dataset.sheetId;
    delete node.dataset.sheetId;
  });
}

function modalSetup(modal) {
  if (modal.hasAttribute("data-still")) return;
  const close = () => modalClose(modal);
  modal.querySelector(".penna-modal-scrim")?.addEventListener("click", close);
  modal.querySelectorAll("[data-sheet-close]").forEach((button) => button.addEventListener("click", close));
}

document.querySelectorAll(".penna-sheet").forEach(sheetSetup);
document.querySelectorAll(".penna-modal").forEach(modalSetup);
document.querySelectorAll("[data-sheet-open]").forEach((button) => {
  button.addEventListener("click", () => {
    const target = document.getElementById(button.dataset.sheetOpen);
    if (target) sheetOpen(target);
  });
});
document.querySelectorAll("[data-modal-open]").forEach((button) => {
  button.addEventListener("click", () => {
    const target = document.getElementById(button.dataset.modalOpen);
    if (target) modalOpen(target);
  });
});

new MutationObserver(() => {
  document.querySelectorAll(".penna-sheet[data-closed]:not([data-reopenable])").forEach(sheetOpen);
  document.querySelectorAll(".penna-modal[data-closed]:not([data-reopenable])").forEach(modalOpen);
}).observe(document.documentElement, { attributes: true, attributeFilter: ["data-penna-theme", "data-penna-mode"] });

globalThis.pennaSheet = { open: sheetOpen, close: sheetClose, openModal: modalOpen, closeModal: modalClose };
})();
