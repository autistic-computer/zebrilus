function settingsNavigation(nav) {
  const buttons = () => [...nav.querySelectorAll(".penna-settings-nav-button")];
  nav.addEventListener("click", (event) => {
    const button = event.target.closest(".penna-settings-nav-button");
    if (!button) return;
    if (button.dataset.id === "back") {
      nav.dispatchEvent(new CustomEvent("penna-back", { bubbles: true }));
      return;
    }
    const index = buttons().filter((each) => each.dataset.id !== "back").indexOf(button);
    nav.dispatchEvent(new CustomEvent("penna-pick", { bubbles: true, detail: { index } }));
    if (nav.hasAttribute("data-controlled")) return;
    buttons().forEach((other) => other.removeAttribute("aria-current"));
    button.setAttribute("aria-current", "page");
    pennaKit.ceilText(nav);
  });
  nav.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const list = buttons();
    const index = list.indexOf(document.activeElement);
    if (index < 0) return;
    event.preventDefault();
    const next = list[(index + (event.key === "ArrowDown" ? 1 : list.length - 1)) % list.length];
    next.focus();
  });
}

document.querySelectorAll(".penna-settings-nav").forEach(settingsNavigation);
