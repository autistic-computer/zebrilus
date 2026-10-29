import { getElementContext } from "react-grab/primitives";
import { fallbackContext, reactGrabContext } from "./format";

const API_NAME = "__alaCefElementPicker";
const QUERY_NAME = "__alaCefElementPickerQuery";
const OVERLAY_ATTRIBUTE = "data-ala-cef-element-picker";
const CONTEXT_TIMEOUT_MS = 9_000;

interface PickGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  scrollX: number;
  scrollY: number;
  viewportWidth: number;
  viewportHeight: number;
  visualViewport?: { offsetLeft: number; offsetTop: number; width: number; height: number };
}

interface ElementPickerAppearance {
  highlightOutline: string;
  highlightFill: string;
  highlightContrast: string;
  previewBackground: string;
  previewForeground: string;
  previewBorder: string;
  shadow: string | null;
  radius: number;
  fontFamily: string;
  pageZoom: number;
}

type PickMessage =
  | { version: 1; kind: "picked"; token: string; text: string; geometry?: PickGeometry }
  | { version: 1; kind: "cancelled"; token: string }
  | { version: 1; kind: "failed"; token: string };

type QueryFunction = (options: {
  request: string;
  persistent?: boolean;
  onSuccess?: (response: string) => void;
  onFailure?: (code: number, message: string) => void;
}) => void;

interface PickerApi {
  start(token: string, appearance: ElementPickerAppearance): void;
  cancel(): void;
  point(x: number, y: number, width: number): void;
  release(): void;
}

interface HeldPicker {
  point(x: number, y: number, width: number): void;
  release(): void;
}

declare global {
  interface Window {
    __alaCefElementPicker?: PickerApi;
    __alaCefElementPickerQuery?: QueryFunction;
  }
}

let cleanupActivePicker: (() => void) | null = null;
let heldPicker: HeldPicker | null = null;

function send(message: PickMessage): void {
  const query = window[QUERY_NAME as keyof Window] as QueryFunction | undefined;
  if (typeof query !== "function") return;
  query({
    request: JSON.stringify(message),
    onSuccess: () => undefined,
    onFailure: () => {
      if (message.kind !== "picked") return;
      query({
        request: JSON.stringify({ version: 1, kind: "failed", token: message.token }),
        onSuccess: () => undefined,
        onFailure: () => undefined,
      });
    },
  });
}

function geometryFor(element: Element): PickGeometry | undefined {
  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return undefined;
  return {
    x: rect.left,
    y: rect.top,
    width: rect.width,
    height: rect.height,
    scrollX: window.scrollX,
    scrollY: window.scrollY,
    viewportWidth: window.innerWidth,
    viewportHeight: window.innerHeight,
    ...(window.visualViewport ? {
      visualViewport: {
        offsetLeft: window.visualViewport.offsetLeft,
        offsetTop: window.visualViewport.offsetTop,
        width: window.visualViewport.width,
        height: window.visualViewport.height,
      },
    } : {}),
  };
}

async function contextForElement(element: Element): Promise<string> {
  try {
    const context = await Promise.race([
      getElementContext(element),
      new Promise<never>((_, reject) => {
        window.setTimeout(() => reject(new Error("element context timed out")), CONTEXT_TIMEOUT_MS);
      }),
    ]);
    return reactGrabContext(element, context) || fallbackContext(element);
  } catch {
    return fallbackContext(element);
  }
}

function elementAt(clientX: number, clientY: number, host: HTMLElement): Element | null {
  return (
    document
      .elementsFromPoint(clientX, clientY)
      .find(
        (element) =>
          element !== host &&
          element !== document.documentElement &&
          element !== document.body &&
          !element.hasAttribute(OVERLAY_ATTRIBUTE),
      ) ?? null
  );
}

function labelFor(element: Element): string {
  if (element.id) return `${element.localName}#${element.id}`;
  const className = Array.from(element.classList).find((name) => name.length <= 32);
  return className ? `${element.localName}.${className}` : element.localName;
}

function adaptiveRadius(radius: number, width: number, height: number, scale: number): number {
  const cap = 0.45 * Math.min(width, height);
  return cap > 0 ? cap * Math.tanh((radius * scale) / cap) : 0;
}

function start(token: string, requestedAppearance: ElementPickerAppearance): void {
  cleanupActivePicker?.();
  cleanupActivePicker = null;
  if (!token || !requestedAppearance) return;

  const appearance = requestedAppearance;
  const radius = Number.isFinite(appearance.radius)
    ? Math.min(Math.max(appearance.radius, 0), 1_000)
    : 0;
  const pageZoom = Number.isFinite(appearance.pageZoom)
    ? Math.min(Math.max(appearance.pageZoom, 0.25), 5)
    : 1;
  const scale = 1 / pageZoom;

  const host = document.createElement("div");
  host.setAttribute(OVERLAY_ATTRIBUTE, "");
  host.style.cssText =
    "position:fixed;inset:0;z-index:2147483646;pointer-events:none;contain:strict";
  host.style.setProperty("--ala-cef-picker-highlight-outline", appearance.highlightOutline);
  host.style.setProperty("--ala-cef-picker-highlight-fill", appearance.highlightFill);
  host.style.setProperty("--ala-cef-picker-highlight-contrast", appearance.highlightContrast);
  host.style.setProperty("--ala-cef-picker-preview-background", appearance.previewBackground);
  host.style.setProperty("--ala-cef-picker-preview-foreground", appearance.previewForeground);
  host.style.setProperty("--ala-cef-picker-preview-border", appearance.previewBorder);
  host.style.setProperty("--ala-cef-picker-preview-shadow", appearance.shadow ?? "transparent");
  host.style.setProperty(
    "--ala-cef-picker-font-family",
    `${JSON.stringify(appearance.fontFamily)}, monospace`,
  );
  host.style.setProperty("--ala-cef-picker-outline-width", `${scale}px`);
  host.style.setProperty("--ala-cef-picker-hairline", `${scale}px`);
  host.style.setProperty("--ala-cef-picker-viewport-inset", `${16 * scale}px`);
  host.style.setProperty("--ala-cef-picker-label-max-width", `${360 * scale}px`);
  host.style.setProperty("--ala-cef-picker-padding-y", `${3 * scale}px`);
  host.style.setProperty("--ala-cef-picker-padding-x", `${6 * scale}px`);
  host.style.setProperty("--ala-cef-picker-font-size", `${11 * scale}px`);
  host.style.setProperty("--ala-cef-picker-line-height", `${15 * scale}px`);
  host.style.setProperty("--ala-cef-picker-shadow-y", `${scale}px`);
  host.style.setProperty("--ala-cef-picker-shadow-blur", `${3 * scale}px`);
  const shadow = host.attachShadow({ mode: "closed" });
  const style = document.createElement("style");
  style.textContent = `
    :host { all: initial; }
    .outline {
      position: fixed;
      display: none;
      box-sizing: border-box;
      border: var(--ala-cef-picker-outline-width) solid var(--ala-cef-picker-highlight-outline);
      background: var(--ala-cef-picker-highlight-fill);
      box-shadow: 0 0 0 calc(var(--ala-cef-picker-hairline) / 2) var(--ala-cef-picker-highlight-contrast);
      pointer-events: none;
    }
    .label {
      position: fixed;
      display: none;
      box-sizing: border-box;
      max-width: min(
        var(--ala-cef-picker-label-max-width),
        calc(100vw - var(--ala-cef-picker-viewport-inset))
      );
      overflow: hidden;
      padding: var(--ala-cef-picker-padding-y) var(--ala-cef-picker-padding-x);
      border: var(--ala-cef-picker-hairline) solid var(--ala-cef-picker-preview-border);
      background: var(--ala-cef-picker-preview-background);
      box-shadow:
        0 var(--ala-cef-picker-shadow-y) var(--ala-cef-picker-shadow-blur)
        var(--ala-cef-picker-preview-shadow);
      color: var(--ala-cef-picker-preview-foreground);
      corner-shape: squircle;
      font-family: var(--ala-cef-picker-font-family);
      font-size: var(--ala-cef-picker-font-size);
      font-weight: 500;
      line-height: var(--ala-cef-picker-line-height);
      text-overflow: ellipsis;
      white-space: nowrap;
      pointer-events: none;
    }
  `;
  const outline = document.createElement("div");
  outline.className = "outline";
  const label = document.createElement("div");
  label.className = "label";
  shadow.append(style, outline, label);

  const cursorStyle = document.createElement("style");
  cursorStyle.setAttribute(OVERLAY_ATTRIBUTE, "");
  cursorStyle.textContent =
    "html[data-ala-cef-picking], html[data-ala-cef-picking] body, html[data-ala-cef-picking] body * { cursor: crosshair !important; -webkit-tap-highlight-color: transparent !important; }";
  document.documentElement.setAttribute("data-ala-cef-picking", "");
  document.documentElement.append(cursorStyle, host);

  let selected: Element | null = null;
  let pending = false;

  const hide = (): void => {
    outline.style.display = "none";
    label.style.display = "none";
  };

  const positionLabel = (target: DOMRect): void => {
    label.style.display = "block";
    label.style.left = "0px";
    label.style.top = "0px";
    const measured = label.getBoundingClientRect();
    const edgeGap = 8 * scale;
    const labelGap = 4 * scale;
    const maxLeft = Math.max(edgeGap, window.innerWidth - measured.width - edgeGap);
    const maxTop = Math.max(edgeGap, window.innerHeight - measured.height - edgeGap);
    const above = target.top - measured.height - labelGap;
    const below = target.bottom + labelGap;
    label.style.left = `${Math.min(Math.max(target.left, edgeGap), maxLeft)}px`;
    label.style.top = `${above >= edgeGap ? above : Math.min(Math.max(below, edgeGap), maxTop)}px`;
    label.style.borderRadius = `${adaptiveRadius(
      radius,
      measured.width,
      measured.height,
      scale,
    )}px`;
  };

  const paint = (): void => {
    if (!selected || !selected.isConnected) {
      hide();
      return;
    }
    const rect = selected.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) {
      hide();
      return;
    }
    outline.style.display = "block";
    outline.style.left = `${rect.left}px`;
    outline.style.top = `${rect.top}px`;
    outline.style.width = `${rect.width}px`;
    outline.style.height = `${rect.height}px`;
    label.textContent = labelFor(selected);
    positionLabel(rect);
  };

  const cleanup = (): void => {
    window.removeEventListener("pointermove", onPointerMove, true);
    window.removeEventListener("pointerdown", onPointerDown, true);
    window.removeEventListener("pointerup", suppressPrimaryButton, true);
    window.removeEventListener("mousedown", suppressPrimaryButton, true);
    window.removeEventListener("mouseup", suppressPrimaryButton, true);
    window.removeEventListener("click", onClick, true);
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("scroll", paint, true);
    window.removeEventListener("resize", paint);
    document.documentElement.removeAttribute("data-ala-cef-picking");
    cursorStyle.remove();
    host.remove();
    if (cleanupActivePicker === cleanup) cleanupActivePicker = null;
    if (heldPicker === held) heldPicker = null;
  };

  const cancel = (notify: boolean): void => {
    cleanup();
    if (notify) send({ version: 1, kind: "cancelled", token });
  };

  function onPointerMove(event: PointerEvent): void {
    if (pending) return;
    selected = elementAt(event.clientX, event.clientY, host);
    paint();
  }

  function onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return;
    if (!pending) {
      selected = elementAt(event.clientX, event.clientY, host);
      paint();
    }
    event.preventDefault();
    event.stopImmediatePropagation();
  }

  function suppressPrimaryButton(event: MouseEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  }

  function onClick(event: MouseEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (!selected || pending) return;
    commit(selected);
  }

  function commit(element: Element): void {
    pending = true;
    const geometry = geometryFor(element);
    label.textContent = "Inspecting source…";
    positionLabel(element.getBoundingClientRect());
    void contextForElement(element)
      .then((text) => {
        cleanup();
        send({ version: 1, kind: "picked", token, text, geometry });
      })
      .catch(() => {
        cleanup();
        send({ version: 1, kind: "failed", token });
      });
  }

  function onKeyDown(event: KeyboardEvent): void {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    cancel(true);
  }

  const held: HeldPicker = {
    point(x, y, width) {
      if (pending) return;
      const viewport = window.visualViewport;
      const scale = (viewport?.width ?? window.innerWidth) / width;
      selected =
        width > 0 && x >= 0 && y >= 0 && x <= width && Number.isFinite(scale)
          ? elementAt(
              (viewport?.offsetLeft ?? 0) + x * scale,
              (viewport?.offsetTop ?? 0) + y * scale,
              host,
            )
          : null;
      paint();
    },
    release() {
      if (pending) return;
      if (selected) commit(selected);
      else cancel(true);
    },
  };

  window.addEventListener("pointermove", onPointerMove, true);
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("pointerup", suppressPrimaryButton, true);
  window.addEventListener("mousedown", suppressPrimaryButton, true);
  window.addEventListener("mouseup", suppressPrimaryButton, true);
  window.addEventListener("click", onClick, true);
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("scroll", paint, true);
  window.addEventListener("resize", paint);
  cleanupActivePicker = cleanup;
  heldPicker = held;
}

function cancel(): void {
  cleanupActivePicker?.();
  cleanupActivePicker = null;
}

function point(x: number, y: number, width: number): void {
  heldPicker?.point(x, y, width);
}

function release(): void {
  heldPicker?.release();
}

if (!window[API_NAME as keyof Window]) {
  Object.defineProperty(window, API_NAME, {
    configurable: false,
    enumerable: false,
    writable: false,
    value: Object.freeze({ start, cancel, point, release } satisfies PickerApi),
  });
}
