(() => {
  const mac = navigator.userAgent.includes("Mac");
  const MODIFIERS = ["ctrl", "alt", "shift", "cmd"];
  const MAC_NAMES = { ctrl: "⌃", alt: "⌥", shift: "⇧", cmd: "⌘", backspace: "⌫", delete: "⌫", escape: "⎋", enter: "⏎", left: "←", right: "→", up: "↑", down: "↓" };
  const NAMES = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", cmd: "Win", backspace: "Backspace", delete: "Delete", escape: "Esc", enter: "Enter", left: "Left", right: "Right", up: "Up", down: "Down" };
  const SHARED = { space: "Space", pagedown: "Page Down", pageup: "Page Up" };
  const ICONS = {
    ctrl: mac ? "key-control" : "key-control-iso",
    alt: mac ? "key-option" : "key-alt-iso",
    shift: "key-shift",
    ...(mac ? { cmd: "key-command", tab: "key-tab" } : { tab: "key-tab-iso" }),
    escape: "key-escape",
    enter: "key-return",
    backspace: "key-backspace",
    up: "arrow-up",
    down: "arrow-down",
    left: "arrow-left",
    right: "arrow-right",
  };

  function parse(stroke) {
    const parts = stroke.endsWith("--") ? [...stroke.slice(0, -2).split("-"), "-"] : stroke.split("-");
    const key = parts.pop();
    return { held: MODIFIERS.filter((name) => parts.includes(name)), key };
  }

  function name(key, lowercase) {
    const named = (mac ? MAC_NAMES : NAMES)[key] ?? SHARED[key] ?? key.charAt(0).toUpperCase() + key.slice(1);
    return lowercase ? named.toLowerCase() : named;
  }

  const separator = mac ? "" : "+";

  function text(stroke, lowercase = false) {
    const { held, key } = parse(stroke);
    return [...held.map((each) => name(each)), name(key, lowercase)].join(separator);
  }

  function glyphs(stroke, lowercase = false) {
    const { held, key } = parse(stroke);
    const part = (each, lower) => (ICONS[each] ? { icon: ICONS[each] } : { text: name(each, lower) });
    const merged = [];
    for (const label of [...held.map((each) => part(each, false)), part(key, lowercase)]) {
      const last = merged.at(-1);
      if (last?.text !== undefined && label.text !== undefined) last.text += separator + label.text;
      else merged.push(label);
    }
    return merged.map((part) => {
      if (part.icon) {
        const icon = document.createElement("i");
        icon.dataset.icon = part.icon;
        return icon;
      }
      const span = document.createElement("span");
      span.setAttribute("data-penna-text", "");
      span.textContent = part.text;
      return span;
    });
  }

  function fill(root = document) {
    root.querySelectorAll("[data-kbd]").forEach((cap) => {
      cap.replaceChildren(...glyphs(cap.dataset.kbd, cap.hasAttribute("data-kbd-lowercase")));
      cap.setAttribute("aria-label", text(cap.dataset.kbd, cap.hasAttribute("data-kbd-lowercase")));
    });
    root.querySelectorAll("[data-kbd-text]").forEach((run) => {
      run.textContent = text(run.dataset.kbdText);
    });
  }

  fill();
  globalThis.pennaKbd = { mac, text, glyphs, fill };
})();
