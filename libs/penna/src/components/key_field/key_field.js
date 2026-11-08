(() => {
  const FIELDS = ".penna-key-field";
  const PROMPT = "Press a key";
  const PLACEHOLDER = "None";
  const MODIFIERS = new Set(["Shift", "Control", "Alt", "AltGraph", "Meta", "OS", "Fn", "FnLock", "Hyper", "Super", "CapsLock"]);
  const IGNORED = new Set(["Dead", "Unidentified", "Process"]);
  const NAMED = {
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Enter: "enter",
    Escape: "escape",
    " ": "space",
    Backspace: "backspace",
    Delete: "delete",
    Tab: "tab",
    PageUp: "pageup",
    PageDown: "pagedown",
    Home: "home",
    End: "end",
    Insert: "insert",
  };

  let recording = null;
  const pinned = new WeakMap();

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
    const saved = field.getAttribute("data-state");
    field.removeAttribute("data-state");
    const style = getComputedStyle(field);
    const fillValue = style.getPropertyValue("--penna-fill").trim();
    const base = rgba(style.getPropertyValue("--penna-bg"));
    if (saved === null) field.removeAttribute("data-state");
    else field.setAttribute("data-state", saved);
    if (fillValue && !field.matches(":hover")) field.style.setProperty("--key-field-rest-fill", fillValue);
    const fill = rgba(fillValue);
    const mix = (channel) => Math.round(base[channel] * (1 - fill.a) + fill[channel] * fill.a);
    field.style.setProperty("--key-field-ground", `rgb(${mix("r")} ${mix("g")} ${mix("b")})`);
  }

  function disabled(field) {
    return field.getAttribute("aria-disabled") === "true";
  }

  function capSize(field) {
    return field.dataset.size === "xsmall" || field.dataset.size === "small" ? "small" : null;
  }

  function text(value) {
    const run = document.createElement("span");
    run.className = "penna-key-field-text";
    run.setAttribute("data-penna-text", "");
    run.textContent = value;
    return run;
  }

  function paint(field) {
    const key = field.dataset.key || "";
    const kept = pinned.get(field) ?? new Set();
    const live = !disabled(field) && (recording === field || kept.has("recording"));
    const states = [];
    if (disabled(field)) states.push("disabled");
    else if (live || document.activeElement === field || kept.has("focused")) states.push("focused");
    if (live) states.push("recording");
    if (states.length) field.dataset.state = states.join(" ");
    else field.removeAttribute("data-state");
    field.setAttribute("role", "button");
    if (disabled(field)) field.removeAttribute("tabindex");
    else field.tabIndex = 0;
    if (live) {
      field.replaceChildren(text(PROMPT));
    } else if (key) {
      const cap = document.createElement("kbd");
      cap.className = "penna-kbd";
      cap.dataset.penna = "keycap";
      cap.dataset.kbd = key;
      const size = capSize(field);
      if (size) cap.dataset.size = size;
      field.replaceChildren(cap);
      globalThis.pennaKbd?.fill(field);
    } else {
      field.replaceChildren(text(field.dataset.placeholder || PLACEHOLDER));
    }
    field.setAttribute("aria-label", key ? globalThis.pennaKbd?.text(key) ?? key : field.dataset.placeholder || PLACEHOLDER);
    ground(field);
    globalThis.pennaKit?.ceilText(field);
  }

  function spell(event) {
    if (MODIFIERS.has(event.key) || IGNORED.has(event.key)) return null;
    const held = event.ctrlKey || event.altKey || event.metaKey;
    let key = NAMED[event.key];
    let shift = event.shiftKey;
    if (!key && /^F\d{1,2}$/.test(event.key)) key = event.key.toLowerCase();
    if (!key && held && /^Key[A-Z]$/.test(event.code)) key = event.code.slice(3).toLowerCase();
    if (!key && held && /^Digit\d$/.test(event.code)) key = event.code.slice(5);
    if (!key && [...event.key].length === 1) {
      key = event.key.toLowerCase();
      if (shift && !held && !/[\p{L}\p{N}\s]/u.test(event.key)) shift = false;
    }
    if (!key) return null;
    return [event.ctrlKey && "ctrl", event.altKey && "alt", event.metaKey && "cmd", shift && "shift", key].filter(Boolean).join("-");
  }

  function stop(field) {
    if (recording !== field) return;
    recording = null;
    document.removeEventListener("keydown", capture, true);
    paint(field);
  }

  function record(field) {
    if (disabled(field)) return;
    if (recording && recording !== field) stop(recording);
    if (document.activeElement !== field) field.focus({ preventScroll: true });
    if (recording === field) return;
    recording = field;
    document.addEventListener("keydown", capture, true);
    paint(field);
  }

  function capture(event) {
    const field = recording;
    if (!field) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (event.repeat) return;
    if (event.key === "Escape" && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey) {
      stop(field);
      return;
    }
    const key = spell(event);
    if (!key) return;
    field.dataset.key = key;
    stop(field);
    field.dispatchEvent(new CustomEvent("change", { bubbles: true, detail: { key } }));
  }

  function bind(field) {
    if (pinned.has(field)) return;
    pinned.set(field, new Set((field.dataset.state || "").split(/\s+/).filter(Boolean)));
    field.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || disabled(field)) return;
      event.preventDefault();
      record(field);
    });
    field.addEventListener("keydown", (event) => {
      if (recording === field || disabled(field)) return;
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        record(field);
      }
    });
    field.addEventListener("focus", () => paint(field));
    field.addEventListener("blur", () => {
      if (recording === field) stop(field);
      else paint(field);
    });
  }

  function init(root = document) {
    root.querySelectorAll(FIELDS).forEach((field) => {
      bind(field);
      const kept = pinned.get(field);
      if (kept.delete("recording")) record(field);
      else paint(field);
    });
  }

  init();
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(() => document.querySelectorAll(FIELDS).forEach(ground)), true);
  globalThis.pennaKeyField = { init, paint, record, spell };
})();
