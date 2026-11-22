import { on } from "./core.js";
import { preferences } from "./preferences.js";

const MIN_CHIME_GAP_MS = 2000;
const MIN_BELL_GAP_MS = 200;

let lastChime = 0;
let lastBell = 0;
let everConnected = false;

function play(name, gapMs, isBell = false) {
  if (preferences.sounds === false) return;
  const now = performance.now();
  const last = isBell ? lastBell : lastChime;
  if (last && now - last < gapMs) return;
  if (isBell) lastBell = now;
  else lastChime = now;
  const audio = new Audio(`sounds/${name}.wav`);
  audio.play().catch(() => {});
}

function notify(attention) {
  if (preferences.notifications === false || !document.hidden) return;
  if (typeof Notification === "undefined" || Notification.permission !== "granted") return;
  const labels = {
    request: "An agent pane needs your input",
    failed: "An agent pane failed",
    done: "An agent pane finished",
  };
  const body = labels[attention];
  if (!body) return;
  try {
    new Notification("zz", { body });
  } catch {}
}

on("hello", () => {
  everConnected = true;
});

on("bell", () => play("bell", MIN_BELL_GAP_MS, true));

on("disconnected", () => {
  if (everConnected) play("disconnected", MIN_CHIME_GAP_MS);
});

on("agentSubmitted", () => play("sent", MIN_BELL_GAP_MS));

on("agentState", (event) => {
  const attention = event?.attention;
  if (!attention) return;
  if (attention === "request" || attention === "failed" || attention === "done") {
    play(attention, MIN_CHIME_GAP_MS);
    notify(attention);
  }
});
