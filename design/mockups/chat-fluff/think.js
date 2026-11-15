(() => {
  const { make, sine, quint, clamp, still, tidy } = window.fluff;
  const LABELS = ["Thinking", "Reading agent/view.rs", "Searching for popover", "Thinking"];
  const EVERY = 2400;
  const SWAP = 220, IN = 160, STAGGER = 18, OUT = 160;
  const SWEEP = 2000, REST = 600, DOT = 400;

  const blur = (el, px, alpha) => {
    el.style.filter = px > 0.01 ? `blur(${px.toFixed(2)}px)` : "";
    el.style.opacity = alpha >= 0.999 ? "" : alpha.toFixed(3);
  };
  const chars = (text) => [...text].map((ch) => `<i>${ch}</i>`).join("");

  const TAKES = {
    swap: {
      build: (say, from, to) => {
        say.innerHTML = `${from ? `<span data-old>${from}</span>` : ""}<span data-new>${to}</span>`;
      },
      length: () => SWAP,
      change: (say, ms) => {
        const e = sine(clamp(ms / SWAP, 0, 1));
        const old = say.querySelector("[data-old]");
        if (old) blur(old, 6 * e, 1 - e);
        blur(say.querySelector("[data-new]"), 6 * (1 - e), e);
      },
      rest: () => {},
    },
    wave: {
      build: (say, from, to) => {
        say.innerHTML = `${from ? `<span data-old>${from}</span>` : ""}<span data-new>${chars(to)}</span>`;
      },
      length: () => SWAP,
      change: (say, ms) => TAKES.swap.change(say, ms),
      rest: (say, ms) => {
        const letters = [...say.querySelectorAll("[data-new] > i")];
        const at = ms % (SWEEP + REST);
        const crest = -3 + (letters.length + 6) * Math.min(1, at / SWEEP);
        letters.forEach((letter, i) => {
          const d = (i + 0.5 - crest) / 3;
          const bump = Math.abs(d) < 1 ? (Math.cos(Math.PI * d) + 1) / 2 : 0;
          blur(letter, 2.5 * bump, 1 - 0.3 * bump);
        });
      },
    },
    resolve: {
      build: (say, from, to) => {
        say.innerHTML = `${from ? `<span data-old>${from}...</span>` : ""}<span data-new>${chars(to)}<i data-dot>.</i><i data-dot>.</i><i data-dot>.</i></span>`;
      },
      length: (say) => STAGGER * (say.querySelectorAll("[data-new] > i").length - 1) + IN,
      change: (say, ms) => {
        const old = say.querySelector("[data-old]");
        if (old) {
          const e = sine(clamp(ms / OUT, 0, 1));
          blur(old, 5 * e, 1 - e);
        }
        say.querySelectorAll("[data-new] > i").forEach((letter, i) => {
          const e = quint(clamp((ms - i * STAGGER) / IN, 0, 1));
          blur(letter, 5 * (1 - e), e);
        });
      },
      rest: (say, ms) => {
        const turn = Math.floor(ms / DOT) % 3;
        const local = (ms % DOT) / DOT;
        [...say.querySelectorAll("[data-dot]")].forEach((dot, i) => {
          const soft = i === turn ? Math.sin(Math.PI * local) : 0;
          blur(dot, 1.2 * soft, 1 - 0.5 * soft);
        });
      },
    },
  };

  const row = (detail) => `
    <div class="penna-chat-activity">
      <span class="penna-chat-label"><span class="fx-say"></span></span>
      <span class="penna-chat-detail">${detail}</span>
    </div>`;
  const bubble = `<div class="penna-chat-message"><div class="penna-card penna-chat-bubble" data-penna="card.bubble"><span>Why does the popover flicker when it opens?</span></div></div>`;

  const settle = (say) => {
    say.querySelector("[data-old]")?.remove();
    for (const el of say.querySelectorAll("[style]")) el.removeAttribute("style");
  };

  for (const chat of document.querySelectorAll("[data-think]")) {
    const take = chat.dataset.think;
    if (take === "today") {
      chat.innerHTML = `${bubble}<div class="penna-chat-activity"><span class="penna-chat-glyph penna-chat-breath"><i data-icon="sparkle"></i></span><span class="penna-chat-label penna-chat-shimmer">Thinking</span><span class="penna-chat-detail">14s</span></div>`;
      if (still) for (const el of chat.querySelectorAll(".penna-chat-shimmer, .penna-chat-breath [data-icon]")) el.style.animation = el.matches(".penna-chat-shimmer") ? "penna-chat-sweep 2600ms linear -900ms paused" : "penna-chat-breath 2600ms -900ms paused";
      continue;
    }
    const kind = TAKES[take];
    chat.innerHTML = bubble + row("12s");
    const say = chat.querySelector(".fx-say");
    const detail = chat.querySelector(".penna-chat-detail");

    const film = document.querySelector(`[data-film="${take}"]`);
    for (const p of [0, 0.33, 0.66, 1]) {
      film.append(make(`<span>${Math.round(p * 100)}%</span>`));
      const line = make(row("14s"));
      film.append(line);
      const s = line.querySelector(".fx-say");
      kind.build(s, LABELS[0], LABELS[1]);
      kind.change(s, p * kind.length(s));
      if (p === 1) settle(s);
    }

    if (still) {
      kind.build(say, null, LABELS[1]);
      detail.textContent = "14s";
      kind.rest(say, 1000);
      continue;
    }

    let index = 0, changed = performance.now(), length = 0, settled = true;
    const begin = performance.now();
    kind.build(say, null, LABELS[0]);
    const tick = (now) => {
      const run = Math.max(0, now - begin);
      detail.textContent = `${12 + Math.floor(run / 1000)}s`;
      const next = Math.floor(run / EVERY) % LABELS.length;
      if (next !== index) {
        const from = LABELS[index], to = LABELS[next];
        index = next;
        if (from !== to) {
          kind.build(say, from, to);
          changed = now;
          length = kind.length(say);
          settled = false;
          kind.change(say, 0);
        }
      }
      const since = Math.max(0, now - changed);
      if (!settled) {
        if (since >= length) {
          settle(say);
          settled = true;
        } else kind.change(say, since);
      }
      if (settled) kind.rest(say, Math.max(0, since - length));
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }
  tidy();
})();
