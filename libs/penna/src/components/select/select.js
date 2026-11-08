(() => {
const SELECTS = ".penna-select";

function options(select) {
  return (select.dataset.options || "").split(",").map((name) => name.trim()).filter(Boolean);
}

function picked(select) {
  const index = Number.parseInt(select.dataset.value ?? "", 10);
  return Number.isInteger(index) && index >= 0 && index < options(select).length ? index : -1;
}

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

function ground(button) {
  const style = getComputedStyle(button);
  const base = rgba(style.getPropertyValue("--penna-bg"));
  const fill = rgba(style.getPropertyValue("--penna-fill"));
  const mix = (channel) => Math.round(base[channel] * (1 - fill.a) + fill[channel] * fill.a);
  button.style.setProperty("--select-ground", `rgb(${mix("r")} ${mix("g")} ${mix("b")})`);
}

function trigger(select) {
  let button = select.querySelector(":scope > .penna-select-trigger");
  if (!button) {
    button = document.createElement("div");
    button.className = "penna-select-trigger";
    button.dataset.penna = "field";
    const title = document.createElement("span");
    title.className = "penna-select-title";
    title.dataset.pennaText = "";
    const caret = document.createElement("i");
    caret.dataset.icon = "chevron-down";
    button.append(title, caret);
    select.prepend(button);
  }
  return button;
}

function paintTrigger(select) {
  const button = trigger(select);
  const title = button.querySelector(".penna-select-title");
  const index = picked(select);
  const names = options(select);
  title.textContent = index >= 0 ? names[index] : select.dataset.placeholder || "Select";
  title.toggleAttribute("data-placeholder", index < 0);
  if (select.dataset.tone) button.dataset.tone = select.dataset.tone;
  else delete button.dataset.tone;
  if (select.getAttribute("aria-disabled") === "true") button.dataset.state = "disabled";
  else if (select.hasAttribute("data-open") || select.matches(":focus-visible")) button.dataset.state = "focused";
  else button.removeAttribute("data-state");
  ground(button);
  pennaKit.ceilText(button);
}

function menu(select) {
  return select.querySelector(":scope > .penna-select-menu");
}

function searchable(select) {
  const flag = select.dataset.searchable;
  return flag === undefined ? options(select).length > 12 : flag !== "false";
}

function rows(select) {
  return [...(menu(select)?.querySelectorAll('[data-penna="menu-item"]') ?? [])];
}

function visible(select) {
  return rows(select).flatMap((row, at) => (row.hidden ? [] : [at]));
}

function searchField(select) {
  const field = document.createElement("label");
  field.className = "penna-input";
  field.dataset.penna = "field";
  const prefix = document.createElement("span");
  prefix.className = "penna-input-prefix";
  const glass = document.createElement("i");
  glass.dataset.icon = "search";
  prefix.append(glass);
  const input = document.createElement("input");
  input.placeholder = select.dataset.searchPlaceholder || "Search";
  input.setAttribute("aria-label", input.placeholder);
  const suffix = document.createElement("span");
  suffix.className = "penna-input-suffix";
  const clear = document.createElement("button");
  clear.className = "penna-button penna-input-clear";
  clear.dataset.id = "penna-input-clear";
  clear.dataset.penna = "button.quiet";
  clear.dataset.shape = "icon";
  clear.dataset.size = "small";
  clear.type = "button";
  clear.tabIndex = -1;
  clear.setAttribute("aria-label", "Clear");
  const mark = document.createElement("i");
  mark.dataset.icon = "circle-x";
  clear.append(mark);
  suffix.append(clear);
  field.append(prefix, input, suffix);
  input.addEventListener("input", () => filter(select));
  const wrap = document.createElement("div");
  wrap.className = "penna-select-search";
  wrap.append(field);
  return wrap;
}

function filter(select) {
  const list = menu(select);
  const input = list?.querySelector(".penna-select-search input");
  if (!input) return;
  const query = input.value.trim().toLowerCase();
  const names = options(select);
  rows(select).forEach((row, at) => {
    row.hidden = Boolean(query) && !names[at].toLowerCase().includes(query);
  });
  const shown = visible(select);
  list.querySelector(".penna-select-none").hidden = shown.length > 0 || !query;
  const index = picked(select);
  highlight(select, !query && index >= 0 ? index : shown[0] ?? -1);
}

function highlight(select, index) {
  const list = menu(select);
  if (!list) return;
  select.dataset.highlight = String(index);
  list.querySelectorAll('[data-penna="menu-item"]').forEach((item, at) => {
    if (at === index) item.dataset.state = "selected";
    else item.removeAttribute("data-state");
  });
  const row = list.querySelectorAll('[data-penna="menu-item"]')[index];
  row?.scrollIntoView({ block: "nearest" });
}

function buildMenu(select) {
  menu(select)?.remove();
  const list = document.createElement("div");
  list.className = "penna-menu penna-select-menu";
  list.dataset.penna = "popover";
  const search = searchable(select);
  let holder = list;
  if (search) {
    list.dataset.searchable = "";
    holder = document.createElement("div");
    holder.className = "penna-select-rows";
    holder.setAttribute("role", "menu");
    list.append(searchField(select), holder);
  } else {
    list.setAttribute("role", "menu");
  }
  const names = options(select);
  const index = picked(select);
  names.forEach((name, at) => {
    const item = document.createElement("div");
    item.dataset.penna = "menu-item";
    item.dataset.id = String(at);
    item.setAttribute("role", "menuitem");
    const check = document.createElement("i");
    check.className = "penna-select-check";
    if (at === index) check.dataset.icon = "check";
    const label = document.createElement("span");
    label.className = "penna-label";
    label.dataset.pennaText = "";
    label.textContent = name;
    item.append(check, label);
    item.addEventListener("mouseenter", () => highlight(select, at));
    item.addEventListener("mouseleave", () => {
      if (select.dataset.highlight === String(at)) highlight(select, -1);
    });
    item.addEventListener("click", (event) => {
      event.stopPropagation();
      commit(select, at);
    });
    holder.append(item);
  });
  if (search) {
    const none = document.createElement("div");
    none.className = "penna-select-none";
    none.hidden = true;
    const label = document.createElement("span");
    label.className = "penna-label";
    label.dataset.pennaText = "";
    label.textContent = "No matches";
    none.append(label);
    holder.append(none);
  }
  if (!names.length) {
    const empty = document.createElement("div");
    empty.className = "penna-select-empty";
    const icon = document.createElement("i");
    icon.dataset.icon = "inbox";
    empty.append(icon);
    holder.append(empty);
  }
  list.addEventListener("mousedown", (event) => {
    if (!event.target.closest(".penna-select-search")) event.preventDefault();
  });
  const frame = select.closest("section") ?? document.documentElement;
  list.style.setProperty("--penna-select-max", `${Math.min(frame.getBoundingClientRect().height * 0.5, 450)}px`);
  select.append(list);
  pennaKit.ceilText(list);
  if (search) {
    globalThis.pennaInput?.wire(list);
    list.style.width = `${list.getBoundingClientRect().width}px`;
    list.querySelector(".penna-select-search input").focus({ preventScroll: true });
  }
  highlight(select, index);
  return list;
}

function open(select) {
  if (select.hasAttribute("data-open") || select.getAttribute("aria-disabled") === "true") return;
  select.toggleAttribute("data-open", true);
  select.setAttribute("aria-expanded", "true");
  paintTrigger(select);
  buildMenu(select);
}

function close(select) {
  if (!select.hasAttribute("data-open")) return;
  const inside = menu(select)?.contains(document.activeElement);
  select.removeAttribute("data-open");
  select.setAttribute("aria-expanded", "false");
  menu(select)?.remove();
  if (inside) select.focus({ preventScroll: true });
  paintTrigger(select);
}

function commit(select, index) {
  select.dataset.value = String(index);
  close(select);
  paintTrigger(select);
  select.dispatchEvent(new CustomEvent("change", { detail: { index, value: options(select)[index] } }));
}

function step(select, delta) {
  const shown = visible(select);
  if (!shown.length) return;
  const at = shown.indexOf(Number.parseInt(select.dataset.highlight ?? "-1", 10));
  const next = at < 0 ? (delta > 0 ? shown[0] : shown.at(-1)) : shown[(at + delta + shown.length) % shown.length];
  highlight(select, next);
}

function wire(select) {
  if (select.penna) return;
  select.penna = true;
  const disabled = select.getAttribute("aria-disabled") === "true";
  if (!disabled) select.tabIndex = 0;
  select.setAttribute("role", "combobox");
  select.setAttribute("aria-haspopup", "menu");
  paintTrigger(select);
  select.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || disabled || menu(select)?.contains(event.target)) return;
    event.preventDefault();
    select.focus();
    if (select.hasAttribute("data-open")) close(select);
    else open(select);
  });
  select.addEventListener("keydown", (event) => {
    if (disabled) return;
    const shown = select.hasAttribute("data-open");
    if (event.key === "Escape" && shown) {
      event.preventDefault();
      close(select);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (shown) step(select, event.key === "ArrowDown" ? 1 : -1);
      else open(select);
    } else if (event.key === "Enter" || (event.key === " " && !event.target.closest(".penna-select-search"))) {
      event.preventDefault();
      const at = Number.parseInt(select.dataset.highlight ?? "-1", 10);
      if (shown && at >= 0) commit(select, at);
      else if (!shown) open(select);
    } else if (event.key === "Tab" && shown) {
      close(select);
    }
  });
  select.addEventListener("focusin", () => paintTrigger(select));
  select.addEventListener("focusout", (event) => {
    if (select.contains(event.relatedTarget)) return;
    if (!select.hasAttribute("data-pinned") && document.hasFocus()) close(select);
    requestAnimationFrame(() => paintTrigger(select));
  });
  if (select.hasAttribute("data-open")) {
    select.removeAttribute("data-open");
    open(select);
  }
}

function settleSelects(root = document) {
  root.querySelectorAll(SELECTS).forEach((select) => {
    wire(select);
    paintTrigger(select);
  });
}

document.addEventListener("mousedown", (event) => {
  document.querySelectorAll(`${SELECTS}[data-open]`).forEach((select) => {
    if (!select.contains(event.target)) {
      select.removeAttribute("data-pinned");
      close(select);
    }
  });
});

settleSelects();
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && requestAnimationFrame(() => settleSelects()), true);
})();
