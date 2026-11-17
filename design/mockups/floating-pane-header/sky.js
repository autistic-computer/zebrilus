(() => {
  const UP = [
    "..W........",
    "..WW.####..",
    "...WW##E#BB",
    "#..WWW####.",
    ".#########.",
    "..#######..",
    "...........",
  ];
  const DOWN = [
    "...........",
    ".....####..",
    "....###E#BB",
    "#..#######.",
    ".##WWW####.",
    "..#WWW###..",
    "...WW......",
  ];
  const SW = 11, SH = 7, CELL = 4, PAD = 10, FADE = 72, STILL = 40;
  const FLOCK = [
    { lane: 0.2, scale: 1 },
    { lane: 0.75, scale: 1 },
    { lane: 0.45, scale: 1 },
    { lane: 0.1, scale: 2 },
    { lane: 0.6, scale: 2 },
  ];
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const probe = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  const rgb = (css) => {
    probe.clearRect(0, 0, 1, 1);
    probe.fillStyle = "#000";
    probe.fillStyle = css;
    probe.fillRect(0, 0, 1, 1);
    return [...probe.getImageData(0, 0, 1, 1).data.slice(0, 3)];
  };
  const mix = (a, b, t) => a.map((v, i) => Math.round(v + (b[i] - v) * t));
  const FLAME = rgb("#f25820");
  const smoothstep = (a, b, v) => {
    const t = clamp((v - a) / (b - a), 0, 1);
    return t * t * (3 - 2 * t);
  };
  const hash = (px, py) => {
    let x = (Math.floor(px) + 65536) >>> 0;
    let y = (Math.floor(py) + 65536) >>> 0;
    x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
    y = (Math.imul(y, 1664525) + 1013904223) >>> 0;
    x = (x + Math.imul(y, 1664525)) >>> 0;
    y = (y + Math.imul(x, 1664525)) >>> 0;
    x = (x ^ (x >>> 16)) >>> 0;
    y = (y ^ (y >>> 16)) >>> 0;
    x = (x + Math.imul(y, 1664525)) >>> 0;
    x = (x ^ (x >>> 16)) >>> 0;
    return (x >>> 8) / 16777215;
  };
  const noise = (px, py) => {
    const ix = Math.floor(px), iy = Math.floor(py), fx = px - ix, fy = py - iy;
    const ux = fx * fx * (3 - 2 * fx), uy = fy * fy * (3 - 2 * fy);
    const a = hash(ix, iy), b = hash(ix + 1, iy), c = hash(ix, iy + 1), d = hash(ix + 1, iy + 1);
    const top = a + (b - a) * ux, bottom = c + (d - c) * ux;
    return top + (bottom - top) * uy;
  };
  const fbm = (px, py) => {
    let sum = 0, amp = 0.5;
    for (let i = 0; i < 4; i++) {
      sum += amp * noise(px, py);
      px = px * 2.03 + 13.7;
      py = py * 2.03 + 7.3;
      amp *= 0.5;
    }
    return sum / 0.9375;
  };
  const bayer = (cx, cy) => {
    let rank = 0;
    for (let i = 0; i < 3; i++) rank = rank * 4 + (((((cx ^ cy) >> i) & 1) << 1) | ((cy >> i) & 1));
    return (rank + 0.5) / 64;
  };
  const bits = (rows, marks) => rows.map((row) => [...row].map((ch) => marks.includes(ch)));
  const INK = [bits(UP, "#W"), bits(DOWN, "#W")];
  const BEAK = [bits(UP, "B"), bits(DOWN, "B")];
  const range = document.createRange();
  const textRects = (node) => {
    range.selectNodeContents(node);
    return [...range.getClientRects()].filter((r) => r.width > 0);
  };

  const paintSky = (body, seed) => {
    const canvas = body.querySelector(".hf-sky");
    const ctx = canvas.getContext("2d");
    const box = body.getBoundingClientRect();
    const width = box.width, height = box.height;
    const cols = Math.ceil(width / CELL), rows = Math.ceil(height / CELL);
    if (!cols || !rows) return;
    canvas.width = cols;
    canvas.height = rows;
    canvas.style.width = `${cols * CELL}px`;
    canvas.style.height = `${rows * CELL}px`;

    const rect = (r) => [r.left - box.left - PAD, r.top - box.top - PAD, r.right - box.left + PAD, r.bottom - box.top + PAD].map((v) => v / CELL);
    const union = (list) => {
      const all = list.map((el) => el.getBoundingClientRect());
      return { left: Math.min(...all.map((r) => r.left)), top: Math.min(...all.map((r) => r.top)), right: Math.max(...all.map((r) => r.right)), bottom: Math.max(...all.map((r) => r.bottom)) };
    };
    const hero = body.querySelector(".hf-hero");
    const zones = [
      [-PAD, height - PAD, width + PAD, height + PAD].map((v) => v / CELL),
      rect(hero.querySelector("[data-icon]").getBoundingClientRect()),
      ...textRects(hero.querySelector("b")).map(rect),
      ...textRects(hero.querySelector("span")).map(rect),
      rect(union([...body.querySelectorAll(".hf-pbtn")])),
      rect(union([body.querySelector(".hf-composer"), ...body.querySelectorAll(".hf-chip")])),
    ];

    const style = getComputedStyle(body.closest("[data-penna='pane']"));
    const ground = rgb(style.backgroundColor);
    const ink = rgb(getComputedStyle(document.documentElement).getPropertyValue("--penna-fg").trim());
    const accent = rgb(getComputedStyle(document.documentElement).getPropertyValue("--penna-accent").trim());
    const night = document.documentElement.dataset.pennaMode !== "light";
    const steps = [0.12, 0.24, 0.4].map((t) => mix(ground, accent, t));
    const ramp = night ? [ground, ...steps] : [...steps.reverse(), ground];

    const fade = FADE / CELL;
    const clearance = (x, y) => Math.min(...zones.map(([l, t, r, b]) => Math.hypot(Math.max(l - x, x - r, 0), Math.max(t - y, y - b, 0)) / fade));
    const image = ctx.createImageData(cols, rows);
    const data = image.data;
    const sx = seed & 0xfff, sy = (seed >> 12) & 0xfff;
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        const cx = x + 0.5, cy = y + 0.5;
        const open = smoothstep(0, 1, clearance(cx, cy));
        let c = ground;
        if (open > 0) {
          const px = (cx + sx) / 160, py = (cy + sy) / 160;
          const bias = (0.45 * (cx / cols) + 0.5 * (1 - cy / rows) - 0.55) * 0.4;
          const density = fbm(px * 2.8 - STILL * 0.018, py * 3.8) * 0.6 + fbm(px * 5.5 - STILL * 0.07, py * 7 + 4.7) * 0.4 + bias;
          const sprinkle = smoothstep(0.35, 0.9, density) * open;
          if (sprinkle > 0) {
            const tone = night ? 0.48 * Math.pow(sprinkle, 0.75) : 1 - 0.72 * sprinkle;
            const level = clamp(tone, 0, 1) * 3;
            const below = Math.min(Math.floor(level + 1e-4), 3);
            c = ramp[level - below > bayer(x, y) && below < 3 ? below + 1 : below];
          }
        }
        const o = (y * cols + x) * 4;
        data[o] = c[0];
        data[o + 1] = c[1];
        data[o + 2] = c[2];
        data[o + 3] = 255;
      }
    }

    const bandsFor = (scale) => {
      const spans = zones.map((z) => [z[1], z[3]]).sort((a, b) => a[0] - b[0]);
      const out = [];
      let edge = 0;
      for (const [top, bottom] of spans) {
        if (top > edge) out.push([edge, top]);
        edge = Math.max(edge, bottom);
      }
      if (rows > edge) out.push([edge, rows]);
      return out.filter(([top, bottom]) => Math.floor(bottom) - SH * scale - 1 - Math.max(2, Math.ceil(top) + 1) >= 2);
    };
    const small = bandsFor(1), large = bandsFor(2);
    const count = width >= 700 ? FLOCK.length : 3;
    const phase = ((seed >>> 8) & 0xffff) / 65536;
    FLOCK.slice(0, count).forEach((bird, index) => {
      const bands = bird.scale > 1 ? large : small;
      const band = bands[index % Math.max(1, bands.length)];
      if (!band) return;
      const span = SW * bird.scale;
      let spread = 0.16 + (index % 3) * 0.27 + Math.floor(index / 3) * 0.135;
      spread = 0.1 + 0.8 * ((spread + phase) % 1);
      const bx = Math.floor(cols * spread - span / 2);
      const wing = index & 1;
      const low = Math.max(2, Math.ceil(band[0]) + 1);
      const high = Math.floor(band[1]) - SH * bird.scale - 1;
      const drift = Math.sin(STILL * 0.9 + index * 1.9 + phase * Math.PI * 2) * 1.2;
      const by = Math.round(clamp(low + bird.lane * (high - low) + drift, low, high));
      for (let y = 0; y < SH * bird.scale; y++) {
        for (let x = 0; x < SW * bird.scale; x++) {
          const gx = bx + x, gy = by + y;
          if (gx < 0 || gy < 0 || gx >= cols || gy >= rows) continue;
          const row = Math.floor(y / bird.scale), col = Math.floor(x / bird.scale);
          const c = BEAK[wing][row][col] ? FLAME : INK[wing][row][col] ? ink : null;
          if (!c) continue;
          const o = (gy * cols + gx) * 4;
          data[o] = c[0];
          data[o + 1] = c[1];
          data[o + 2] = c[2];
        }
      }
    });
    ctx.putImageData(image, 0, 0);
  };

  window.hfSky = () => {
    document.querySelectorAll(".hf-body[data-sky]").forEach((body) => paintSky(body, Number(body.dataset.sky)));
  };
})();
