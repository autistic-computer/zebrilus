(() => {
  const params = new URLSearchParams(location.search);
  const only = params.get("only");
  if (/^[abc]$/.test(only ?? "")) {
    document.documentElement.dataset.only = only;
    for (const part of document.querySelectorAll("[data-part]")) part.hidden = part.dataset.part !== only;
  }

  const bezier = (x1, y1, x2, y2) => (t) => {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    let lo = 0, hi = 1, s = t;
    for (let i = 0; i < 32; i++) {
      s = (lo + hi) / 2;
      const x = 3 * (1 - s) * (1 - s) * s * x1 + 3 * (1 - s) * s * s * x2 + s * s * s;
      if (x < t) lo = s;
      else hi = s;
    }
    return 3 * (1 - s) * (1 - s) * s * y1 + 3 * (1 - s) * s * s * y2 + s * s * s;
  };

  window.fluff = {
    still: params.has("still"),
    calm: matchMedia("(prefers-reduced-motion: reduce)").matches,
    bezier,
    quint: bezier(0.22, 1, 0.36, 1),
    away: bezier(0.4, 0, 1, 1),
    sine: bezier(0.37, 0, 0.63, 1),
    clamp: (v, lo, hi) => Math.min(hi, Math.max(lo, v)),
    tidy: () => {
      window.pennaKit?.roundAll?.();
    },
    make: (html) => {
      const box = document.createElement("template");
      box.innerHTML = html.trim();
      return box.content.firstChild;
    },
  };

  const pane = `
    <section data-penna="pane">
      <div data-penna="bar.pane">
        <div data-penna="button.title"><i data-icon="asterisk"></i>Claude Code<span class="zz-cwd">~/dev/autistic-computer/zebrilus</span></div>
        <span class="zz-grow"></span>
        <button data-penna="icon-button.bar"><i data-icon="panel-bottom"></i></button>
        <button data-penna="icon-button.bar"><i data-icon="layout-columns"></i></button>
        <button data-penna="icon-button.bar"><i data-icon="xmark"></i></button>
      </div>
      <div class="fx-body">
        <canvas class="fx-sky"></canvas>
        <div class="zz-empty">
          <div class="zz-empty-col">
            <div class="zz-hero"><i data-icon="asterisk"></i><b>Claude Code is ready</b><span>Ask it anything, or pick up a chat.</span></div>
            <div class="zz-project">
              <button class="zz-pbtn" data-penna="button" type="button"><i data-icon="folder"></i><span>zebrilus</span><i data-icon="chevron-down"></i></button>
              <button class="zz-pbtn" data-penna="button.quiet" type="button"><i data-icon="history"></i><span>Resume</span><i data-icon="chevron-down"></i></button>
            </div>
          </div>
        </div>
        <div class="zz-dock">
          <div class="zz-composer" data-penna="field.composer">
            <button class="zz-plus" data-penna="icon-button"><i data-icon="plus"></i></button>
            <div class="zz-input penna-muted">Message Claude Code...</div>
            <button data-penna="button.send" data-prominent data-state="disabled"><i data-icon="arrow-up"></i></button>
          </div>
          <div class="zz-foot">
            <span class="zz-grow"></span>
            <span class="zz-chip" data-penna="chip"><i data-icon="circle-user"></i>Ask</span>
            <span class="zz-chip" data-penna="chip">Opus 5.5<span class="zz-dim">High</span><i data-icon="chevron-down"></i></span>
          </div>
        </div>
      </div>
    </section>`;
  for (const frame of document.querySelectorAll("[data-sky]")) frame.innerHTML = pane;
})();
