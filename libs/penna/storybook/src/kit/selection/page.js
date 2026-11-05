document.querySelectorAll("[data-focused]").forEach((element) => {
  element.addEventListener("blur", () => element.removeAttribute("data-focused"), { once: true });
});
document.querySelectorAll('section [data-state~="focused"]').forEach((element) => {
  element.closest("section").addEventListener("mousedown", () => element.focus({ preventScroll: true }));
});
document.querySelector(".penna-select[data-pinned]")?.focus({ preventScroll: true });
