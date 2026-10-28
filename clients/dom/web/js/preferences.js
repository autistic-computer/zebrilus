import init, * as engine from "../theme/zz_dom_theme.js";
import { emit } from "./core.js";

const MIRROR = "zz-dom-preferences";
const root = document.documentElement;
const media = matchMedia("(prefers-color-scheme: dark)");
const listeners = new Set();
const started = performance.now();
export const preferences = {};
let ready = false;
let frame = 0;
let icons = "";
let iconsKey = "";

function stored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function replace(next) {
  for (const key of Object.keys(preferences)) if (!(key in next)) delete preferences[key];
  Object.assign(preferences, next);
}

function mirror() {
  try {
    localStorage.setItem(MIRROR, JSON.stringify(preferences));
  } catch {}
}

try {
  await init();
  const legacy = stored(MIRROR);
  if (stored(engine.storage_key()) === null && legacy) engine.save(legacy);
  replace(JSON.parse(engine.load()));
  mirror();
  ready = true;
} catch (error) {
  console.error("zz theme engine did not load", error);
}

export const timing = { load: Math.round((performance.now() - started) * 10) / 10, render: 0 };

export function get() {
  return preferences;
}

export function loaded() {
  return ready;
}

export function dark() {
  const mode = preferences.mode || "system";
  return mode === "system" ? media.matches : mode === "dark";
}

export function subscribe(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function changed(previous) {
  mirror();
  schedule();
  for (const listener of listeners) listener(preferences, previous);
  emit("preferences", { preferences, previous });
}

export function set(changes) {
  if (!ready) return preferences;
  const previous = { ...preferences };
  replace(JSON.parse(engine.save(JSON.stringify({ ...preferences, ...changes }))));
  changed(previous);
  return preferences;
}

export const setPreferences = set;

export function themeAction(action, argument = "") {
  if (!ready) return { error: "The theme engine did not load." };
  const previous = { ...preferences };
  const outcome = JSON.parse(engine.theme_action(JSON.stringify(preferences), action, argument));
  replace(outcome.preferences);
  if (!outcome.error) changed(previous);
  return outcome;
}

export function picks() {
  return ready ? JSON.parse(engine.picks(JSON.stringify(preferences), dark())) : null;
}

export function sections() {
  return ready ? JSON.parse(engine.sections()) : [];
}

export function about(platform) {
  return ready ? JSON.parse(engine.about(platform)) : {};
}

export function accentHex(value) {
  return ready ? engine.accent_hex(value) : undefined;
}

function sheet(id, kit) {
  let style = document.getElementById(id);
  if (!style) {
    style = document.createElement("style");
    style.id = id;
    const link = document.getElementById(kit);
    if (link) link.before(style);
    else document.head.prepend(style);
  }
  const link = document.getElementById(kit);
  if (link) link.disabled = true;
  return style;
}

function write(id, kit, css) {
  const style = sheet(id, kit);
  if (style.textContent !== css) style.textContent = css;
}

function apply() {
  frame = 0;
  if (!ready) return;
  const begun = performance.now();
  const json = JSON.stringify(preferences);
  const isDark = dark();
  write("zz-live-tokens", "zz-tokens", engine.stylesheet(json, isDark));
  const key = JSON.stringify([preferences.theme, preferences.themes, preferences.radius]);
  if (key !== iconsKey) {
    iconsKey = key;
    icons = engine.icons(json);
  }
  write("zz-live-icons", "zz-icons", icons);
  const theme = preferences.theme || "default";
  const mode = isDark ? "dark" : "light";
  if (globalThis.zzDesign) Object.assign(globalThis.zzDesign.choice, { theme, mode });
  if (root.dataset.zzTheme !== theme) root.dataset.zzTheme = theme;
  if (root.dataset.zzMode !== mode) root.dataset.zzMode = mode;
  root.style.zoom = preferences.zoom === 1 ? "" : String(preferences.zoom);
  root.toggleAttribute("data-zz-still", !preferences.animations);
  root.style.setProperty("--zz-animation-speed", String(preferences.animation_speed));
  timing.render = Math.round((performance.now() - begun) * 10) / 10;
  requestAnimationFrame(() => globalThis.zzDesign?.roundAll());
}

function schedule() {
  if (!frame) frame = requestAnimationFrame(apply);
}

media.addEventListener("change", () => {
  if ((preferences.mode || "system") === "system") changed({ ...preferences });
});

apply();
globalThis.zzPreferences = { preferences, get, set, setPreferences, subscribe, dark, themeAction, timing };
