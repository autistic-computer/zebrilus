import init, * as wasm from "../pkg/zz_dom.js";

export const state = {
  client: null,
  socket: null,
  host: null,
  hosts: [],
  hostId: "local",
  showcase: false,
  tree: null,
  appearance: null,
  cell: { width: 8, height: 16 },
  remembered: "",
  connected: false,
  chatView: false,
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

let flushing = false;

function receiveFrame(client, bytes) {
  let events;
  try {
    events = JSON.parse(client.receive(bytes));
  } catch (error) {
    status(String(error));
    return [];
  }
  return events;
}

function flushHost(entry, active) {
  if (!entry.host) return;
  while (true) {
    let progressed = false;
    let outbound;
    while ((outbound = entry.client.outbound())) {
      progressed = true;
      entry.host.send(outbound);
    }
    let inbound;
    while ((inbound = entry.host.outbound())) {
      progressed = true;
      const events = receiveFrame(entry.client, inbound);
      for (const event of events) {
        if (event.type === "hello") entry.connected = true;
        if (event.type === "attached") entry.remembered = `$${event.session}`;
        if (active) emit(event.type, event);
      }
    }
    if (!progressed) break;
  }
}

export function flush() {
  const { client, socket, host } = state;
  if (!client) return;
  if (host) {
    if (flushing) return;
    flushing = true;
    try {
      for (const entry of state.hosts) {
        flushHost(entry, entry.id === state.hostId);
      }
    } finally {
      flushing = false;
    }
    return;
  }
  let message;
  while ((message = client.outbound())) {
    if (socket?.readyState === WebSocket.OPEN) socket.send(message);
  }
}

export function switchHost(id, after) {
  const next = state.hosts.find((entry) => entry.id === id);
  if (!next) return;
  const changed = state.hostId !== next.id;
  if (changed) {
    state.hostId = next.id;
    state.client = next.client;
    state.host = next.host;
    state.connected = next.connected;
    state.client.color_scheme(dark());
    state.client.focus(document.hasFocus());
    emit("hostSwitched", { id: next.id });
    emit("appearance");
  }
  if (after) after(next.client);
  flush();
  markTree();
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
    if (state.client.set_chat_view(state.chatView)) flush();
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
  if (typeof wasm.ShowcaseHost === "function") {
    state.showcase = true;
    const specs = typeof wasm.ShowcaseHost.hosts === "function"
      ? JSON.parse(wasm.ShowcaseHost.hosts())
      : [{ id: "local", name: "This computer", local: true }];
    state.hosts = specs.map((spec, index) => {
      const client = new wasm.Client(Math.floor(Math.random() * 2 ** 52) + index + 1);
      client.set_host_name(spec.name);
      const host = new wasm.ShowcaseHost(spec.id);
      return { id: spec.id, name: spec.name, local: spec.local, client, host, connected: false, remembered: "" };
    });
    const primary = state.hosts[0];
    state.hostId = primary.id;
    state.client = primary.client;
    state.host = primary.host;
    status("Attaching…");
    for (const entry of state.hosts) {
      entry.client.hello("", dark());
    }
    flush();
    for (const entry of state.hosts) {
      entry.client.sidebar_hello(Date.now() / 1000);
    }
    markTree();
    return;
  }
  const client = new wasm.Client(Math.floor(Math.random() * 2 ** 52) + 1);
  client.set_host_name("This computer");
  state.client = client;
  state.hostId = "local";
  state.hosts = [{ id: "local", name: "This computer", local: true, client, host: null, connected: false, remembered: "" }];
  const socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`);
  state.socket = socket;
  socket.binaryType = "arraybuffer";
  socket.onopen = () => {
    status("Attaching…");
    state.client.hello(state.remembered, dark());
    flush();
  };
  socket.onmessage = (message) => {
    const events = receiveFrame(state.client, new Uint8Array(message.data));
    flush();
    for (const event of events) emit(event.type, event);
  };
  socket.onclose = () => {
    state.connected = false;
    if (state.hosts[0]) state.hosts[0].connected = false;
    status("Disconnected. Retrying…");
    emit("disconnected");
    setTimeout(connect, 1500);
  };
}

on("hello", () => {
  state.connected = true;
  const active = state.hosts.find((entry) => entry.id === state.hostId);
  if (active) active.connected = true;
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
    for (const entry of state.hosts) entry.client.color_scheme(dark());
    flush();
    emit("theme");
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-penna-mode", "data-penna-theme"] });
  globalThis.zzDom = { state, emit, on, command, switchHost };
  connect();
}
