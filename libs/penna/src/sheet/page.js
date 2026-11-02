document.querySelectorAll(".sheet-phone").forEach((phone) => {
  const sheet = phone.querySelector(".penna-sheet");
  phone.addEventListener("click", (event) => {
    if (sheet.hasAttribute("data-closed") && !sheet.contains(event.target)) sheetOpen(sheet);
  });
});
