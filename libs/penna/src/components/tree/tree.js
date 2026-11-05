function treeGuides(depths) {
  const done = [];
  const open = [];
  depths.forEach((depth, row) => {
    while (depth < open.length) done.push(open.pop());
    for (let level = open.length; level < depth; level++) open.push({ level, start: row, count: 0 });
    open.forEach((guide) => {
      guide.count = row - guide.start + 1;
    });
  });
  return done.concat(open);
}

function treeDraw(tree) {
  tree.querySelectorAll(".penna-tree-guide").forEach((guide) => guide.remove());
  const rows = [...tree.querySelectorAll(".penna-tree-row")];
  const depths = rows.map((row) => Number(row.dataset.depth) || 0);
  const active = rows.findIndex((row) => row.getAttribute("aria-selected") === "true");
  const hairline = 1 / (devicePixelRatio || 1);
  const style = getComputedStyle(tree);
  const length = (name) => parseFloat(style.getPropertyValue(name)) || 0;
  const indent = length("--penna-tree-indent");
  const offset = length("--penna-tree-offset");
  const end = length("--penna-tree-end");
  const height = rows.length ? rows[0].getBoundingClientRect().height : 0;
  const left = parseFloat(style.paddingLeft) || 0;
  const top = parseFloat(style.paddingTop) || 0;
  treeGuides(depths).forEach((guide) => {
    const line = document.createElement("div");
    line.className = "penna-tree-guide";
    const lit = active >= 0 && guide.level + 1 === depths[active] && guide.start <= active && active < guide.start + guide.count;
    if (lit) line.dataset.active = "";
    line.style.left = `${left + guide.level * indent + offset}px`;
    line.style.top = `${top + guide.start * height + end}px`;
    line.style.height = `${guide.count * height - end * 2}px`;
    line.style.width = `${hairline}px`;
    tree.append(line);
  });
}

function treeSelect(tree, row) {
  tree.querySelectorAll(".penna-tree-row").forEach((other) => {
    other.setAttribute("aria-selected", String(other === row));
    other.tabIndex = other === row ? 0 : -1;
  });
  treeDraw(tree);
}

function treeInit(element) {
  element.addEventListener("click", (event) => {
    const row = event.target.closest(".penna-tree-row");
    if (!row) return;
    treeSelect(element, row);
    row.focus({ preventScroll: true });
  });
  element.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = [...element.querySelectorAll(".penna-tree-row")];
    const index = rows.findIndex((row) => row.getAttribute("aria-selected") === "true");
    const next = rows[Math.min(Math.max(index + (event.key === "ArrowDown" ? 1 : -1), 0), rows.length - 1)];
    event.preventDefault();
    treeSelect(element, next);
    next.focus({ preventScroll: true });
  });
  const first = element.querySelector('.penna-tree-row[aria-selected="true"]') || element.querySelector(".penna-tree-row");
  if (first) first.tabIndex = 0;
  treeDraw(element);
}

document.querySelectorAll(".penna-tree").forEach(treeInit);
