(() => {
  const TARGET = "penna-docs-target";
  let storybook;

  function knobs(wasm) {
    const { theme, mode } = pennaKit.choice;
    wasm.set_knobs(new URLSearchParams({ "interface-theme": theme, theme: mode }).toString());
  }

  function load() {
    const url = document.querySelector("[data-docs-storybook]")?.dataset.docsStorybook;
    if (!url) return Promise.reject(new Error("no storybook build"));
    storybook ??= (async () => {
      const wasm = await import(new URL(url, document.baseURI).href);
      await wasm.default();
      wasm.run();
      while (!wasm.is_ready()) await new Promise((done) => requestAnimationFrame(done));
      knobs(wasm);
      return wasm;
    })();
    return storybook;
  }

  async function mount(example) {
    const target = example.querySelector(".penna-docs-mount:not([data-mounted])");
    if (!target) return;
    target.toggleAttribute("data-mounted", true);
    const wasm = await load();
    wasm.mount(target.dataset.story, target.dataset.section, `#${target.id}`);
  }

  function show(example, native) {
    example.querySelectorAll(".penna-docs-stage").forEach((stage) => {
      stage.hidden = (stage.dataset.target === "native") !== native;
    });
    if (native) mount(example).catch((error) => console.error(error));
  }

  function pick(control, index) {
    const segment = control.querySelectorAll(".penna-segment")[index];
    if (segment && String(index) !== control.dataset.value) segment.click();
  }

  function remembered() {
    try {
      return localStorage.getItem(TARGET) === "native" ? 1 : 0;
    } catch {
      return 0;
    }
  }

  function wire(example) {
    const tabs = example.querySelector(".penna-docs-tabs .penna-segmented");
    const panes = [...example.querySelectorAll(".penna-docs-code > [data-tab]")];
    tabs?.addEventListener("change", (event) => {
      panes.forEach((pane, index) => (pane.hidden = index !== event.detail.index));
    });
    const targets = example.querySelector(".penna-docs-targets .penna-segmented");
    targets?.addEventListener("change", (event) => {
      const native = event.detail.index === 1;
      try {
        localStorage.setItem(TARGET, native ? "native" : "html");
      } catch {}
      show(example, native);
      document
        .querySelectorAll(".penna-docs-targets .penna-segmented")
        .forEach((other) => other !== targets && pick(other, event.detail.index));
    });
  }

  document.querySelectorAll(".penna-docs-example").forEach(wire);

  if (remembered() === 1) {
    addEventListener("load", () => {
      document.querySelectorAll(".penna-docs-targets .penna-segmented").forEach((control) => pick(control, 1));
    });
  }

  document.addEventListener("penna-theme", () => {
    storybook?.then(knobs).catch(() => {});
  });

  document.querySelectorAll("[data-docs-menu]").forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelector(".penna-docs-side")?.toggleAttribute("data-open");
    });
  });

  document.querySelectorAll("[data-docs-mode]").forEach((button) => {
    button.addEventListener("click", () => {
      pennaKit.choice.mode = pennaKit.choice.mode === "dark" ? "light" : "dark";
      pennaKit.apply();
    });
  });
})();
