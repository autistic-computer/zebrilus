(() => {
  function wire(root) {
    root.querySelectorAll(".penna-snippet").forEach((snippet) => {
      if (snippet.dataset.snippetWired) return;
      snippet.dataset.snippetWired = "1";
      const btn = snippet.querySelector("[data-snippet-copy]");
      const code = snippet.querySelector(".penna-snippet-code");
      if (!btn || !code) return;
      let timer = 0;
      btn.addEventListener("click", async () => {
        const text = code.textContent?.trim() || "";
        try {
          await navigator.clipboard.writeText(text);
        } catch {}
        const icon = btn.querySelector("[data-icon]");
        btn.setAttribute("data-copied", "");
        if (icon) icon.dataset.icon = "check";
        clearTimeout(timer);
        timer = setTimeout(() => {
          btn.removeAttribute("data-copied");
          if (icon) icon.dataset.icon = "copy";
        }, 1400);
      });
    });
  }

  document.addEventListener("DOMContentLoaded", () => wire(document));
  window.pennaSnippet = { wire };
})();
