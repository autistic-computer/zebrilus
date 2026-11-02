document.querySelectorAll("[data-focused]").forEach((element) => {
  element.addEventListener("blur", () => element.removeAttribute("data-focused"), { once: true });
});
