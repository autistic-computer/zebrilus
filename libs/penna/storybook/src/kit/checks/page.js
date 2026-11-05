document.querySelectorAll('section [data-state~="focused"]').forEach((element) => {
  element.closest("section").addEventListener("mousedown", () => element.focus({ preventScroll: true }));
});
