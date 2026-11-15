(() => {
  const { make, quint, away, still, tidy } = window.fluff;

  const TAKES = {
    rise: { enter: 160, exit: 100 },
    grow: { enter: 140, exit: 90 },
    dissolve: { enter: 180, exit: 120 },
  };
  const HOLD = 900, GAP = 420;

  const rank = (x, y) => {
    let r = 0;
    for (let i = 0; i < 3; i++) r = r * 4 + (((((x ^ y) >> i) & 1) << 1) | ((y >> i) & 1));
    return r;
  };
  const masks = Array.from({ length: 9 }, (_, k) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#000";
    for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) if (rank(x, y) < k * 8) ctx.fillRect(x * 8, y * 8, 8, 8);
    return `url(${canvas.toDataURL()})`;
  });

  const models = ["Default", "Opus 5.5", "Fable 5.2", "Sonnet 5.5", "Haiku 5.5", "Fable 5.1"];
  const picker = () => `
    <div class="fx-scene fx-pick">
      <div class="fx-slot">
        <div class="zz-pop" data-penna="popover">
          <div class="zz-head">Model</div>
          ${models.map((name) => `<div data-penna="menu-item"${name === "Opus 5.5" ? ' data-state="selected"' : ""}><span>${name}</span><i data-icon="check"></i></div>`).join("")}
          <div class="zz-sep"></div>
          <div class="zz-head"><span>Effort</span><b>High</b></div>
          <div class="zz-slide"><i class="zz-fill"></i>${[0, 1, 2, 3, 4, 5].map((i) => `<b style="--i: ${i}"${i < 3 ? " data-on" : ""}></b>`).join("")}<i class="zz-knob"></i></div>
        </div>
      </div>
      <div class="zz-composer" data-penna="field.composer">
        <button class="zz-plus" data-penna="icon-button"><i data-icon="plus"></i></button>
        <div class="zz-input penna-muted">Message Claude Code...</div>
        <button data-penna="button.send" data-prominent data-state="disabled"><i data-icon="arrow-up"></i></button>
      </div>
      <div class="zz-foot">
        <span class="zz-grow"></span>
        <span class="zz-chip" data-penna="chip"><i data-icon="circle-user"></i>Ask</span>
        <span class="zz-chip" data-penna="chip" data-state="current">Opus 5.5<span class="zz-dim">High</span><i data-icon="chevron-down"></i></span>
      </div>
    </div>`;

  const files = [
    ["folder", "crates/zz-ui/src/agent", ""],
    ["file", "view.rs", "agent"],
    ["file", "composer.rs", "agent"],
    ["file", "popover.rs", "agent/chat"],
    ["file", "mention.rs", "agent/chat"],
  ];
  const mention = () => `
    <div class="fx-scene fx-at">
      <div class="fx-slot">
        <div class="fx-mention" data-penna="popover">
          <div class="zz-filter" data-penna="field"><i data-icon="search"></i><span>agent</span></div>
          ${files.map(([icon, name, where], i) => `<div data-penna="menu-item"${i === 1 ? ' data-state="current"' : ""}><i data-icon="${icon}"></i><span>${name}</span>${where ? `<span class="fx-where">${where}</span>` : ""}</div>`).join("")}
        </div>
      </div>
      <div class="zz-composer" data-penna="field.composer">
        <button class="zz-plus" data-penna="icon-button"><i data-icon="plus"></i></button>
        <div class="zz-input fx-typed">Look at <em>@agent</em></div>
        <button data-penna="button.send" data-prominent><i data-icon="arrow-up"></i></button>
      </div>
      <div class="zz-foot">
        <span class="zz-grow"></span>
        <span class="zz-chip" data-penna="chip"><i data-icon="circle-user"></i>Ask</span>
        <span class="zz-chip" data-penna="chip">Opus 5.5<span class="zz-dim">High</span><i data-icon="chevron-down"></i></span>
      </div>
    </div>`;

  const pose = (pop, take, phase, t, origin) => {
    pop.style.transformOrigin = origin;
    pop.classList.toggle("fx-masked", take === "dissolve");
    if (take === "dissolve") {
      const k = phase === "in" ? Math.floor(t * 8 + 1e-6) : 8 - Math.floor(t * 8 + 1e-6);
      pop.style.visibility = k ? "" : "hidden";
      pop.style.webkitMaskImage = pop.style.maskImage = k >= 8 ? "none" : masks[k];
      return;
    }
    const v = phase === "in" ? quint(t) : 1 - away(t);
    pop.style.opacity = v;
    pop.style.transform = take === "rise" ? `translateY(${(6 * (1 - v)).toFixed(2)}px)` : `scale(${(0.96 + 0.04 * v).toFixed(4)})`;
  };

  const FRAMES = [
    ["in", 0, "0%"],
    ["in", 0.4, "40%"],
    ["in", 1, "100%"],
    ["out", 0.5, "out 50%"],
  ];

  for (const figure of document.querySelectorAll("[data-motion]")) {
    const take = figure.dataset.motion;
    const { enter, exit } = TAKES[take];
    const strip = figure.querySelector(".fx-strip");
    for (const [phase, t, label] of FRAMES) {
      const shot = make(`<div class="fx-shot"><div class="fx-cell">${picker()}</div><span>${label}</span></div>`);
      strip.append(shot);
      pose(shot.querySelector("[data-penna='popover']"), take, phase, t, "100% 100%");
    }

    const live = figure.querySelector(".fx-live");
    const demos = [
      [picker(), "100% 100%"],
      [mention(), "50% 100%"],
    ].map(([html, origin]) => {
      const demo = make(`<div class="fx-demo">${html}</div>`);
      live.append(demo);
      const slot = demo.querySelector(".fx-slot");
      slot.style.outline = "none";
      return { pop: slot.firstElementChild, origin };
    });
    const replay = make(`<button class="fx-replay" data-penna="button" type="button">Replay</button>`);
    live.firstElementChild.append(replay);

    const show = (phase, t) => {
      for (const { pop, origin } of demos) pose(pop, take, phase, t, origin);
    };
    show("in", 1);
    if (still) {
      show("in", 0.75);
      continue;
    }

    let start = 0, hovered = false, running = false;
    const cycle = enter + HOLD + exit + GAP;
    const frame = (now) => {
      let at = now - start;
      if (at >= cycle) {
        if (!hovered) {
          running = false;
          show("in", 1);
          return;
        }
        start = now;
        at = 0;
      }
      if (at < enter) show("in", at / enter);
      else if (at < enter + HOLD) show("in", 1);
      else if (at < enter + HOLD + exit) show("out", (at - enter - HOLD) / exit);
      else show("out", 1);
      requestAnimationFrame(frame);
    };
    const play = () => {
      if (running) return;
      running = true;
      start = performance.now();
      requestAnimationFrame(frame);
    };
    replay.addEventListener("click", play);
    replay.addEventListener("mouseenter", () => {
      hovered = true;
      play();
    });
    replay.addEventListener("mouseleave", () => {
      hovered = false;
    });
  }
  tidy();
})();
