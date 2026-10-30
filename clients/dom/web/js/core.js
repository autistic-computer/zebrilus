import init, { Client } from "../pkg/zz_dom.js";

export const state = {
  client: null,
  socket: null,
  tree: null,
  appearance: null,
  cell: { width: 8, height: 16 },
  remembered: "",
  connected: false,
};

const handlers = new Map();
export const keyHandlers = [];

export function onKey(handler) {
  keyHandlers.unshift(handler);
}
const phases = { tree: [], panes: [], after: [] };
const dirtyPanes = new Set();
let treeDirty = false;
let frame = 0;

export const $ = (selector, root = document) => root.querySelector(selector);

export function on(type, handler) {
  if (!handlers.has(type)) handlers.set(type, []);
  handlers.get(type).push(handler);
}

export function emit(type, event = {}) {
  for (const handler of handlers.get(type) || []) handler(event);
  for (const handler of handlers.get("*") || []) handler({ type, ...event });
}

export function phase(name, handler) {
  phases[name].push(handler);
}

export function flush() {
  const { client, socket } = state;
  if (!client) return;
  let message;
  while ((message = client.outbound())) {
    if (socket?.readyState === WebSocket.OPEN) socket.send(message);
  }
}

export function command(name, args = []) {
  const id = state.client?.command(name, args);
  flush();
  return id;
}

export function schedule() {
  if (!frame) frame = requestAnimationFrame(render);
}

export function markTree() {
  treeDirty = true;
  schedule();
}

export function markPane(id) {
  dirtyPanes.add(id);
  schedule();
}

export function markAllPanes(ids) {
  for (const id of ids) dirtyPanes.add(id);
  schedule();
}

function render() {
  frame = 0;
  if (treeDirty && state.client) {
    treeDirty = false;
    state.tree = JSON.parse(state.client.tree());
    for (const handler of phases.tree) handler(state.tree);
  }
  const panes = [...dirtyPanes];
  dirtyPanes.clear();
  for (const handler of phases.panes) handler(panes);
  for (const handler of phases.after) handler();
}

export const dark = () => document.documentElement.dataset.pennaMode !== "light";

export function session() {
  const tree = state.tree;
  return tree?.sessions.find((entry) => entry.id === tree.attached) || tree?.sessions[0];
}

export function activeWindow() {
  return session()?.windows.find((window) => window.focused);
}

export function activePane() {
  return activeWindow()?.activePane;
}

export function status(text) {
  emit("status", { text });
}

function connect() {
  state.client = new Client(Math.floor(Math.random() * 2 ** 52) + 1);
  const socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`);
  state.socket = socket;
  socket.binaryType = "arraybuffer";
  socket.onopen = () => {
    status("Attaching…");
    state.client.hello(state.remembered, dark());
    flush();
  };
  socket.onmessage = (message) => {
    let events;
    try {
      events = JSON.parse(state.client.receive(new Uint8Array(message.data)));
    } catch (error) {
      status(String(error));
      return;
    }
    flush();
    for (const event of events) emit(event.type, event);
  };
  socket.onclose = () => {
    state.connected = false;
    status("Disconnected. Retrying…");
    emit("disconnected");
    setTimeout(connect, 1500);
  };
}

on("hello", () => {
  state.connected = true;
  emit("appearance");
});

on("attached", (event) => {
  state.remembered = `$${event.session}`;
  status("Connected");
  state.client.color_scheme(dark());
  state.client.focus(document.hasFocus());
  flush();
  emit("appearance");
  markTree();
});

on("tree", markTree);
on("paneRemoved", markTree);
on("viewport", (event) => markPane(event.pane));

export async function start() {
  await init();
  new MutationObserver(() => {
    if (!state.client) return;
    state.client.color_scheme(dark());
    flush();
    emit("theme");
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-penna-mode", "data-penna-theme"] });
  globalThis.zzDom = { state, emit, on, command };
  connect();
}
