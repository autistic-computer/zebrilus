(() => {
  const hero = document.querySelector("[data-docs-hero]");
  if (!hero) return;

  const canvas = hero.querySelector(".docs-hero-canvas");
  const skySlider = hero.querySelector("[data-hero-sky]");
  const searchKey = hero.querySelector("[data-hero-search-key]");

  if (searchKey && window.pennaKbd && !window.pennaKbd.mac) {
    searchKey.textContent = "Ctrl+K";
  }

  const BAYER = [
    0, 8, 2, 10,
    12, 4, 14, 6,
    3, 11, 1, 9,
    15, 7, 13, 5,
  ];

  const PLUME = [
    "................HHHH....",
    "..............HH#####...",
    "............HH#######H..",
    "...........H##########..",
    ".........HH###########..",
    "........H########S###S..",
    ".......H#######.######..",
    "......H#######.#####S...",
    ".....H#######.###S......",
    ".....H######.#######S...",
    "....H######.#######S....",
    "....H#####.#######S.....",
    "....H####.#######S......",
    "...H####.######S........",
    "...H###.#####S..........",
    "...H##.######S..........",
    "...H#.#####S............",
    "...H######S.............",
    "...H####S...............",
    "..H###S.................",
    "..##S...................",
    ".FG.....................",
    ".FF.....................",
    ".F......................",
  ];

  const BIRD = [
    [
      "..WW........",
      "...WW.......",
      "....WW.###..",
      "#...WW##E#BB",
      ".#####W###..",
      "..########..",
      "...######...",
    ],
    [
      "............",
      "..WWW..###..",
      "#..WWWW#E#BB",
      ".#########..",
      "..########..",
      "...######...",
      "............",
    ],
    [
      "............",
      ".......###..",
      "#....###E#BB",
      ".####WW###..",
      "..###WWW##..",
      "....WWWWW...",
      ".....WW.....",
    ],
    [
      "............",
      ".....W.###..",
      "#...WW##E#BB",
      ".###WW####..",
      "..########..",
      "...######...",
      "............",
    ],
  ];

  const DARK_RAMP = [
    [24, 24, 23],
    [29, 39, 54],
    [39, 69, 102],
    [58, 108, 152],
    [76, 134, 182],
  ];

  const LIGHT_RAMP = [
    [52, 141, 207],
    [103, 196, 242],
    [188, 229, 248],
    [240, 239, 234],
    [247, 234, 214],
  ];

  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  let animate = !reduced.matches;
  let elevation = 0.56;
  let pointerX = 0;
  let pointerY = 0;
  let targetX = 0;
  let targetY = 0;
  let elapsed = 0;
  let lastFrame = 0;
  let rafId = 0;
  let visible = true;
  let gridW = 0;
  let gridH = 0;
  let img = null;

  const ctx = canvas?.getContext("2d", { alpha: false });

  function hash(x, y) {
    let n = Math.imul(x + 374761393, 668265263) ^ Math.imul(y + 1274126177, 2246822519);
    n = Math.imul(n ^ (n >>> 13), 1274126177);
    return ((n ^ (n >>> 16)) >>> 0) / 4294967295;
  }

  function noise(x, y) {
    const ix = Math.floor(x);
    const iy = Math.floor(y);
    const fx = x - ix;
    const fy = y - iy;
    const ux = fx * fx * (3 - 2 * fx);
    const uy = fy * fy * (3 - 2 * fy);
    const a = hash(ix, iy);
    const b = hash(ix + 1, iy);
    const c = hash(ix, iy + 1);
    const d = hash(ix + 1, iy + 1);
    return a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
  }

  function fbm(x, y) {
    return (
      0.56 * noise(x, y) +
      0.30 * noise(x * 2.04 + 13.7, y * 2.04 + 29.1) +
      0.14 * noise(x * 4.12 + 31.3, y * 4.12 + 7.9)
    );
  }

  function isLight() {
    return document.documentElement.dataset.pennaMode === "light";
  }

  function put(data, w, h, x, y, r, g, b) {
    if (x < 0 || y < 0 || x >= w || y >= h) return;
    const at = (y * w + x) * 4;
    data[at] = r;
    data[at + 1] = g;
    data[at + 2] = b;
    data[at + 3] = 255;
  }

  function putBlock(data, w, h, x, y, scale, rgb) {
    for (let dy = 0; dy < scale; dy++) {
      for (let dx = 0; dx < scale; dx++) {
        put(data, w, h, x + dx, y + dy, rgb[0], rgb[1], rgb[2]);
      }
    }
  }

  function resize() {
    if (!canvas || !ctx) return;
    const box = hero.getBoundingClientRect();
    const cell = 3;
    const nextW = Math.max(120, Math.ceil(box.width / cell));
    const nextH = Math.max(80, Math.ceil(box.height / cell));
    if (nextW === gridW && nextH === gridH) return;
    gridW = nextW;
    gridH = nextH;
    canvas.width = gridW;
    canvas.height = gridH;
    img = ctx.createImageData(gridW, gridH);
    draw();
  }

  function draw() {
    if (!ctx || !img) return;
    const w = gridW;
    const h = gridH;
    const data = img.data;
    const light = isLight();
    const ramp = light ? LIGHT_RAMP : DARK_RAMP;
    const shift = (elevation - 0.5) * 0.42;
    const px = pointerX * 5;
    const py = pointerY * 3;
    const drift = elapsed * 0.09;

    const sunX = Math.round(w * (0.14 + elevation * 0.24) - px * 0.35);
    const sunY = Math.round(h * (0.30 - elevation * 0.14) - py * 0.35);

    for (let y = 0; y < h; y++) {
      const ny = y / h;
      const by = (y & 3) * 4;
      for (let x = 0; x < w; x++) {
        const thresh = (BAYER[by + (x & 3)] + 0.5) / 16;
        const cloudA = fbm((x + px) * 0.022 - drift, (y + py) * 0.044);
        const cloudB = fbm((x + px * 1.4) * 0.036 - drift * 1.6 + 11.4, (y + py) * 0.062 + 5.2);
        const ribbon = Math.pow(Math.max(0, 1 - Math.abs(cloudB * 2 - 1)), 1.65);

        const dx = x - sunX;
        const dy = y - sunY;
        const dist = Math.hypot(dx, dy);
        const halo = Math.exp(-dist * 0.038) * (0.45 + elevation * 0.4);

        let tone;
        if (light) {
          tone = ny * 0.92 + (cloudA - 0.48) * 0.36 + ribbon * 0.28 * (1 - ny * 0.45) - shift * 0.42;
        } else {
          tone = (1 - ny * 0.95) * 0.62 + cloudA * 0.30 + ribbon * 0.34 * (1 - ny * 0.55) + halo * 0.48 + shift * 0.45;
        }

        const clamped = Math.max(0, Math.min(0.999, tone));
        const glowBand = !light
          ? halo * 0.55
          : halo > 0.34
            ? (halo - 0.34) * 1.1
            : 0;

        let rgb;
        if (glowBand > thresh * 0.85) {
          rgb = ramp[4];
        } else {
          const scaled = clamped * 3;
          const low = Math.min(2, Math.floor(scaled));
          const pick = scaled - low > thresh ? low + 1 : low;
          rgb = ramp[pick];
        }

        const at = (y * w + x) * 4;
        data[at] = rgb[0];
        data[at + 1] = rgb[1];
        data[at + 2] = rgb[2];
        data[at + 3] = 255;
      }
    }

    if (!light) {
      const cell = 11;
      for (let sy = 1; sy < h * 0.72; sy += cell) {
        for (let sx = 1; sx < w - 2; sx += cell) {
          const gx = Math.floor(sx / cell);
          const gy = Math.floor(sy / cell);
          const r = hash(gx + 19, gy + 73);
          if (r < 0.70) continue;
          const ox = Math.floor(hash(gx + 41, gy + 13) * (cell - 2));
          const oy = Math.floor(hash(gx + 83, gy + 29) * (cell - 2));
          const pxStar = sx + ox;
          const pyStar = sy + oy;
          const twinkle = 0.5 + 0.5 * Math.sin(elapsed * (1.2 + r * 2.1) + r * 50);
          if (twinkle < 0.28) continue;
          const bright = r > 0.90 ? [246, 243, 245] : [182, 198, 218];
          put(data, w, h, pxStar, pyStar, bright[0], bright[1], bright[2]);
          if (r > 0.93 && twinkle > 0.65) {
            put(data, w, h, pxStar - 1, pyStar, 150, 174, 200);
            put(data, w, h, pxStar + 1, pyStar, 150, 174, 200);
            put(data, w, h, pxStar, pyStar - 1, 150, 174, 200);
            put(data, w, h, pxStar, pyStar + 1, 150, 174, 200);
          }
        }
      }
    }

    const orbR = light ? 6 : 7;
    const orbCol = light ? [246, 206, 84] : [246, 195, 48];
    for (let dy = -orbR; dy <= orbR; dy++) {
      for (let dx = -orbR; dx <= orbR; dx++) {
        const d1 = Math.hypot(dx, dy) / orbR;
        if (d1 > 1) continue;
        if (!light) {
          const d2 = Math.hypot(dx - orbR * 0.44, dy + orbR * 0.22) / (orbR * 0.84);
          if (d2 < 0.92) continue;
        }
        put(data, w, h, sunX + dx, sunY + dy, orbCol[0], orbCol[1], orbCol[2]);
      }
    }

    const birdFrame = BIRD[Math.floor(elapsed * 4.5) % BIRD.length];
    const birdX = Math.floor(((elapsed * 7.5 + w * 0.56) % (w + 28)) - 14);
    const birdY = Math.round(h * 0.16 + Math.sin(elapsed * 1.4) * 2.2);
    const birdBody = light ? [30, 26, 28] : [246, 243, 245];
    const birdWing = light ? [64, 58, 62] : [206, 202, 206];
    const birdBeak = [242, 88, 32];
    for (let row = 0; row < birdFrame.length; row++) {
      const line = birdFrame[row];
      for (let col = 0; col < line.length; col++) {
        const ch = line[col];
        if (ch === ".") continue;
        const rgb = ch === "B" ? birdBeak : ch === "W" ? birdWing : ch === "E" ? (light ? [246, 243, 245] : [30, 26, 28]) : birdBody;
        put(data, w, h, birdX + col, birdY + row, rgb[0], rgb[1], rgb[2]);
      }
    }

    const plumeScale = w >= 180 ? 2 : 1;
    const plumeSize = 24 * plumeScale;
    const bob = Math.round(Math.sin(elapsed * 1.55) * 1.4);
    const plumeX = Math.round(w * (w >= 180 ? 0.27 : 0.14) - plumeSize * 0.5 + px * 0.55);
    const plumeY = Math.round(h * 0.56 - plumeSize * 0.5 + bob + py * 0.45);

    const palette = light
      ? {
          H: [54, 52, 56],
          "#": [30, 26, 28],
          S: [78, 75, 79],
          F: [242, 88, 32],
          G: [246, 195, 48],
        }
      : {
          H: [255, 255, 255],
          "#": [238, 236, 232],
          S: [164, 162, 158],
          F: [242, 88, 32],
          G: [246, 195, 48],
        };

    for (let row = 0; row < PLUME.length; row++) {
      const line = PLUME[row];
      for (let col = 0; col < line.length; col++) {
        const ch = line[col];
        const rgb = palette[ch];
        if (!rgb) continue;
        putBlock(data, w, h, plumeX + col * plumeScale, plumeY + row * plumeScale, plumeScale, rgb);
      }
    }

    const nibX = plumeX + plumeScale;
    const nibY = plumeY + 23 * plumeScale;
    for (let i = 0; i < 4; i++) {
      const phase = (elapsed * 0.38 + i * 0.25) % 1;
      const mx = Math.round(nibX - 3 - phase * 10 + Math.sin(phase * 6.2 + i) * 2);
      const my = Math.round(nibY + 1 - phase * 14);
      const rgb = i % 2 === 0 ? [242, 88, 32] : [246, 195, 48];
      if (phase < 0.85) put(data, w, h, mx, my, rgb[0], rgb[1], rgb[2]);
    }

    ctx.putImageData(img, 0, 0);
  }

  function tick(now) {
    if (!lastFrame) lastFrame = now;
    const dt = Math.min(0.05, (now - lastFrame) / 1000);
    lastFrame = now;
    pointerX += (targetX - pointerX) * 0.1;
    pointerY += (targetY - pointerY) * 0.1;
    if (animate) elapsed += dt;
    draw();
    if (visible && (animate || Math.abs(targetX - pointerX) > 0.005 || Math.abs(targetY - pointerY) > 0.005)) {
      rafId = requestAnimationFrame(tick);
    } else {
      rafId = 0;
      lastFrame = 0;
    }
  }

  function kick() {
    if (!rafId && visible) rafId = requestAnimationFrame(tick);
  }

  hero.addEventListener("pointermove", (event) => {
    const box = hero.getBoundingClientRect();
    targetX = ((event.clientX - box.left) / Math.max(box.width, 1) - 0.5) * 2;
    targetY = ((event.clientY - box.top) / Math.max(box.height, 1) - 0.5) * 2;
    kick();
  });

  hero.addEventListener("pointerleave", () => {
    targetX = 0;
    targetY = 0;
    kick();
  });

  skySlider?.addEventListener("change", (event) => {
    const value = Number(event.detail.value);
    if (!Number.isFinite(value)) return;
    elevation = value / 100;
    draw();
  });

  hero.querySelectorAll("[data-hero-action]").forEach((row) => {
    const run = () => {
      const action = row.dataset.heroAction;
      if (action === "search") {
        document.querySelector("[data-docs-search]")?.click();
      } else if (action === "foundations") {
        location.href = "foundations.html";
      } else if (action === "components") {
        location.href = "button.html";
      } else if (action === "look") {
        location.href = "look.html";
      } else if (action === "mode") {
        pennaKit.choice.mode = pennaKit.choice.mode === "dark" ? "light" : "dark";
        pennaKit.apply();
      }
    };
    row.addEventListener("click", run);
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        run();
      }
    });
  });

  document.addEventListener("penna-theme", () => {
    requestAnimationFrame(draw);
  });
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && draw(), true);
  reduced.addEventListener("change", () => {
    animate = !reduced.matches;
    if (animate) kick();
    else draw();
  });

  new ResizeObserver(resize).observe(hero);
  if ("IntersectionObserver" in window) {
    new IntersectionObserver((entries) => {
      visible = entries.some((entry) => entry.isIntersecting);
      if (visible && animate) kick();
    }).observe(hero);
  }

  resize();
  if (animate) kick();
})();
