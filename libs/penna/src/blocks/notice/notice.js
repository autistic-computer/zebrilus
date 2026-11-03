(() => {
  const probe = new OffscreenCanvas(1, 1).getContext("2d");

  const channels = (color) => {
    probe.clearRect(0, 0, 1, 1);
    probe.fillStyle = color;
    probe.fillRect(0, 0, 1, 1);
    return [...probe.getImageData(0, 0, 1, 1).data.slice(0, 3)].map((value) => value / 255);
  };

  const STATES = ["", "hover", "active"];

  const fillOf = (tone, state) => {
    const probe = document.createElement("span");
    probe.hidden = true;
    probe.dataset.penna = "button";
    probe.toggleAttribute("data-prominent", true);
    if (tone) probe.dataset.tone = tone;
    if (state) probe.dataset.state = state;
    document.body.append(probe);
    const fill = getComputedStyle(probe).getPropertyValue("--penna-fill").trim();
    probe.remove();
    return channels(fill);
  };

  const toward = (ink, rest, moved) =>
    ink.map((value, index) => {
      const step = moved[index] - rest[index];
      const wash = step > 0 ? 1 : 0;
      const room = wash - rest[index];
      const share = room === 0 ? 0 : step / room;
      return Math.round((value + (wash - value) * share) * 255);
    });

  function ground(root = document) {
    const page = getComputedStyle(document.documentElement);
    const foreground = channels(page.getPropertyValue("--penna-primary").trim()).join() === channels(page.getPropertyValue("--penna-fg").trim()).join();
    root.querySelectorAll(".penna-notice .penna-button[data-prominent]").forEach((button) => {
      const carrier = button.closest(".penna-notice-tone");
      if (!carrier || !foreground) {
        button.removeAttribute("data-on");
        return;
      }
      const ink = channels(getComputedStyle(carrier).getPropertyValue("--penna-ink").trim());
      const [rest, ...moved] = STATES.map((state) => fillOf(button.dataset.tone, state));
      ["", "-hover", "-active"].forEach((suffix, index) => {
        const color = index === 0 ? ink.map((value) => Math.round(value * 255)) : toward(ink, rest, moved[index - 1]);
        button.style.setProperty(`--penna-on-fill${suffix}`, `rgb(${color.join(" ")})`);
      });
      button.setAttribute("data-on", "");
    });
  }

  function settle() {
    ground();
  }

  settle();
  document.fonts.ready.then(settle);
  document.fonts.addEventListener("loadingdone", settle);
  document.addEventListener("load", (event) => event.target.id === "penna-tokens" && settle(), true);
})();
