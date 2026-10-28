import { activePane, emit, flush, keyHandlers, markPane, on, state } from "./core.js";

export { onKey } from "./core.js";
import { activeView, bindIme, uploadImage } from "./terminal.js";

const ime = document.createElement("textarea");
ime.className = "ime";
ime.autocapitalize = "off";
ime.autocomplete = "off";
ime.spellcheck = false;
ime.setAttribute("aria-label", "Terminal input");
document.body.append(ime);
bindIme(ime);

const MODIFIERS = ["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead", "Process", "Unidentified", "Fn", "OS"];

export function focusTerminal() {
  ime.focus({ preventScroll: true });
}

function target() {
  const pane = activePane();
  if (pane === undefined || !state.client) return null;
  const view = activeView();
  return { pane, output: view?.pane.id === pane ? view.output : false, view: view?.pane.id === pane ? view : null };
}

function typeText(text, composed) {
  const at = target();
  if (!at || !text) return;
  if (state.client.type_text(at.pane, at.output, text, composed)) markPane(at.pane);
  at.view?.resetBlink();
  flush();
}

ime.addEventListener("input", (event) => {
  if (event.isComposing) return;
  typeText(ime.value, false);
  ime.value = "";
});

ime.addEventListener("compositionstart", () => activeView()?.compose(""));
ime.addEventListener("compositionupdate", (event) => activeView()?.compose(event.data));
ime.addEventListener("compositionend", (event) => {
  activeView()?.compose("");
  typeText(event.data, true);
  ime.value = "";
});

document.querySelector("#workspace").addEventListener("mouseup", () => {
  if (getSelection()?.toString()) return;
  if (document.activeElement?.closest?.(".find")) return;
  focusTerminal();
});

function keyName(event) {
  if (event.key === " ") return " ";
  return event.key.length === 1 ? event.key.toLowerCase() : event.key;
}

function keyText(event) {
  if ((event.metaKey || event.ctrlKey) && !(event.ctrlKey && event.altKey)) return null;
  return event.key.length === 1 ? event.key : null;
}

function modifiers(event) {
  return [event.shiftKey, event.ctrlKey, event.altKey, event.metaKey];
}

document.addEventListener("keydown", (event) => {
  for (const handler of keyHandlers) if (handler(event)) return;
  const inField = event.target !== ime && event.target.closest?.("input, textarea, select");
  if (inField || event.isComposing || event.keyCode === 229 || MODIFIERS.includes(event.key)) return;
  const at = target();
  if (!at) return;
  at.view?.resetBlink();
  const response = JSON.parse(
    state.client.key_down(at.pane, at.output, keyName(event), keyText(event), ...modifiers(event), event.repeat),
  );
  flush();
  if (response.focusSearch) {
    event.preventDefault();
    at.view?.searchInput()?.focus({ preventScroll: true });
    return;
  }
  if (response.redraw) markPane(at.pane);
  if (response.action) {
    if (response.action !== "paste") event.preventDefault();
    if (response.action === "search") at.view?.openSearch();
    if (response.action === "fontIncrease") at.view?.font(1);
    if (response.action === "fontDecrease") at.view?.font(-1);
    return;
  }
  if (response.sent || response.stop) {
    event.preventDefault();
    return;
  }
  if (response.text && event.target !== ime) focusTerminal();
});

document.addEventListener("keyup", (event) => {
  const inField = event.target !== ime && event.target.closest?.("input, textarea, select");
  if (inField || MODIFIERS.includes(event.key)) return;
  const at = target();
  if (!at) return;
  if (state.client.key_up(at.pane, at.output, keyName(event), ...modifiers(event))) {
    event.preventDefault();
    flush();
  }
});

document.addEventListener("paste", (event) => {
  const at = target();
  if (!at || (event.target !== ime && event.target.closest?.("input, textarea"))) return;
  const image = [...(event.clipboardData?.files || [])].find((file) => file.type.startsWith("image/"));
  if (image && !at.output) {
    event.preventDefault();
    uploadImage(at.pane, image);
    return;
  }
  const text = event.clipboardData?.getData("text/plain");
  if (!text) return;
  event.preventDefault();
  state.client.terminal_paste(at.pane, at.output, text);
  flush();
  markPane(at.pane);
});

for (const [type, focused] of [
  ["focus", true],
  ["blur", false],
]) {
  addEventListener(type, () => {
    state.client?.focus(focused);
    flush();
    emit("focus", { focused });
  });
}

on("attached", focusTerminal);
on("focusTerminal", focusTerminal);
on("commandOutput", () => {
  if (state.client?.command_output_pane() != null) focusTerminal();
});
