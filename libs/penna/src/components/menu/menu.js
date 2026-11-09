(() => {
  let keyboard = false;
  addEventListener("keydown", () => (keyboard = true), true);
  addEventListener("pointermove", () => (keyboard = false), true);
  addEventListener("pointerdown", () => (keyboard = false), true);

  function make(tag, className, attrs = {}, children = []) {
    const element = document.createElement(tag);
    if (className) element.className = className;
    for (const [key, value] of Object.entries(attrs)) {
      if (value === false || value == null) continue;
      element.setAttribute(key, value === true ? "" : value);
    }
    element.append(...children);
    return element;
  }

  const text = (value, className = "penna-label") => make("span", className, { "data-penna-text": true }, [value]);
  const glyph = (name, className = "") => make("i", className, { "data-icon": name });

  const probe = document.createElement("i");
  probe.hidden = true;

  function alpha(value) {
    if (!probe.isConnected) document.body.append(probe);
    probe.style.color = "";
    probe.style.color = value.trim() || "transparent";
    const parts = getComputedStyle(probe).color.match(/[\d.]+/g)?.map(Number) ?? [];
    return parts.length === 4 || parts.length === 5 ? parts.at(-1) : parts.length ? 1 : 0;
  }

  function edges(root = document) {
    root.querySelectorAll(".penna-popup-stepper").forEach((group) => {
      const style = getComputedStyle(group);
      group.toggleAttribute("data-edged", alpha(style.getPropertyValue("--penna-edge")) > 0);
      if (!group.matches(":hover")) group.style.setProperty("--penna-popup-field", style.getPropertyValue("--penna-fill"));
    });
  }

  function fit(root = document) {
    edges(root);
    globalThis.pennaKit?.ceilText(root);
  }

  function state(element, name, on) {
    const states = new Set((element.dataset.state ?? "").split(/\s+/).filter(Boolean));
    if (on) states.add(name);
    else states.delete(name);
    if (states.size) element.dataset.state = [...states].join(" ");
    else element.removeAttribute("data-state");
  }

  const open = new Set();

  class Menu {
    constructor(items, options = {}) {
      this.items = items;
      this.parent = options.parent ?? null;
      this.onDismiss = options.onDismiss ?? null;
      this.onAction = options.onAction ?? null;
      this.selected = null;
      this.submenus = new Map();
      this.element = make("div", "penna-popup", { "data-penna": "popover" });
      this.list = make("div", "penna-popup-items", { role: "menu", tabindex: "-1" });
      this.element.append(this.list);
      this.element.pennaMenu = this;
      const icons = items.some((item) => item.icon || item.checked);
      const last = items.length - 1;
      this.rows = items.map((item, ix) => (ix === last && item.kind === "separator" ? null : this.row(item, ix, icons)));
      this.list.append(...this.rows.filter(Boolean));
      this.list.addEventListener("keydown", (event) => this.key(event));
      this.query = "";
      this.scopes = options.scopes ?? [];
      this.scope = Math.max(0, Math.min(options.scope ?? 0, this.scopes.length - 1));
      this.search = options.search ? this.searchField(options.search) : null;
      if (this.scopes.length) this.scopeField();
      if (this.search || this.scopes.length) {
        this.none = make("div", "penna-popup-none", { hidden: true }, [text("No matches")]);
        this.list.append(this.none);
      }
      if (this.scopes.length) this.refilter();
    }

    scopeField() {
      const segments = make("div", "penna-segments", { "data-penna": "button.quiet", "data-state": "current" }, [
        make("span", "penna-segment-pill"),
        ...this.scopes.map((scope) => make("span", "penna-segment", { role: "button", "data-id": `button:${scope.label}` }, [text(scope.label)])),
      ]);
      this.scopeControl = make("div", "penna-segmented", { "data-penna": "field", "data-width": "full", "data-value": String(this.scope), role: "radiogroup" }, [segments]);
      this.scopeControl.addEventListener("change", (event) => this.setScope(event.detail.index));
      this.element.setAttribute("data-scopes", "");
      this.list.before(make("div", "penna-popup-scopes", {}, [this.scopeControl]));
    }

    setScope(scope) {
      this.scope = Math.max(0, Math.min(scope, this.scopes.length - 1));
      if (this.scopeControl.dataset.value !== String(this.scope)) globalThis.pennaSegmented?.set(this.scopeControl, this.scope);
      this.refilter();
    }

    stepScope(delta) {
      const count = this.scopes.length;
      if (count > 1) this.setScope((((this.scope + delta) % count) + count) % count);
    }

    searchField(placeholder) {
      const input = make("input", "", { placeholder, "aria-label": placeholder });
      const clear = make("button", "penna-button penna-input-clear", { type: "button", tabindex: "-1", "data-id": "penna-input-clear", "data-penna": "button.quiet", "data-shape": "icon", "data-size": "small", "aria-label": "Clear" }, [glyph("circle-x")]);
      const field = make("label", "penna-input", { "data-penna": "field" }, [make("span", "penna-input-prefix", {}, [glyph("search")]), input, make("span", "penna-input-suffix", {}, [clear])]);
      input.addEventListener("input", () => this.filter(input.value));
      input.addEventListener("keydown", (event) => {
        if (["ArrowDown", "ArrowUp", "Enter", "Escape", "Tab"].includes(event.key)) this.key(event);
      });
      this.element.setAttribute("data-search", "");
      this.element.prepend(make("div", "penna-popup-search", {}, [field]));
      return input;
    }

    keep() {
      return this.scopes[this.scope]?.keep ?? null;
    }

    matches(ix) {
      const item = this.items[ix];
      const keep = this.keep();
      if (!this.query && !keep) return true;
      return ["item", "submenu", "stepper"].includes(item.kind ?? "item") && item.label.toLowerCase().includes(this.query) && (!keep || keep(item.label));
    }

    filter(value) {
      this.query = value.trim().toLowerCase();
      this.refilter();
    }

    refilter() {
      this.rows.forEach((row, ix) => {
        if (row) row.hidden = !this.matches(ix);
      });
      this.none.hidden = !(this.query || this.keep()) || this.items.some((_, ix) => this.clickable(ix));
      const checked = this.query ? -1 : this.items.findIndex((item, ix) => item.checked && this.clickable(ix));
      if (checked >= 0) this.select(checked);
      else this.first();
    }

    slot(icons, item) {
      if (!icons) return [];
      const name = item.icon ?? (item.checked ? "check" : null);
      return [make("span", "penna-popup-icon", {}, name ? [glyph(name)] : [])];
    }

    clickable(ix) {
      const item = this.items[ix];
      return Boolean(item) && item.kind !== "separator" && item.kind !== "label" && !item.disabled && this.matches(ix);
    }

    row(item, ix, icons) {
      const kind = item.kind ?? "item";
      if (kind === "separator") return make("div", "penna-popup-separator", { "data-id": ix, "data-kind": kind, role: "separator" });
      const disabled = kind === "label" || item.disabled;
      const row = make("div", "penna-popup-row", { "data-penna": "menu-item", "data-id": ix, "data-kind": kind, role: kind === "label" ? "presentation" : "menuitem", "data-disabled": disabled });
      if (kind === "label") {
        row.append(make("span", "penna-popup-line", {}, [...this.slot(icons, {}), make("span", "penna-popup-text", {}, [text(item.label)])]));
      } else if (kind === "stepper") {
        row.setAttribute("aria-label", `${item.label}: ${item.format(item.value)}; Left and Right adjust, Enter resets`);
        const button = (id, attrs, children) =>
          make("button", "penna-button", { type: "button", tabindex: "-1", "data-id": "penna-input-clear", "data-penna": "button.quiet", "data-flat": true, "data-size": "small", "data-id": `${id}-${ix}`, ...attrs }, children);
        const value = text(item.format(item.value));
        const group = make("span", "penna-popup-stepper", { "data-penna": "field" }, [
          button("menu-step-down", { "data-shape": "icon", "aria-label": `Decrease ${item.label}`, "data-tooltip": `Decrease ${item.label}`, "data-step": "down" }, [glyph("minus")]),
          button("menu-step-reset", { "data-reset": true, "aria-label": `Reset ${item.label}`, "data-tooltip": `Reset ${item.label}`, "data-step": "reset" }, [value]),
          button("menu-step-up", { "data-shape": "icon", "aria-label": `Increase ${item.label}`, "data-tooltip": `Increase ${item.label}`, "data-step": "up" }, [glyph("plus")]),
        ]);
        group.addEventListener("click", (event) => {
          const target = event.target.closest("[data-step]");
          if (!target) return;
          event.stopPropagation();
          this.select(ix);
          this.step(ix, target.dataset.step);
        });
        item.render = () => {
          value.textContent = item.format(item.value);
          row.setAttribute("aria-label", `${item.label}: ${item.format(item.value)}; Left and Right adjust, Enter resets`);
          globalThis.pennaKit?.ceilText(row);
        };
        const ground = make("span", "penna-popup-ground", { "data-penna": "popover", "data-ground": true }, [group]);
        row.append(...this.slot(icons, {}), make("span", "penna-popup-text", {}, [text(item.label)]), ground);
      } else if (kind === "note") {
        row.append(make("span", "penna-popup-line", {}, [...this.slot(icons, item), text(item.label, "penna-label penna-popup-note")]));
      } else if (kind === "submenu") {
        const main = make("span", "penna-popup-main", {}, [text(item.label), glyph("chevron-right", "penna-popup-detail")]);
        row.append(make("span", "penna-popup-line", {}, [...this.slot(icons, item), main]));
        row.setAttribute("aria-haspopup", "menu");
      } else {
        const label = item.link ? make("span", "penna-popup-link", {}, [text(item.label), glyph("external-link", "penna-popup-detail")]) : text(item.label);
        const main = make("span", "penna-popup-main", {}, [label]);
        if (item.key) main.append(text(globalThis.pennaKbd.text(item.key), "penna-label penna-popup-key"));
        row.append(...this.slot(icons, item), main);
      }
      row.addEventListener("mouseenter", () => this.hover(ix, true));
      row.addEventListener("mouseleave", () => this.hover(ix, false));
      row.addEventListener("mousedown", (event) => {
        if (event.button === 0) event.stopPropagation();
      });
      row.addEventListener("click", (event) => {
        if (event.target.closest(".penna-popup") !== this.element || event.target.closest(".penna-popup-stepper")) return;
        if (!this.clickable(ix)) return;
        event.stopPropagation();
        this.selected = ix;
        this.confirm();
      });
      return row;
    }

    hover(ix, on) {
      if (on) {
        const sub = this.active();
        if (this.selected !== ix && sub && sub.list.contains(document.activeElement)) this.focus();
        this.select(ix);
      } else if (this.items[ix].kind !== "submenu" && this.selected === ix && !keyboard) {
        this.select(null);
      }
    }

    active() {
      const item = this.items[this.selected];
      return item?.kind === "submenu" && !item.disabled ? this.submenu(this.selected) : null;
    }

    submenu(ix) {
      if (!this.submenus.has(ix)) {
        const sub = new Menu(this.items[ix].items, { parent: this, onAction: this.onAction });
        sub.element.classList.add("penna-popup-sub");
        this.submenus.set(ix, sub);
      }
      return this.submenus.get(ix);
    }

    select(ix) {
      this.selected = ix;
      this.paint();
      if ((this.search || this.scopes.length) && ix != null) this.reveal(this.rows[ix]);
    }

    reveal(row) {
      if (!row) return;
      const box = this.list.getBoundingClientRect();
      const at = row.getBoundingClientRect();
      if (at.top < box.top) this.list.scrollTop -= box.top - at.top;
      else if (at.bottom > box.bottom) this.list.scrollTop += at.bottom - box.bottom;
    }

    paint() {
      this.rows.forEach((row, ix) => {
        if (!row || row.classList.contains("penna-popup-separator")) return;
        const item = this.items[ix];
        const on = this.selected === ix && !row.hasAttribute("data-disabled");
        const state = on ? (item.kind === "submenu" ? "hover" : "selected") : null;
        if (state) row.dataset.state = state;
        else row.removeAttribute("data-state");
        row.toggleAttribute("data-selected", on);
      });
      this.submenus.forEach((sub, ix) => {
        if (this.selected === ix && !this.items[ix].disabled) {
          if (!sub.element.isConnected) {
            this.rows[ix].append(sub.element);
            fit(sub.element);
          }
        } else if (sub.element.isConnected) {
          sub.selected = null;
          sub.paint();
          sub.element.remove();
        }
      });
      const item = this.items[this.selected];
      if (item?.kind === "submenu" && !item.disabled && !this.submenus.has(this.selected)) {
        this.submenu(this.selected);
        this.paint();
      }
    }

    focus() {
      (this.search ?? this.list).focus({ preventScroll: true });
    }

    step(ix, action) {
      const item = this.items[ix];
      if (action === "down") item.value = Math.max(item.min ?? -Infinity, item.value - item.step);
      if (action === "up") item.value = Math.min(item.max ?? Infinity, item.value + item.step);
      if (action === "reset") item.value = item.initial;
      item.render();
    }

    stepper(action) {
      const item = this.items[this.selected];
      if (item?.kind !== "stepper") return false;
      this.step(this.selected, action);
      return true;
    }

    first() {
      const ix = this.items.findIndex((_, ix) => this.clickable(ix));
      this.select(ix < 0 ? null : ix);
    }

    move(delta) {
      const ixs = this.items.map((_, ix) => ix).filter((ix) => this.clickable(ix));
      const filters = this.search || this.scopes.length;
      const first = filters ? (ixs[0] ?? null) : 0;
      if (delta > 0) {
        if (this.selected == null) return this.select(first);
        this.select(ixs.find((ix) => ix > this.selected) ?? first);
      } else {
        const from = this.selected ?? 0;
        this.select([...ixs].reverse().find((ix) => ix < from) ?? ixs.at(-1) ?? (filters ? null : 0));
      }
    }

    key(event) {
      const handled = () => {
        event.preventDefault();
        event.stopPropagation();
      };
      if (event.key === "ArrowDown") return handled(), this.move(1);
      if (event.key === "ArrowUp") return handled(), this.move(-1);
      if (event.key === "Enter") return handled(), this.confirm();
      if (event.key === "Escape") return handled(), this.dismiss();
      if (event.key === "Tab" && this.scopes.length > 1 && !event.altKey && !event.ctrlKey && !event.metaKey) return handled(), this.stepScope(event.shiftKey ? -1 : 1);
      if (event.key === "ArrowRight") {
        if (this.stepper("up")) return handled();
        const sub = this.active();
        if (sub) {
          handled();
          sub.select(0);
          sub.focus();
          return;
        }
        if (this.parent) handled();
        return;
      }
      if (event.key === "ArrowLeft") {
        if (this.stepper("down")) return handled();
        const sub = this.active();
        if (sub) {
          handled();
          sub.select(null);
          return;
        }
        if (this.parent) {
          handled();
          this.select(null);
          this.parent.focus();
        }
      }
    }

    confirm() {
      if (this.stepper("reset")) return;
      const item = this.items[this.selected];
      if (!item || !this.clickable(this.selected) || item.kind === "submenu") return;
      this.list.dispatchEvent(new CustomEvent("penna-menu-action", { bubbles: true, detail: { label: item.label, action: item.action ?? null } }));
      this.onAction?.(item);
      if (item.link) window.open(item.link, "_blank", "noopener");
      this.dismiss();
    }

    dismiss() {
      this.selected = null;
      this.paint();
      if (this.parent) {
        this.parent.dismiss();
        return;
      }
      if (this.onDismiss) this.onDismiss();
      else if (this.element.isConnected && (document.activeElement === document.body || this.element.contains(document.activeElement))) this.focus();
    }
  }

  function float(menu, host, x, y, previous) {
    menu.element.setAttribute("data-floating", "");
    menu.element.style.left = `${x}px`;
    menu.element.style.top = `${y}px`;
    host.append(menu.element);
    fit(menu.element);
    if (menu.search) globalThis.pennaInput?.wire(menu.element);
    if (menu.scopes.length) globalThis.pennaSegmented?.wire(menu.element);
    if (menu.search || menu.scopes.length) menu.element.style.width = `${menu.element.getBoundingClientRect().width}px`;
    globalThis.pennaKit?.roundAll();
    open.add(menu);
    menu.previous = previous ?? null;
    menu.focus();
    return menu;
  }

  function close(menu) {
    for (const each of menu ? [menu] : [...open]) {
      const had = each.element.contains(document.activeElement);
      each.selected = null;
      each.paint();
      each.element.remove();
      open.delete(each);
      each.closed?.();
      if (had && each.previous?.isConnected) each.previous.focus({ preventScroll: true });
    }
  }

  const chord = (event) => (event.repeat ? null : ["alt", "ctrl", "meta", "shift"].filter((name) => event[`${name}Key`]).join("-"));

  addEventListener("mousedown", (event) => {
    for (const menu of [...open]) {
      if (!menu.element.contains(event.target) && !menu.trigger?.contains(event.target)) close(menu);
    }
  });

  function dropdown(trigger, items, options = {}) {
    const host = trigger.parentElement;
    let menu = null;
    const show = () => {
      menu = new Menu(items, { onDismiss: () => close(menu), search: options.search, scopes: options.scopes, scope: options.scope });
      menu.trigger = trigger;
      menu.closed = () => {
        trigger.setAttribute("aria-expanded", "false");
        state(trigger, "current", false);
      };
      trigger.setAttribute("aria-expanded", "true");
      state(trigger, "current", true);
      float(menu, host, options.x ?? 0, trigger.offsetTop + trigger.offsetHeight + (options.gap ?? 4), trigger);
    };
    const press = () => {
      if (menu && open.has(menu)) close(menu);
      else {
        close();
        show();
      }
    };
    trigger.setAttribute("aria-haspopup", "menu");
    trigger.addEventListener("mousedown", (event) => {
      if (event.button !== 0) return;
      press();
      event.preventDefault();
    });
    trigger.addEventListener("keydown", (event) => {
      if (!["Enter", " ", "ArrowDown"].includes(event.key) || chord(event) !== "") return;
      event.preventDefault();
      press();
      if (open.has(menu)) menu.first();
    });
    return { show, close: () => menu && close(menu) };
  }

  function context(region, items) {
    let menu = null;
    const show = (x, y) => {
      menu = new Menu(items, { onDismiss: () => close(menu) });
      float(menu, region, x, y, document.activeElement);
    };
    region.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      close();
      const box = region.getBoundingClientRect();
      show(event.clientX - box.left - region.clientLeft, event.clientY - box.top - region.clientTop);
    });
    region.addEventListener("keydown", (event) => {
      const held = chord(event);
      if (!(event.key === "ContextMenu" && held === "") && !(event.key === "F10" && held === "shift")) return;
      event.preventDefault();
      close();
      show(0, 0);
      menu.first();
    });
    return { show, close: () => menu && close(menu) };
  }

  addEventListener("load", () => fit());
  document.fonts?.ready.then(() => fit());
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && fit(), true);

  globalThis.pennaMenu = { Menu, dropdown, context, close, fit };
})();
