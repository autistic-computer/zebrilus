(() => {
  if (!document.querySelector("[data-docs-phone]")) return;

  const holdRoot = document.querySelector("[data-phone-hold]");
  if (holdRoot) {
    const modal = holdRoot.querySelector(".penna-modal");
    const views = [...holdRoot.querySelectorAll("[data-phone-hold-view]")];
    const seg = holdRoot.querySelector("[data-phone-hold-mode]");
    const readout = holdRoot.querySelector("[data-phone-hold-status]");
    const card = holdRoot.querySelector("[data-phone-hold-card]");
    const MODES = ["menu", "rename", "close"];
    const LABELS = {
      menu: "Hold menu · ruled group, danger last",
      rename: "Rename modal · 48 px field",
      close: "Confirm modal · asks before closing",
    };

    function syncSeg(index) {
      if (!seg) return;
      if (window.pennaSegmented?.set) {
        window.pennaSegmented.set(seg, index);
        return;
      }
      seg.dataset.value = String(index);
      const segments = [...seg.querySelectorAll(".penna-segment")];
      segments.forEach((el, i) => el.setAttribute("aria-checked", String(i === index)));
    }

    function openHold(mode, fromSeg = false) {
      const index = Math.max(0, MODES.indexOf(mode));
      views.forEach((view) => {
        view.hidden = view.dataset.phoneHoldView !== mode;
      });
      if (modal) {
        modal.removeAttribute("data-closed");
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(modal));
      }
      if (!fromSeg) syncSeg(index);
      if (readout) readout.textContent = LABELS[mode] ?? "";
    }

    seg?.addEventListener("change", (event) => {
      openHold(MODES[event.detail.index] ?? "menu", true);
    });

    holdRoot.querySelectorAll("[data-phone-hold-open]").forEach((trigger) => {
      trigger.addEventListener("click", () => {
        openHold(trigger.dataset.phoneHoldOpen || "menu");
      });
    });

    holdRoot.querySelectorAll("[data-phone-hold-dismiss]").forEach((btn) => {
      btn.addEventListener("click", () => {
        if (modal) modal.setAttribute("data-closed", "");
        if (readout) readout.textContent = btn.dataset.phoneHoldDismiss || "Dismissed — tap card to reopen";
      });
    });

    modal?.querySelector(".penna-modal-scrim")?.addEventListener("click", () => {
      if (readout) readout.textContent = "Dismissed — tap card to reopen";
    });

    card?.addEventListener("click", () => openHold("menu"));
    card?.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      openHold("menu");
    });
  }

  const screenRoot = document.querySelector("[data-phone-screen]");
  if (screenRoot) {
    const zones = [...screenRoot.querySelectorAll("[data-phone-zone]")];
    const parts = [...screenRoot.querySelectorAll("[data-phone-part]")];

    function highlight(name) {
      zones.forEach((z) => z.toggleAttribute("data-active", z.dataset.phoneZone === name));
      parts.forEach((p) => p.toggleAttribute("data-active", p.dataset.phonePart === name));
    }

    zones.forEach((zone) => {
      const name = zone.dataset.phoneZone;
      zone.addEventListener("mouseenter", () => highlight(name));
      zone.addEventListener("click", () => highlight(name));
      zone.addEventListener("focus", () => highlight(name));
    });
    screenRoot.addEventListener("mouseleave", () => highlight(""));
  }

  const stripRoot = document.querySelector("[data-phone-strip]");
  if (stripRoot) {
    const slots = [...stripRoot.querySelectorAll("[data-phone-slot]")];
    const readout = stripRoot.querySelector("[data-phone-strip-val]");

    function pickSlot(target) {
      slots.forEach((slot) => {
        const active = slot === target;
        slot.toggleAttribute("data-active", active);
        if (active) {
          slot.dataset.penna = "phone.pill.icon";
          slot.setAttribute("aria-current", "true");
        } else {
          slot.removeAttribute("data-penna");
          slot.setAttribute("aria-current", "false");
        }
      });
      if (readout) readout.textContent = target.dataset.phoneSlotLabel || "";
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(stripRoot));
    }

    slots.forEach((slot) => {
      slot.addEventListener("click", () => pickSlot(slot));
    });
  }

  const stackRoot = document.querySelector("[data-phone-stack-demo]");
  if (stackRoot) {
    const stack = stackRoot.querySelector(".penna-stack");
    const disc = stackRoot.querySelector("[data-phone-stack-toggle]");
    const btn = stackRoot.querySelector("[data-phone-stack-btn]");
    const readout = stackRoot.querySelector("[data-phone-stack-val]");
    const chips = [...stackRoot.querySelectorAll(".penna-stack-chip")];

    function setOpen(open) {
      if (!stack) return;
      stack.toggleAttribute("data-open", open);
      if (btn) btn.querySelector(".penna-label").textContent = open ? "Close stack" : "Open stack";
      if (open) requestAnimationFrame(() => window.pennaKit?.roundAll?.(stack));
    }

    disc?.addEventListener("click", () => setOpen(!stack?.hasAttribute("data-open")));
    btn?.addEventListener("click", () => setOpen(!stack?.hasAttribute("data-open")));
    stack?.querySelector(".penna-stack-scrim")?.addEventListener("click", () => setOpen(false));

    chips.forEach((chip) => {
      chip.addEventListener("click", () => {
        chips.forEach((other) => {
          if (other === chip) other.dataset.state = "selected";
          else other.removeAttribute("data-state");
        });
        const label = chip.querySelector("span")?.textContent?.trim() || "";
        if (readout) readout.textContent = `Selected · ${label}`;
      });
    });
  }

  const kbdRule = document.querySelector("[data-phone-kbd-rule]");
  if (kbdRule) {
    const toggle = kbdRule.querySelector("[data-phone-kbd-switch]");
    const docks = [...kbdRule.querySelectorAll("[data-phone-dock]")];
    const readout = kbdRule.querySelector("[data-phone-kbd-val]");
    const input = kbdRule.querySelector("[data-phone-compose-input]");
    const send = kbdRule.querySelector("[data-phone-compose-send]");

    function setKeyboard(up) {
      docks.forEach((dock) => {
        dock.hidden = (dock.dataset.phoneDock === "composer") !== up;
      });
      if (readout) readout.textContent = up ? "Composer · 8 px above keyboard" : "Bottom strip · 56 px at rest";
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(kbdRule));
    }

    toggle?.addEventListener("change", (event) => setKeyboard(Boolean(event.detail.checked)));

    function syncSend() {
      if (!input || !send) return;
      const hasText = input.value.trim().length > 0;
      send.disabled = !hasText;
      if (hasText) {
        send.dataset.emphasis = "solid";
        send.dataset.solid = "primary";
      } else {
        delete send.dataset.emphasis;
        delete send.dataset.solid;
      }
      requestAnimationFrame(() => window.pennaKit?.roundAll?.(send));
    }

    input?.addEventListener("input", syncSend);
    send?.addEventListener("click", () => {
      if (!input || !input.value.trim()) return;
      input.value = "";
      syncSend();
      if (readout) readout.textContent = "Sent · composer stays 8 px above keyboard";
    });
  }

  const termRoot = document.querySelector("[data-phone-term]");
  if (termRoot) {
    const readout = termRoot.querySelector("[data-phone-term-val]");
    const mods = [...termRoot.querySelectorAll("[data-phone-mod]")];
    const arrows = termRoot.querySelector("[data-phone-arrows]");

    function describe() {
      const latched = mods.filter((m) => m.dataset.penna === "phone.pill.icon").map((m) => m.dataset.phoneMod);
      const parts = [];
      if (latched.length) parts.push(`${latched.join(" + ")} latched`);
      if (arrows?.hasAttribute("data-armed")) parts.push("Arrows armed");
      if (readout) readout.textContent = parts.length ? parts.join(" · ") : "Rest";
    }

    mods.forEach((mod) => {
      mod.addEventListener("click", () => {
        const on = mod.dataset.penna === "phone.pill.icon";
        mod.dataset.penna = on ? "phone.key" : "phone.pill.icon";
        mod.setAttribute("aria-pressed", String(!on));
        describe();
      });
    });

    arrows?.addEventListener("click", () => {
      const armed = arrows.toggleAttribute("data-armed");
      arrows.setAttribute("aria-pressed", String(armed));
      describe();
    });

    termRoot.querySelectorAll("[data-phone-tap]").forEach((btn) => {
      btn.addEventListener("click", () => {
        if (readout) readout.textContent = `${btn.dataset.phoneTap} sent`;
      });
    });
  }

  const chordsRoot = document.querySelector("[data-phone-chords]");
  if (chordsRoot) {
    const chips = [...chordsRoot.querySelectorAll(".penna-stack-chip")];
    const readout = chordsRoot.querySelector("[data-phone-chord-val]");
    chips.forEach((chip) => {
      chip.addEventListener("click", () => {
        chips.forEach((other) => {
          if (other === chip) other.dataset.state = "selected";
          else other.removeAttribute("data-state");
        });
        const label = chip.dataset.phoneChord || "";
        if (readout) readout.textContent = label;
      });
    });
  }

  const findRoot = document.querySelector("[data-phone-find]");
  if (findRoot) {
    const count = findRoot.querySelector(".penna-find-count");
    const query = findRoot.querySelector(".penna-find-query");
    const prev = findRoot.querySelector("[data-phone-find-prev]");
    const next = findRoot.querySelector("[data-phone-find-next]");
    const close = findRoot.querySelector("[data-phone-find-close]");
    const total = 7;
    let index = 3;

    function syncFind() {
      if (count) count.textContent = `${index} of ${total}`;
    }

    prev?.addEventListener("click", () => {
      index = index <= 1 ? total : index - 1;
      syncFind();
    });
    next?.addEventListener("click", () => {
      index = index >= total ? 1 : index + 1;
      syncFind();
    });
    close?.addEventListener("click", () => {
      index = 1;
      if (query) query.textContent = "pill-find";
      syncFind();
    });
  }

  const browserRoot = document.querySelector("[data-phone-browser]");
  if (browserRoot) {
    const stackChips = [...browserRoot.querySelectorAll("[data-phone-browser-stack] .penna-stack-chip:not(:disabled)")];
    const stackVal = browserRoot.querySelector("[data-phone-browser-val]");
    stackChips.forEach((chip) => {
      chip.addEventListener("click", () => {
        stackChips.forEach((other) => {
          if (other === chip) other.dataset.state = "selected";
          else other.removeAttribute("data-state");
        });
        if (stackVal) stackVal.textContent = chip.getAttribute("aria-label") || chip.querySelector("span")?.textContent?.trim() || "";
      });
    });

    const addrText = browserRoot.querySelector("[data-phone-addr-text]");
    const clearBtn = browserRoot.querySelector("[data-phone-addr-clear]");
    const resetBtn = browserRoot.querySelector("[data-phone-addr-reset]");
    browserRoot.querySelectorAll("[data-phone-suggest]").forEach((row) => {
      const pick = () => {
        if (!addrText) return;
        addrText.textContent = row.dataset.phoneSuggest || "";
        addrText.removeAttribute("data-empty");
      };
      row.addEventListener("click", pick);
      row.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        pick();
      });
    });
    clearBtn?.addEventListener("click", () => {
      if (!addrText) return;
      addrText.textContent = "Search or enter address";
      addrText.dataset.empty = "";
    });
    resetBtn?.addEventListener("click", () => {
      if (!addrText) return;
      addrText.textContent = "penna.dev/p";
      addrText.removeAttribute("data-empty");
    });
  }

  const cornersRoot = document.querySelector("[data-phone-corners]");
  if (cornersRoot) {
    const control = cornersRoot.querySelector("[data-phone-corners-control]");
    const stage = cornersRoot.querySelector("[data-phone-corners-stage]");
    const readout = cornersRoot.querySelector("[data-phone-corners-val]");
    const LOOKS = [
      { look: "default", label: "Rounded · radius 12" },
      { look: "pill", label: "Pill · control-radius full" },
    ];

    function applyCorners(index) {
      const entry = LOOKS[index] ?? LOOKS[0];
      if (stage) {
        stage.dataset.look = entry.look;
        requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
      }
      if (readout) readout.textContent = entry.label;
    }

    control?.addEventListener("change", (event) => applyCorners(event.detail.index));
    document.addEventListener("penna-theme", () => {
      if (stage) requestAnimationFrame(() => window.pennaKit?.roundAll?.(stage));
    });
    applyCorners(0);
  }

  document.querySelectorAll("[data-copy-token]").forEach((btn) => {
    if (btn.dataset.copyWired) return;
    btn.dataset.copyWired = "1";
    let timer = null;
    const original = btn.textContent;
    btn.addEventListener("click", () => {
      const value = btn.dataset.copyToken;
      if (!value) return;
      navigator.clipboard?.writeText(value).catch(() => {});
      clearTimeout(timer);
      btn.dataset.copied = "";
      btn.textContent = "Copied";
      timer = setTimeout(() => {
        delete btn.dataset.copied;
        btn.textContent = original;
      }, 1200);
    });
  });
})();
