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

  const linear = (value) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  const encode = (value) => (value <= 0.0031308 ? 12.92 * value : 1.055 * value ** (1 / 2.4) - 0.055);

  const oklab = (color) => {
    const [r, g, b] = color.map(linear);
    const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    return [
      0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
      1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
      0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ];
  };

  const srgb = ([lightness, a, b]) => {
    const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
    const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
    const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
    return [
      4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
      -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
      -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ].map((value) => Math.round(encode(Math.min(Math.max(value, 0), 1)) * 255));
  };

  const pole = (color) => ((Math.max(...color) + Math.min(...color)) / 2 < 0.5 ? [1, 1, 1] : [0, 0, 0]);

  const toward = (ink, rest, moved) => {
    const [from, to, end] = [rest, moved, pole(rest)].map(oklab);
    const amount = Math.abs(end[0] - from[0]) < 1e-6 ? 0 : (to[0] - from[0]) / (end[0] - from[0]);
    const [start, target] = [ink, pole(ink)].map(oklab);
    return srgb(start.map((value, index) => value + (target[index] - value) * amount));
  };

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
