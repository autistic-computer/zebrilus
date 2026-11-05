(() => {
function wireToggle(toggle) {
  if (toggle.pennaToggle) return;
  toggle.pennaToggle = true;
  toggle.setAttribute("role", "switch");
  if (!toggle.hasAttribute("aria-checked")) toggle.setAttribute("aria-checked", "false");
  if (!toggle.querySelector(".penna-toggle-track")) {
    const track = document.createElement("span");
    track.className = "penna-toggle-track";
    const thumb = document.createElement("span");
    thumb.className = "penna-toggle-thumb";
    track.append(thumb);
    toggle.prepend(track);
  }
  toggle.addEventListener("mousedown", (event) => {
    if (event.button === 0) event.preventDefault();
  });
  toggle.addEventListener("click", () => {
    if (toggle.disabled) return;
    const next = toggle.getAttribute("aria-checked") !== "true";
    toggle.setAttribute("aria-checked", String(next));
    toggle.dispatchEvent(new CustomEvent("change", { detail: { checked: next } }));
  });
}

function underShadow() {
  const contact = getComputedStyle(document.documentElement).getPropertyValue("--penna-shadow-contact");
  const color = contact.match(/#[0-9a-f]{3,8}\b|rgba?\([^)]*\)/i)?.[0];
  document.querySelectorAll(".penna-toggle").forEach((toggle) => {
    if (color) toggle.style.setProperty("--penna-toggle-under", color);
    else toggle.style.removeProperty("--penna-toggle-under");
  });
}

document.querySelectorAll(".penna-toggle").forEach(wireToggle);
underShadow();
document.addEventListener("load", (event) => event.target.id === "penna-tokens" && underShadow(), true);
})();
