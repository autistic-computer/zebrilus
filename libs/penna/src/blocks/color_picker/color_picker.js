(() => {
  const FULL = 24;
  const CORNER = 0.45;
  const HUE_STOPS = 12;

  const parse = (text) => {
    const match = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(text.trim());
    if (!match) return null;
    const digits = match[1].length === 3 ? [...match[1]].map((digit) => digit + digit).join("") : match[1];
    return [0, 2, 4].map((at) => parseInt(digits.slice(at, at + 2), 16) / 255);
  };

  const hex = ([r, g, b]) => `#${[r, g, b].map((channel) => Math.round(Math.min(Math.max(channel, 0), 1) * 255).toString(16).padStart(2, "0")).join("")}`;

  const css = ([r, g, b]) => `rgb(${[r, g, b].map((channel) => +(channel * 255).toFixed(2)).join(" ")})`;

  const toHsv = ([r, g, b]) => {
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const delta = max - min;
    let hue = 0;
    if (delta > 0) {
      if (max === r) hue = ((g - b) / delta + 6) % 6;
      else if (max === g) hue = (b - r) / delta + 2;
      else hue = (r - g) / delta + 4;
    }
    return { hue: hue / 6, saturation: max > 0 ? delta / max : 0, value: max };
  };

  const toRgb = ({ hue, saturation, value }) => {
    const at = (n) => {
      const k = (n + hue * 6) % 6;
      return value - value * saturation * Math.max(0, Math.min(k, 4 - k, 1));
    };
    return [at(5), at(3), at(1)];
  };

  const ramp = (colors) => colors.map((color, index) => `${css(color)} ${(index / (colors.length - 1)) * 100}%`).join(", ");

  const swatchRadius = (element) => {
    const radius = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-radius")) || 0;
    const size = Math.min(element.offsetWidth, element.offsetHeight);
    if (radius > FULL) return size / 2;
    const cap = size * CORNER;
    return cap > 0 ? cap * Math.tanh(radius / cap) : 0;
  };

  const fieldRadius = (element) => {
    const radius = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--penna-radius")) || 0;
    return Math.min(radius, Math.min(element.offsetWidth, element.offsetHeight) / 2);
  };

  const MARGIN = 8;

  const fit = (open) => {
    const section = open.closest("section[data-section]");
    if (!section) return;
    open.style.removeProperty("translate");
    const limits = section.getBoundingClientRect();
    const box = open.getBoundingClientRect();
    const gap = parseFloat(getComputedStyle(open).getPropertyValue("--penna-color-picker-gap")) || 0;
    let top = box.top;
    if (box.bottom > limits.bottom) top -= box.bottom - limits.bottom + MARGIN;
    if (top < limits.top) top = limits.top + MARGIN + gap;
    if (top !== box.top) open.style.translate = `0 ${Math.round(top - box.top)}px`;
  };

  const pickers = new Set();
  const closers = new Set();

  const edge = () => {
    const probe = document.createElement("i");
    probe.dataset.penna = "card";
    probe.hidden = true;
    document.body.append(probe);
    const width = parseFloat(getComputedStyle(probe).getPropertyValue("--penna-bw")) || 0;
    probe.remove();
    document.documentElement.style.setProperty("--penna-color-picker-edge", `${width}px`);
  };

  function panel(picker) {
    const label = picker.dataset.label;
    const author = picker.querySelector(":scope > .penna-button").dataset.id;
    const element = document.createElement("div");
    element.className = "penna-color-picker-panel";
    element.dataset.penna = "popover";
    element.dataset.ground = "";
    element.tabIndex = -1;
    element.innerHTML = `${label ? `<div class="penna-color-picker-title" data-penna-text></div>` : ""}<div class="penna-color-picker-field" role="group" aria-label="Saturation and brightness" data-id="${author}-field"><div class="penna-color-picker-handle"></div></div><div class="penna-color-picker-hue" role="slider" tabindex="0" aria-label="Hue" aria-valuemin="0" aria-valuemax="360" data-id="${author}-hue"><div class="penna-color-picker-rail"><div class="penna-color-picker-bar"></div><div class="penna-color-picker-knob"></div></div></div><div class="penna-color-picker-hex"><label data-penna="field" data-id="${author}-hex"><input type="text" spellcheck="false" autocomplete="off" placeholder="#rrggbb" aria-label="Hex"></label><button class="penna-button" data-penna="icon-button" data-shape="icon" data-flat aria-label="Clear the override" title="Clear the override" data-id="${author}-clear"><i data-icon="undo-2"></i></button></div>`;
    if (label) element.querySelector(".penna-color-picker-title").textContent = label;
    return element;
  }

  function setup(picker) {
    const trigger = picker.querySelector(":scope > .penna-button");
    const swatch = trigger.querySelector(".penna-color-picker-swatch");
    const state = {
      color: parse(picker.dataset.value ?? "") ?? null,
      inherited: parse(picker.dataset.inherited ?? "") ?? [0, 0, 0],
      picked: null,
      panel: null,
      previous: null,
    };

    const shown = () => state.color ?? state.inherited;
    const hsv = () => {
      const color = shown();
      if (state.picked && hex(toRgb(state.picked)) === hex(color)) return state.picked;
      return toHsv(color);
    };

    const paint = () => {
      picker.style.setProperty("--penna-color-picker-shown", css(shown()));
      const open = state.panel;
      if (!open) return;
      const current = hsv();
      const pure = toRgb({ ...current, saturation: 1, value: 1 });
      const stops = Array.from({ length: HUE_STOPS + 1 }, (_, stop) => toRgb({ hue: stop / HUE_STOPS, saturation: 1, value: 1 }));
      const degrees = Math.round(current.hue * 360);
      open.style.setProperty("--penna-color-picker-white", css(toRgb({ hue: 0, saturation: 0, value: 1 })));
      open.style.setProperty("--penna-color-picker-black", css(toRgb({ hue: 0, saturation: 0, value: 0 })));
      open.style.setProperty("--penna-color-picker-shade", ramp([toRgb({ hue: 0, saturation: 0, value: 1 }), pure]));
      open.style.setProperty("--penna-color-picker-picked", css(toRgb(current)));
      open.style.setProperty("--penna-color-picker-hues", ramp(stops));
      open.style.setProperty("--penna-color-picker-s", current.saturation);
      open.style.setProperty("--penna-color-picker-v", current.value);
      open.style.setProperty("--penna-color-picker-hue", degrees / 360);
      open.querySelector(".penna-color-picker-hue").setAttribute("aria-valuenow", degrees);
      const field = open.querySelector(".penna-color-picker-field");
      field.style.setProperty("--penna-color-picker-field-r", `${fieldRadius(field)}px`);
    };

    const setColor = (color) => {
      state.color = color;
      const input = state.panel?.querySelector("input");
      if (input && document.activeElement !== input) input.value = color ? hex(color) : "";
      picker.dispatchEvent(new CustomEvent("change", { detail: color ? hex(color) : null }));
      paint();
    };

    const pick = (next) => {
      state.picked = next;
      const color = toRgb(next);
      if (state.color && hex(state.color) === hex(color)) {
        paint();
        return;
      }
      setColor(color);
    };

    const commit = (input) => {
      const text = input.value.trim();
      if (!text) {
        setColor(null);
        return;
      }
      const color = parse(text);
      if (color) setColor(color);
    };

    const close = () => {
      if (!state.panel) return;
      const focused = state.panel.contains(document.activeElement);
      state.panel.remove();
      state.panel = null;
      const previous = state.previous;
      state.previous = null;
      if (focused) (previous?.isConnected ? previous : trigger).focus({ preventScroll: true });
      trigger.setAttribute("aria-expanded", "false");
      trigger.dataset.state = (trigger.dataset.state ?? "").replace(/\bcurrent\b/, "").trim();
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", escape);
    };

    const escape = (event) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      close();
    };

    const outside = (event) => {
      if (!picker.contains(event.target)) close();
    };

    const wire = (open) => {
      const field = open.querySelector(".penna-color-picker-field");
      const fieldAt = (event) => {
        const box = field.getBoundingClientRect();
        const along = (offset, length) => Math.min(Math.max(offset / Math.max(length, 1), 0), 1);
        pick({ ...hsv(), saturation: along(Math.floor(event.clientX) - box.left, box.width), value: 1 - along(Math.floor(event.clientY) - box.top, box.height) });
      };
      field.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        state.picked = hsv();
        field.setPointerCapture(event.pointerId);
        fieldAt(event);
      });
      field.addEventListener("pointermove", (event) => field.hasPointerCapture(event.pointerId) && fieldAt(event));

      const hue = open.querySelector(".penna-color-picker-hue");
      const rail = hue.querySelector(".penna-color-picker-rail");
      const setHue = (degrees) => pick({ ...hsv(), hue: (Math.min(Math.max(Math.round(degrees), 0), 360) / 360) % 1 });
      const hueAt = (event) => {
        const box = rail.getBoundingClientRect();
        setHue(((Math.floor(event.clientX) - box.left) / Math.max(box.width, 1)) * 360);
      };
      hue.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        hue.focus({ preventScroll: true });
        hue.setPointerCapture(event.pointerId);
        hueAt(event);
      });
      hue.addEventListener("pointermove", (event) => hue.hasPointerCapture(event.pointerId) && hueAt(event));
      hue.addEventListener("keydown", (event) => {
        const steps = event.shiftKey ? 10 : 1;
        const now = Math.round(hsv().hue * 360);
        const next = { ArrowLeft: now - steps, ArrowDown: now - steps, ArrowRight: now + steps, ArrowUp: now + steps, Home: 0, End: 360 }[event.key];
        if (next === undefined) return;
        event.preventDefault();
        setHue(next);
      });

      const input = open.querySelector("input");
      input.value = state.color ? hex(state.color) : "";
      input.addEventListener("keydown", (event) => {
        if (event.key === "Enter") commit(input);
      });
      input.addEventListener("blur", () => commit(input));

      open.querySelector(".penna-color-picker-hex > .penna-button").addEventListener("click", () => {
        state.picked = null;
        input.value = "";
        setColor(null);
      });
    };

    const openPanel = (focus) => {
      if (state.panel || trigger.disabled) return;
      closers.forEach((other) => other !== close && other());
      state.panel = panel(picker);
      picker.append(state.panel);
      window.pennaKit?.ceilText(state.panel);
      wire(state.panel);
      trigger.setAttribute("aria-expanded", "true");
      trigger.dataset.state = `${trigger.dataset.state ?? ""} current`.trim();
      paint();
      fit(state.panel);
      document.addEventListener("pointerdown", outside, true);
      document.addEventListener("keydown", escape);
      if (!focus) return;
      const active = document.activeElement;
      state.previous = active && active !== document.body ? active : trigger;
      state.panel.focus({ preventScroll: true });
    };

    trigger.setAttribute("aria-haspopup", "dialog");
    trigger.setAttribute("aria-expanded", "false");
    trigger.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || trigger.disabled) return;
      if (state.panel) close();
      else openPanel(true);
    });
    trigger.addEventListener("mousedown", (event) => {
      if (state.panel?.contains(document.activeElement)) event.preventDefault();
    });
    trigger.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      if (state.panel) close();
      else openPanel(true);
    });
    closers.add(close);

    const settle = () => {
      if (swatch) swatch.style.setProperty("--penna-swatch-r", `${swatchRadius(swatch)}px`);
      paint();
    };
    pickers.add(settle);
    if (picker.hasAttribute("data-open")) openPanel();
    settle();
  }

  const settleAll = () => {
    edge();
    pickers.forEach((settle) => settle());
  };

  document.querySelectorAll(".penna-color-picker").forEach(setup);
  settleAll();
  document.fonts.ready.then(settleAll);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settleAll(), true);
})();
