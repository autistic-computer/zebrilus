(() => {
  const frozen = new URLSearchParams(location.search).has("still");
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const UP = ["..W........", "..WW.####..", "...WW##E#BB", "#..WWW####.", ".#########.", "..#######..", "..........."];
  const DOWN = ["...........", ".....####..", "....###E#BB", "#..#######.", ".##WWW####.", "..#WWW###..", "...WW......"];
  const SW = 11, SH = 7, CELL = 4, PAD = 10, FADE = 72, MAX_ZONES = 8;
  const FLOCK = [{ lane: 0.2, speed: 7 }, { lane: 0.75, speed: 5.5 }, { lane: 0.45, speed: 8.5 }];
  const RAMP = [0.12, 0.24, 0.4];

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
    y = (y + Math.imul(x, 1664525)) >>> 0;
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
  const inkBox = (el) => {
    if (!el.hasAttribute("data-text")) return el.getBoundingClientRect();
    range.selectNodeContents(el);
    return range.getBoundingClientRect();
  };

  const skies = [];
  for (const frame of document.querySelectorAll("[data-sky]")) {
    const canvas = frame.querySelector("canvas");
    const ctx = canvas.getContext("2d");
    const sky = { time: frozen ? 40 : 0 };
    let cols = 0, rows = 0, image = null, open, threshold, bias, flock = [], ground, ink, ramp, night = true;

    sky.measure = () => {
      const box = frame.getBoundingClientRect();
      cols = Math.ceil(box.width / CELL);
      rows = Math.ceil(box.height / CELL);
      if (!cols || !rows) return;
      canvas.width = cols;
      canvas.height = rows;
      image = ctx.createImageData(cols, rows);
      const zones = [...frame.querySelectorAll("[data-zone]")].slice(0, MAX_ZONES).map(inkBox).map((r) => [r.left - box.left - PAD, r.top - box.top - PAD, r.right - box.left + PAD, r.bottom - box.top + PAD].map((v) => v / CELL));
      const fade = FADE / CELL;
      const clearance = (x, y) => Math.min(...zones.map(([l, t, r, b]) => Math.hypot(Math.max(l - x, x - r, 0), Math.max(t - y, y - b, 0)) / fade));
      const n = cols * rows;
      open = new Float32Array(n);
      threshold = new Float32Array(n);
      bias = new Float32Array(n);
      for (let y = 0; y < rows; y++) {
        for (let x = 0; x < cols; x++) {
          const i = y * cols + x, ax = x + 0.5, ay = y + 0.5;
          open[i] = smoothstep(0, 1, clearance(ax, ay));
          threshold[i] = bayer(x, y);
          bias[i] = (0.45 * (ax / cols) + 0.5 * (1 - ay / rows) - 0.55) * 0.4;
        }
      }
      const bands = [];
      let edge = 0;
      for (const [top, bottom] of zones.map((z) => [z[1], z[3]]).sort((a, b) => a[0] - b[0])) {
        if (top > edge) bands.push([edge, top]);
        edge = Math.max(edge, bottom);
      }
      if (rows > edge) bands.push([edge, rows]);
      const usable = bands.filter((band) => Math.floor(band[1]) - SH - 1 - Math.max(2, Math.ceil(band[0]) + 1) >= 2);
      flock = FLOCK.map((bird, i) => ({
        ...bird,
        band: usable[i % Math.max(1, usable.length)] ?? [0, rows],
        x: frozen ? cols * (0.16 + i * 0.27) - SW / 2 : cols * (0.08 + i * 0.3),
        phase: i * 1.9,
        wingbeat: frozen ? i : i * 0.7,
      }));
      paint();
    };

    sky.colors = () => {
      const root = getComputedStyle(document.documentElement);
      ground = rgb(root.getPropertyValue("--penna-bg").trim());
      ink = rgb(getComputedStyle(frame).color);
      night = document.documentElement.dataset.pennaMode !== "light";
      const accent = rgb(root.getPropertyValue("--penna-accent").trim());
      const tint = RAMP.map((t) => mix(ground, accent, t));
      ramp = night ? [ground, ...tint] : [...tint.reverse(), ground];
      frame.style.background = `rgb(${ground})`;
      paint();
    };

    const birdY = (b) => {
      const [top, bottom] = b.band;
      const lo = Math.max(2, Math.ceil(top) + 1), hi = Math.floor(bottom) - SH - 1;
      return Math.round(clamp(lo + b.lane * (hi - lo) + Math.sin(sky.time * 0.9 + b.phase) * 1.2, lo, Math.max(lo, hi)));
    };

    const paint = () => {
      if (!image || !ramp) return;
      const data = image.data, t = sky.time;
      for (let y = 0; y < rows; y++) {
        for (let x = 0; x < cols; x++) {
          const i = y * cols + x;
          let c = ground;
          if (open[i] > 0) {
            const px = (x + 0.5) / 160, py = (y + 0.5) / 160;
            const density = fbm(px * 2.8 - t * 0.018, py * 3.8) * 0.6 + fbm(px * 5.5 - t * 0.07, py * 7 + 4.7) * 0.4 + bias[i];
            const sprinkle = smoothstep(0.35, 0.9, density) * open[i];
            const tone = night ? 0.48 * Math.pow(sprinkle, 0.75) : 1 - 0.72 * sprinkle;
            const level = clamp(tone, 0, 1) * 3;
            const below = Math.min(Math.floor(level + 1e-4), 3);
            c = ramp[level - below > threshold[i] && below < 3 ? below + 1 : below];
          }
          const o = i * 4;
          data[o] = c[0];
          data[o + 1] = c[1];
          data[o + 2] = c[2];
          data[o + 3] = 255;
        }
      }
      for (const b of flock) {
        const bx = Math.floor(b.x), by = birdY(b), f = Math.floor(b.wingbeat) & 1;
        for (let sy = 0; sy < SH; sy++) {
          for (let sx = 0; sx < SW; sx++) {
            const x = bx + sx, y = by + sy;
            if (x < 0 || y < 0 || x >= cols || y >= rows) continue;
            const c = BEAK[f][sy][sx] ? FLAME : INK[f][sy][sx] ? ink : null;
            if (!c) continue;
            const o = (y * cols + x) * 4;
            data[o] = c[0];
            data[o + 1] = c[1];
            data[o + 2] = c[2];
          }
        }
      }
      ctx.putImageData(image, 0, 0);
    };

    sky.step = (dt) => {
      sky.time += dt;
      for (const b of flock) {
        b.x += b.speed * dt;
        b.wingbeat += dt * 2.2;
        if (b.x > cols) b.x = -SW - Math.random() * 40;
      }
      paint();
    };
    skies.push(sky);
  }

  const settle = () => {
    for (const sky of skies) sky.colors();
    for (const sky of skies) sky.measure();
  };
  document.getElementById("penna-tokens")?.addEventListener("load", () => requestAnimationFrame(settle));
  addEventListener("load", () => requestAnimationFrame(settle));
  document.fonts?.ready.then(() => requestAnimationFrame(settle));
  document.addEventListener("penna-theme", () => setTimeout(settle, 200));
  settle();

  if (frozen) return;
  let last = 0;
  const tick = (now) => {
    if (!last) last = now;
    const dt = (now - last) / 1000;
    if (dt >= 1 / 31) {
      last = now;
      for (const sky of skies) sky.step(Math.min(dt, 0.1));
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
})();
