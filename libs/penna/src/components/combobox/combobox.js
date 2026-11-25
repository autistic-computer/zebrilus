(() => {
  function wire(root) {
    root.querySelectorAll(".penna-combobox").forEach((box) => {
      if (box.dataset.comboboxWired) return;
      box.dataset.comboboxWired = "1";
      const input = box.querySelector(".penna-combobox-input");
      const tokensHost = box.querySelector(".penna-combobox-tokens");
      const menu = box.querySelector(".penna-combobox-menu");
      const empty = box.querySelector(".penna-combobox-empty");
      const options = [...box.querySelectorAll(".penna-combobox-opt")];
      const multiple = box.dataset.multiple !== "false";
      if (!input || !menu) return;

      let activeIdx = 0;

      function visibleOptions() {
        return options.filter((opt) => !opt.hidden);
      }

      function highlight(next) {
        const list = visibleOptions();
        if (!list.length) return;
        activeIdx = ((next % list.length) + list.length) % list.length;
        list.forEach((opt, i) => {
          if (i === activeIdx) opt.dataset.state = "selected";
          else delete opt.dataset.state;
        });
      }

      function renderTokens() {
        if (!tokensHost) return;
        const selected = options.filter((opt) => opt.getAttribute("aria-selected") === "true");
        const frag = document.createDocumentFragment();
        for (const opt of selected) {
          const chip = document.createElement("span");
          chip.className = "penna-combobox-token";
          const lbl = document.createElement("span");
          lbl.textContent = opt.dataset.label || opt.dataset.value || "";
          const rm = document.createElement("button");
          rm.type = "button";
          rm.className = "penna-combobox-token-x";
          rm.setAttribute("aria-label", `Remove ${lbl.textContent}`);
          const icon = document.createElement("i");
          icon.dataset.icon = "xmark";
          rm.append(icon);
          rm.addEventListener("click", (event) => {
            event.stopPropagation();
            opt.setAttribute("aria-selected", "false");
            renderTokens();
            input.focus();
          });
          chip.append(lbl, rm);
          frag.append(chip);
        }
        tokensHost.replaceChildren(frag);
      }

      function filterOptions() {
        const q = input.value.trim().toLowerCase();
        let shown = 0;
        for (const opt of options) {
          const match = !q || opt.textContent.toLowerCase().includes(q) || (opt.dataset.value || "").toLowerCase().includes(q);
          opt.hidden = !match;
          if (match) shown += 1;
        }
        if (empty) empty.hidden = shown > 0;
        activeIdx = 0;
        highlight(0);
      }

      function toggleOption(opt) {
        if (!opt) return;
        const isOn = opt.getAttribute("aria-selected") === "true";
        if (!multiple) {
          options.forEach((other) => other.setAttribute("aria-selected", "false"));
          opt.setAttribute("aria-selected", String(!isOn));
          menu.hidden = true;
        } else {
          opt.setAttribute("aria-selected", String(!isOn));
        }
        input.value = "";
        filterOptions();
        renderTokens();
        box.dispatchEvent(
          new CustomEvent("change", {
            bubbles: true,
            detail: {
              values: options
                .filter((o) => o.getAttribute("aria-selected") === "true")
                .map((o) => o.dataset.value),
            },
          }),
        );
      }

      function openMenu() {
        menu.hidden = false;
        input.setAttribute("aria-expanded", "true");
        filterOptions();
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(menu));
      }

      function closeMenu() {
        menu.hidden = true;
        input.setAttribute("aria-expanded", "false");
      }

      box.querySelector(".penna-combobox-box")?.addEventListener("click", () => {
        input.focus();
        openMenu();
      });
      input.addEventListener("focus", openMenu);
      input.addEventListener("input", () => {
        if (menu.hidden) openMenu();
        else filterOptions();
      });
      input.addEventListener("keydown", (event) => {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          if (menu.hidden) openMenu();
          else highlight(activeIdx + 1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          highlight(activeIdx - 1);
        } else if (event.key === "Enter") {
          const list = visibleOptions();
          if (!menu.hidden && list[activeIdx]) {
            event.preventDefault();
            toggleOption(list[activeIdx]);
          }
        } else if (event.key === "Backspace" && !input.value) {
          const selected = options.filter((o) => o.getAttribute("aria-selected") === "true");
          const last = selected[selected.length - 1];
          if (last) {
            last.setAttribute("aria-selected", "false");
            renderTokens();
          }
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          closeMenu();
        }
      });

      options.forEach((opt) => {
        opt.addEventListener("mousedown", (event) => event.preventDefault());
        opt.addEventListener("click", () => toggleOption(opt));
      });

      document.addEventListener("click", (event) => {
        if (!box.contains(event.target)) closeMenu();
      });

      renderTokens();
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaCombobox = { wire };
})();
