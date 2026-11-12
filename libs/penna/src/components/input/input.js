(() => {
  const FIELDS = ".penna-input, .penna-number";

  const probe = document.createElement("i");
  probe.style.display = "none";

  function rgba(value) {
    if (!probe.isConnected) document.body.append(probe);
    probe.style.color = "";
    probe.style.color = value.trim() || "transparent";
    const parts = getComputedStyle(probe).color.match(/[\d.]+/g)?.map(Number) ?? [0, 0, 0, 0];
    const [r, g, b, a = 1] = parts.length > 4 ? parts.slice(1) : parts;
    return { r, g, b, a };
  }

  function ground(field) {
    const style = getComputedStyle(field);
    const base = rgba(style.getPropertyValue("--penna-bg"));
    const fill = field.hasAttribute("data-bare") ? { r: 0, g: 0, b: 0, a: 0 } : rgba(style.getPropertyValue("--penna-fill"));
    const mix = (channel) => Math.round(base[channel] * (1 - fill.a) + fill[channel] * fill.a);
    field.style.setProperty("--input-ground", `rgb(${mix("r")} ${mix("g")} ${mix("b")})`);
  }

  function fills(field) {
    const saved = field.getAttribute("data-state");
    const read = (state) => {
      if (state) field.setAttribute("data-state", state);
      else field.removeAttribute("data-state");
      return getComputedStyle(field).getPropertyValue("--penna-fill").trim();
    };
    const hover = read("hover");
    if (saved === null) field.removeAttribute("data-state");
    else field.setAttribute("data-state", saved);
    if (hover) field.style.setProperty("--input-hover-fill", hover);
  }

  function grounds(root = document) {
    root.querySelectorAll(FIELDS).forEach((field) => {
      fills(field);
      ground(field);
    });
    root.querySelectorAll(".penna-rule").forEach((rule) => {
      rule.toggleAttribute("data-edged", rgba(getComputedStyle(rule).getPropertyValue("--penna-edge")).a > 0);
    });
  }

  function focused(field, on) {
    const states = new Set((field.dataset.state ?? "").split(/\s+/).filter(Boolean));
    if (on) states.add("focused");
    else states.delete("focused");
    if (states.size) field.dataset.state = [...states].join(" ");
    else delete field.dataset.state;
  }

  const measure = document.createElement("canvas").getContext("2d");

  function textWidth(input, text) {
    const style = getComputedStyle(input);
    measure.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    if (input.type === "password") text = "•".repeat([...text].length);
    return measure.measureText(text).width;
  }

  function paintSelection(input) {
    const field = input.parentElement;
    let quad = field.querySelector(":scope > .penna-input-selection");
    const start = input.selectionStart ?? 0;
    const end = input.selectionEnd ?? 0;
    if (input.localName !== "input" || start === end) {
      quad?.remove();
      return;
    }
    if (!quad) {
      quad = document.createElement("span");
      quad.className = "penna-input-selection";
      field.insertBefore(quad, input);
    }
    const value = input.value;
    const full = textWidth(input, value);
    const align = getComputedStyle(input).textAlign;
    const room = input.clientWidth;
    const lead = align === "center" ? (room - full) / 2 : align === "right" || align === "end" ? room - full : 0;
    const left = input.offsetLeft + lead + textWidth(input, value.slice(0, start)) - input.scrollLeft;
    quad.style.left = `${left}px`;
    quad.style.top = `${input.offsetTop}px`;
    quad.style.width = `${textWidth(input, value.slice(start, end))}px`;
  }

  function paintCaret(input) {
    const field = input.parentElement;
    let caret = field.querySelector(":scope > .penna-input-caret");
    const start = input.selectionStart ?? 0;
    if (document.activeElement !== input || input.disabled || start !== input.selectionEnd) {
      caret?.remove();
      return;
    }
    if (!caret) {
      caret = document.createElement("span");
      caret.className = "penna-input-caret";
      field.insertBefore(caret, input.nextSibling);
    }
    const value = input.value;
    const full = textWidth(input, value);
    const align = getComputedStyle(input).textAlign;
    const room = input.clientWidth;
    const lead = align === "center" ? (room - full) / 2 : align === "right" || align === "end" ? room - full : 0;
    caret.style.left = `${input.offsetLeft + Math.max(0, lead) + textWidth(input, value.slice(0, start)) - input.scrollLeft}px`;
    caret.style.top = `${input.offsetTop}px`;
    caret.style.animation = "none";
    void caret.offsetWidth;
    caret.style.animation = "";
  }

  function grow(area) {
    const min = Number(area.dataset.minRows ?? 1);
    const max = Number(area.dataset.maxRows ?? min);
    area.style.setProperty("--rows", min);
    const saved = area.scrollTop;
    area.style.height = "0px";
    const rows = Math.round(area.scrollHeight / parseFloat(getComputedStyle(area).lineHeight));
    area.style.removeProperty("height");
    area.style.setProperty("--rows", Math.min(max, Math.max(min, area.value ? rows : min)));
    area.scrollTop = saved;
  }

  function digits(text) {
    return text.split(".")[1]?.length ?? 0;
  }

  function step(field, direction, from) {
    const input = field.querySelector("input");
    if (!input || input.disabled) return;
    const size = Number(field.dataset.step ?? 1);
    const min = field.dataset.min === undefined ? null : Number(field.dataset.min);
    const max = field.dataset.max === undefined ? null : Number(field.dataset.max);
    const current = (from ?? input.value).trim();
    const parsed = current === "" || Number.isNaN(Number(current)) ? null : Number(current);
    let value = (parsed ?? 0) + direction * size;
    let places = Math.max(digits(current), digits(String(size)));
    if (min !== null && value < min) {
      value = min;
      places = Math.max(places, digits(String(min)));
    }
    if (max !== null && value > max) {
      value = max;
      places = Math.max(places, digits(String(max)));
    }
    if (from === undefined && parsed !== null && (direction > 0 ? value <= parsed : value >= parsed)) return;
    if (input.value === value.toFixed(places)) return;
    input.value = value.toFixed(places);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function offsetAt(input, x) {
    const value = input.value;
    const full = textWidth(input, value);
    const align = getComputedStyle(input).textAlign;
    const room = input.clientWidth;
    const lead = align === "center" ? (room - full) / 2 : align === "right" || align === "end" ? room - full : 0;
    const local = x - input.getBoundingClientRect().left - input.clientLeft - Math.max(0, lead) + input.scrollLeft;
    let best = 0;
    for (let index = 1; index <= value.length; index++) {
      if (Math.abs(textWidth(input, value.slice(0, index)) - local) < Math.abs(textWidth(input, value.slice(0, best)) - local)) best = index;
    }
    return best;
  }

  function scrub(field, input) {
    input.addEventListener("mousedown", (event) => {
      if (event.button !== 0 || input.disabled || document.activeElement === input || event.sourceCapabilities?.firesTouchEvents) return;
      event.preventDefault();
      input.focus();
      const start = event.clientX;
      const from = input.value;
      let moved = false;
      const move = (event) => {
        const dx = event.clientX - start;
        if (!moved && Math.abs(dx) < 3) return;
        moved = true;
        step(field, Math.trunc(dx / 4), from);
      };
      const up = (event) => {
        document.removeEventListener("mousemove", move);
        if (moved) return;
        const at = offsetAt(input, event.clientX);
        input.setSelectionRange(at, at);
        paintCaret(input);
      };
      document.addEventListener("mousemove", move);
      document.addEventListener("mouseup", up, { once: true });
    });
  }

  function wire(root = document) {
    root.querySelectorAll(".penna-input > input[data-selection]").forEach((input) => {
      const [start, end] = input.dataset.selection.split(",").map(Number);
      input.setSelectionRange(start, end);
      paintSelection(input);
    });
    root.querySelectorAll(".penna-input > :is(input, textarea)").forEach((input) => {
      if (input.pennaWired) return;
      input.pennaWired = true;
      const field = input.closest(".penna-number") ?? input.closest(".penna-input");
      input.addEventListener("focusin", () => focused(field, true));
      input.addEventListener("focusout", () => document.activeElement === input || focused(field, false));
      if (document.activeElement === input) focused(field, true);
      let kept = input.dataset.selection ? input.dataset.selection.split(",").map(Number) : [input.value.length, input.value.length];
      let pointer = false;
      input.addEventListener("mousedown", () => {
        pointer = true;
      });
      input.addEventListener("focus", () => {
        if (!pointer) {
          const end = input.value.length;
          input.setSelectionRange(Math.min(kept[0], end), Math.min(kept[1], end));
        }
        pointer = false;
      });
      input.addEventListener("blur", () => {
        kept = [input.selectionStart ?? 0, input.selectionEnd ?? 0];
        paintSelection(input);
      });
      input.spellcheck = false;
      if (input.localName === "input") {
        const caret = () => paintCaret(input);
        for (const type of ["focus", "blur", "input", "select", "selectionchange", "scroll", "mouseup"]) input.addEventListener(type, caret);
        input.addEventListener("keydown", () => requestAnimationFrame(caret));
        input.addEventListener("mousemove", (event) => event.buttons && caret());
      }
    });
    root.querySelectorAll(".penna-input > textarea").forEach((area) => {
      grow(area);
      area.addEventListener("input", () => grow(area));
    });
    root.querySelectorAll(".penna-input-clear").forEach((button) => {
      if (button.pennaWired) return;
      button.pennaWired = true;
      const input = button.closest(".penna-input").querySelector("input");
      const slot = button.parentElement;
      const sync = () => {
        if (!input.value) button.remove();
        else if (!button.isConnected) slot.append(button);
      };
      sync();
      input.addEventListener("input", sync);
      button.addEventListener("mousedown", (event) => event.preventDefault());
      button.addEventListener("click", (event) => {
        event.preventDefault();
        input.value = "";
        input.dispatchEvent(new Event("input", { bubbles: true }));
        input.focus();
      });
    });
    root.querySelectorAll(".penna-number").forEach((field) => {
      if (field.pennaWired) return;
      field.pennaWired = true;
      const input = field.querySelector("input");
      input.addEventListener("beforeinput", (event) => {
        if (!event.data) return;
        const next = input.value.slice(0, input.selectionStart) + event.data + input.value.slice(input.selectionEnd);
        if (!/^[-+]?\d*\.?\d*$/.test(next)) event.preventDefault();
      });
      scrub(field, input);
      input.addEventListener("keydown", (event) => {
        if (event.key === "ArrowUp" || event.key === "ArrowDown") {
          event.preventDefault();
          step(field, event.key === "ArrowUp" ? 1 : -1);
        }
      });
      field.querySelectorAll("[data-step-by]").forEach((button) => {
        button.addEventListener("mousedown", (event) => event.preventDefault());
        button.addEventListener("click", () => {
          if (input.disabled) return;
          input.focus();
          step(field, Number(button.dataset.stepBy));
        });
      });
    });
    root.querySelectorAll("input[autofocus]").forEach((input) => {
      input.focus();
      focused(input.closest(".penna-number") ?? input.closest(".penna-input"), true);
      input.setSelectionRange(input.value.length, input.value.length);
      paintCaret(input);
    });
  }

  function settle() {
    grounds();
    document.querySelectorAll(".penna-input > input").forEach((input) => {
      if (document.activeElement !== input) paintSelection(input);
      paintCaret(input);
    });
  }

  wire();
  settle();
  globalThis.pennaInput = {
    wire(root) {
      grounds(root);
      wire(root);
    },
  };
  document.fonts.ready.then(settle);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(settle), true);
})();
