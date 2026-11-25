(() => {
  function parseCell(text) {
    const cleaned = text.trim().replace(/,/g, "").replace(/\s*(ms|%|px|s|k|M|G)$/i, "").trim();
    const num = Number(cleaned);
    return Number.isFinite(num) && cleaned !== "" ? num : null;
  }

  function wire(root) {
    root.querySelectorAll(".penna-table").forEach((table) => {
      if (table.dataset.tableWired) return;
      table.dataset.tableWired = "1";
      const tbody = table.querySelector("tbody");
      const headers = [...table.querySelectorAll("thead th")];
      if (!tbody) return;

      const originalRows = [...tbody.querySelectorAll("tr")];
      let sortCol = -1;
      let sortDir = "none";

      function applySort() {
        headers.forEach((th, index) => {
          const icon = th.querySelector("[data-sort-icon]");
          if (index === sortCol && sortDir !== "none") {
            th.setAttribute("aria-sort", sortDir);
            if (icon) icon.dataset.icon = sortDir === "ascending" ? "chevron-up" : "chevron-down";
          } else {
            th.setAttribute("aria-sort", "none");
            if (icon) icon.dataset.icon = "chevron-down";
          }
        });
        const rows = [...originalRows];
        if (sortCol >= 0 && sortDir !== "none") {
          rows.sort((a, b) => {
            const at = a.children[sortCol]?.textContent || "";
            const bt = b.children[sortCol]?.textContent || "";
            const an = parseCell(at);
            const bn = parseCell(bt);
            const cmp = an !== null && bn !== null ? an - bn : at.localeCompare(bt, undefined, { numeric: true });
            return sortDir === "ascending" ? cmp : -cmp;
          });
        }
        tbody.replaceChildren(...rows);
      }

      headers.forEach((th, index) => {
        if (!th.hasAttribute("data-sort")) return;
        th.addEventListener("click", () => {
          if (sortCol !== index) {
            sortCol = index;
            sortDir = "ascending";
          } else if (sortDir === "ascending") {
            sortDir = "descending";
          } else {
            sortCol = -1;
            sortDir = "none";
          }
          applySort();
        });
      });

      function selectRow(row) {
        const rows = [...tbody.querySelectorAll("tr")];
        rows.forEach((other) => {
          const active = other === row;
          if (active) other.dataset.state = "selected";
          else delete other.dataset.state;
          other.setAttribute("aria-selected", String(active));
        });
      }

      tbody.addEventListener("click", (event) => {
        const row = event.target.closest("tr");
        if (row) selectRow(row);
      });

      tbody.addEventListener("keydown", (event) => {
        const rows = [...tbody.querySelectorAll("tr")];
        const current = rows.indexOf(event.target.closest("tr"));
        if (current < 0) return;
        if (event.key === "ArrowDown" && current + 1 < rows.length) {
          event.preventDefault();
          selectRow(rows[current + 1]);
          rows[current + 1].focus();
        } else if (event.key === "ArrowUp" && current > 0) {
          event.preventDefault();
          selectRow(rows[current - 1]);
          rows[current - 1].focus();
        }
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaTable = { wire };
})();
