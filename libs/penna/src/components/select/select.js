(() => {
const SELECTS = ".penna-select";

function options(select) {
  return (select.dataset.options || "").split(",").map((name) => name.trim()).filter(Boolean);
}

function picked(select) {
  const index = Number.parseInt(select.dataset.value ?? "", 10);
  return Number.isInteger(index) && index >= 0 && index < options(select).length ? index : -1;
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
  if (select.getAttribute("aria-disabled") === "true") button.dataset.state = "disabled";
  else if (select.hasAttribute("data-open") || select.matches(":focus-visible")) button.dataset.state = "focused";
  else button.removeAttribute("data-state");
  pennaKit.ceilText(button);
}

function menu(select) {
  return select.querySelector(":scope > .penna-select-menu");
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
  list.setAttribute("role", "menu");
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
    list.append(item);
  });
  if (!names.length) {
    const empty = document.createElement("div");
    empty.className = "penna-select-empty";
    const icon = document.createElement("i");
    icon.dataset.icon = "inbox";
    empty.append(icon);
    list.append(empty);
  }
  list.addEventListener("mousedown", (event) => event.preventDefault());
  const frame = select.closest("section") ?? document.documentElement;
  list.style.setProperty("--penna-select-max", `${Math.min(frame.getBoundingClientRect().height * 0.5, 450)}px`);
  select.append(list);
  pennaKit.ceilText(list);
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
  select.removeAttribute("data-open");
  select.setAttribute("aria-expanded", "false");
  menu(select)?.remove();
  paintTrigger(select);
}

function commit(select, index) {
  select.dataset.value = String(index);
  close(select);
  paintTrigger(select);
  select.dispatchEvent(new CustomEvent("change", { detail: { index, value: options(select)[index] } }));
}

function step(select, delta) {
  const count = options(select).length;
  if (!count) return;
  const current = Number.parseInt(select.dataset.highlight ?? "-1", 10);
  const next = current < 0 ? (delta > 0 ? 0 : count - 1) : (current + delta + count) % count;
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
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      const at = Number.parseInt(select.dataset.highlight ?? "-1", 10);
      if (shown && at >= 0) commit(select, at);
      else if (!shown) open(select);
    } else if (event.key === "Tab" && shown) {
      close(select);
    }
  });
  select.addEventListener("focusin", () => paintTrigger(select));
  select.addEventListener("focusout", () => {
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
})();
