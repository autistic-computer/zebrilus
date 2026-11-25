(() => {
  const root = document.querySelector("[data-docs-motion]");
  if (!root) return;

  const speedSeg = root.querySelector("[data-mo-speed]");
  const replayAllBtn = root.querySelector("[data-mo-replay]");
  const surfaceEl = root.querySelector("[data-mo-surface]");
  const toggleSurfaceBtn = root.querySelector("[data-mo-toggle-surface]");
  const discBtn = root.querySelector("[data-mo-disclosure]");
  const discBody = root.querySelector("[data-mo-disc-body]");
  const rowsHost = root.querySelector("[data-mo-rows]");
  const replayRowsBtn = root.querySelector("[data-mo-replay-rows]");
  const appearEl = root.querySelector("[data-mo-appear]");
  const replayAppearBtn = root.querySelector("[data-mo-replay-appear]");
  const spinnerEl = root.querySelector("[data-mo-spinner]");

  const msSurface = root.querySelector('[data-mo-ms="surface"]');
  const msTurn = root.querySelector('[data-mo-ms="turn"]');
  const msEnter = root.querySelector('[data-mo-ms="enter"]');
  const msSpin = root.querySelector('[data-mo-ms="spin"]');

  let mode = "default";

  function factor() {
    if (mode === "off") return 0;
    if (mode === "fast") return 0.5;
    return 1;
  }

  function applySpeed() {
    const f = factor();
    root.dataset.moMode = mode;
    root.style.setProperty("--mo-enter-ms", `${Math.round(160 * f)}ms`);
    root.style.setProperty("--mo-leave-ms", `${Math.round(100 * f)}ms`);
    root.style.setProperty("--mo-turn-ms", `${Math.round(220 * f)}ms`);
    root.style.setProperty("--mo-row-ms", `${Math.round(200 * f)}ms`);
    root.style.setProperty("--mo-appear-ms", `${Math.round(120 * f)}ms`);
    root.style.setProperty("--mo-spin-ms", f === 0 ? "0ms" : `${Math.round(900 * f)}ms`);
    if (msSurface) msSurface.textContent = f === 0 ? "0 ms · instant" : `${Math.round(160 * f)} ms in · ${Math.round(100 * f)} ms out`;
    if (msTurn) msTurn.textContent = f === 0 ? "0 ms · instant" : `${Math.round(220 * f)} ms · 90°`;
    if (msEnter) msEnter.textContent = f === 0 ? "0 ms · instant" : `${Math.round(200 * f)} ms · 8 px drop`;
    if (msSpin) msSpin.textContent = f === 0 ? "static · reduced" : `${Math.round(120 * f)} ms · ${Math.round(900 * f)} ms loop`;
    if (spinnerEl) spinnerEl.toggleAttribute("data-paused", f === 0);
  }

  function replaySurface() {
    if (!surfaceEl) return;
    surfaceEl.dataset.shown = "false";
    void surfaceEl.offsetWidth;
    setTimeout(() => {
      surfaceEl.dataset.shown = "true";
    }, mode === "off" ? 20 : Math.max(120, Math.round(140 * factor()) + 40));
  }

  function replayRows() {
    if (!rowsHost) return;
    const rows = [...rowsHost.querySelectorAll(".docs-mo-row")];
    rows.forEach((row) => row.classList.remove("is-entering"));
    void rowsHost.offsetWidth;
    rows.forEach((row, index) => {
      row.style.setProperty("--mo-delay", `${Math.round(index * 45 * factor())}ms`);
      row.classList.add("is-entering");
    });
  }

  function replayAppear() {
    if (!appearEl) return;
    appearEl.classList.remove("is-appearing");
    void appearEl.offsetWidth;
    appearEl.classList.add("is-appearing");
  }

  speedSeg?.addEventListener("change", (event) => {
    mode = event.detail.index === 1 ? "fast" : event.detail.index === 2 ? "off" : "default";
    applySpeed();
  });

  toggleSurfaceBtn?.addEventListener("click", () => {
    if (!surfaceEl) return;
    surfaceEl.dataset.shown = surfaceEl.dataset.shown === "true" ? "false" : "true";
  });

  discBtn?.addEventListener("click", () => {
    const open = discBtn.getAttribute("aria-expanded") !== "true";
    discBtn.setAttribute("aria-expanded", String(open));
    if (discBody) discBody.hidden = !open;
  });

  replayRowsBtn?.addEventListener("click", replayRows);
  replayAppearBtn?.addEventListener("click", replayAppear);

  replayAllBtn?.addEventListener("click", () => {
    replaySurface();
    replayRows();
    replayAppear();
    if (discBtn) {
      const next = discBtn.getAttribute("aria-expanded") !== "true";
      discBtn.setAttribute("aria-expanded", String(next));
      if (discBody) discBody.hidden = !next;
    }
  });

  applySpeed();
  requestAnimationFrame(() => {
    window.pennaKit?.roundAll?.(root);
    window.pennaSegmented?.wire?.(root);
  });
})();
