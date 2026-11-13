document.querySelectorAll(".sheet-phone").forEach((phone) => {
  const sheet = phone.querySelector(".penna-sheet, .penna-modal");
  phone.addEventListener("click", (event) => {
    if (!sheet.hasAttribute("data-closed") || sheet.contains(event.target)) return;
    if (sheet.classList.contains("penna-sheet")) sheetOpen(sheet);
    else sheet.removeAttribute("data-closed");
  });
});
