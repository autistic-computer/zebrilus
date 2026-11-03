document.querySelectorAll("[data-focused]").forEach((element) => {
  element.addEventListener("blur", () => element.removeAttribute("data-focused"), { once: true });
});
document.querySelector(".penna-select[data-pinned]")?.focus({ preventScroll: true });
