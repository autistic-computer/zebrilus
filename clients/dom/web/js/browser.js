import { paneRenderer } from "./workspace.js";

const REASON = "Embedded Chromium needs the desktop app. Open this page in a browser tab.";

function control(icon, action) {
  return `<button data-penna="browser.button" data-action="${action}" title="Requires the desktop browser runtime" disabled><i data-icon="${icon}"></i></button>`;
}

function browser(pane) {
  pane.body.innerHTML = `
    <div class="browser-toolbar">
      <div class="browser-cluster">${control("arrow-left", "back")}${control("arrow-right", "forward")}${control("redo-2", "reload")}</div>
      <div class="browser-url penna-muted"></div>
      <div class="browser-cluster">${control("inspector", "picker")}${control("ellipsis", "more")}</div>
    </div>
    <div class="browser-message">
      <i data-icon="globe"></i>
      <div class="browser-title" data-text>Browser</div>
      <div class="penna-muted reason" aria-label="${REASON}">${REASON}</div>
      <button data-penna="button" class="browser-open" hidden><i data-icon="external-link"></i><span data-text>Open in browser tab</span></button>
    </div>`;
  const url = pane.body.querySelector(".browser-url");
  const open = pane.body.querySelector(".browser-open");
  open.addEventListener("click", () => {
    const target = pane.info?.browser?.url;
    if (target) window.open(target, "_blank", "noopener");
  });
  return {
    update() {
      const target = pane.info?.browser?.url || "";
      url.textContent = target || "Browser";
      url.setAttribute("aria-label", url.textContent);
      open.hidden = !target;
      open.setAttribute("aria-label", "Open in browser tab");
    },
    layout() {},
    destroy() {},
  };
}

paneRenderer("browser", browser);
