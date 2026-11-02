document.querySelectorAll(".penna-dialog-stage").forEach((stage) => {
  const all = [...stage.querySelectorAll(":scope > .penna-dialog-layer")];
  stage.querySelector(".dialog-backdrop").addEventListener("click", () => {
    if (pennaDialog.top(stage)) return;
    all.forEach((layer) => pennaDialog.open(layer));
  });
});
