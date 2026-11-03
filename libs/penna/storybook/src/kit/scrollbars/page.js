for (const lines of document.querySelectorAll(".story-lines")) {
  for (let line = 1; line <= 40; line++) {
    const row = document.createElement("div");
    row.textContent = `${String(line).padStart(3)}  cargo build --workspace --all-features --release --target wasm32`;
    row.toggleAttribute("data-strong", line % 5 === 0);
    lines.append(row);
  }
}
