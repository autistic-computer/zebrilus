(() => {
  const MIN_THUMB = 48;
  const INSET = 2;
  const BAR = 16;
  const FADE_DELAY = 2000;
  const FADE_END = 3000;

  const calm = matchMedia("(prefers-reduced-motion: reduce)");

  function bar(axis, id) {
    const element = document.createElement("div");
    element.className = "penna-scrollbar";
    element.dataset.axis = axis;
    const thumb = document.createElement("div");
    thumb.className = "penna-scroll-thumb";
    const fill = document.createElement("div");
    fill.className = "penna-scroll-fill";
    if (id) {
      element.dataset.id = `${id}-${axis}`;
      fill.dataset.id = `${id}-${axis}-thumb`;
    }
    thumb.append(fill);
    element.append(thumb);
    return element;
  }

  function mount(scroll) {
    if (scroll.pennaScroll) return scroll.pennaScroll;
    const viewport = scroll.querySelector(":scope > .penna-scroll-viewport");
    const mode = scroll.dataset.axis || "both";
    const axes = mode === "both" ? ["horizontal", "vertical"] : [mode];
    const bars = Object.fromEntries(
      axes.map((axis) => {
        const element = scroll.querySelector(`:scope > .penna-scrollbar[data-axis="${axis}"]`) ?? scroll.appendChild(bar(axis, scroll.dataset.bar));
        return [axis, { axis, element, thumb: element.querySelector(".penna-scroll-thumb"), start: 0, length: 0, full: 0, margin: 0, content: 0, container: 0 }];
      }),
    );
    const state = { last: null, shown: 0, drag: null, frame: 0 };

    const offset = (axis) => (axis === "vertical" ? viewport.scrollTop : viewport.scrollLeft);
    const setOffset = (axis, value) => {
      if (axis === "vertical") viewport.scrollTop = value;
      else viewport.scrollLeft = value;
    };

    function layout() {
      let both = mode === "both";
      const box = scroll.getBoundingClientRect();
      for (const axis of axes) {
        const info = bars[axis];
        const vertical = axis === "vertical";
        const content = vertical ? viewport.scrollHeight : viewport.scrollWidth;
        const container = vertical ? box.height - scroll.clientTop * 2 : box.width - scroll.clientLeft * 2;
        const margin = both && !vertical ? BAR : 0;
        if (content <= container) {
          both = false;
          info.element.hidden = true;
          continue;
        }
        info.element.hidden = false;
        const length = Math.max((container / content) * container, MIN_THUMB);
        const start = (offset(axis) / (content - container)) * (container - margin - length);
        const end = Math.min(start + length, container - margin);
        Object.assign(info, { start, length: end - start - INSET * 2, full: length, margin, content, container });
        info.thumb.style.setProperty("--penna-thumb-start", `${start}px`);
        info.thumb.style.setProperty("--penna-thumb-length", `${info.length}px`);
      }
    }

    function visible(on) {
      for (const { element } of Object.values(bars)) element.toggleAttribute("data-visible", on);
    }

    function opacity(value) {
      for (const { element } of Object.values(bars)) {
        if (value === null) element.style.removeProperty("--penna-thumb-opacity");
        else element.style.setProperty("--penna-thumb-opacity", value);
      }
    }

    function tick() {
      state.frame = 0;
      if (state.drag || Object.values(bars).some(({ element }) => element.matches(":hover"))) state.shown = performance.now();
      const elapsed = performance.now() - state.shown;
      if (elapsed >= FADE_END) {
        visible(false);
        opacity(null);
        return;
      }
      const fade = elapsed < FADE_DELAY || calm.matches ? 1 : 1 - ((elapsed - FADE_DELAY) / 1000) ** 10;
      opacity(fade);
      state.frame = requestAnimationFrame(tick);
    }

    function show() {
      if (scroll.dataset.show === "always") return;
      state.shown = performance.now();
      visible(true);
      if (!state.frame) state.frame = requestAnimationFrame(tick);
    }

    function scrolled() {
      const now = axes.map(offset).join();
      layout();
      if (now !== state.last) {
        state.last = now;
        show();
      }
    }

    function along(info, event) {
      const box = info.element.getBoundingClientRect();
      return info.axis === "vertical" ? [event.clientY, box.top, box.height] : [event.clientX, box.left, box.width];
    }

    for (const info of Object.values(bars)) {
      info.element.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.stopPropagation();
        const [position, origin, size] = along(info, event);
        const thumbStart = origin + INSET + info.start;
        const range = info.content - info.container;
        if (position >= thumbStart && position <= thumbStart + info.length) {
          state.drag = { info, grab: position - thumbStart };
          info.element.setPointerCapture(event.pointerId);
          info.element.dataset.state = "active";
          return;
        }
        const travel = size - info.full - (info.axis === "vertical" ? 0 : info.margin);
        const ratio = Math.min(Math.max((position - info.full / 2 - origin) / travel, 0), 1);
        setOffset(info.axis, range * ratio);
        info.thumb.dataset.state = "hover";
      });
      info.element.addEventListener("pointermove", (event) => {
        const [position, origin, size] = along(info, event);
        const thumbStart = origin + INSET + info.start;
        const over = position >= thumbStart && position <= thumbStart + info.length;
        if (over) info.thumb.dataset.state = "hover";
        else delete info.thumb.dataset.state;
        if (state.drag?.info !== info) return;
        const travel = size - info.full - (info.axis === "vertical" ? 0 : info.margin);
        const ratio = Math.min(Math.max((position - state.drag.grab - origin) / travel, 0), 1);
        setOffset(info.axis, (info.content - info.container) * ratio);
      });
      const release = () => {
        if (state.drag?.info !== info) return;
        state.drag = null;
        delete info.element.dataset.state;
        show();
      };
      info.element.addEventListener("pointerup", release);
      info.element.addEventListener("pointercancel", release);
      info.element.addEventListener("lostpointercapture", release);
      info.element.addEventListener(
        "wheel",
        (event) => {
          event.preventDefault();
          viewport.scrollBy(event.deltaX, event.deltaY);
        },
        { passive: false },
      );
      info.element.addEventListener("pointerleave", () => {
        if (state.drag?.info !== info) delete info.thumb.dataset.state;
      });
      info.element.addEventListener("pointerenter", () => {
        if (info.element.hasAttribute("data-visible")) show();
      });
    }

    if (scroll.hasAttribute("data-hovered")) {
      for (const { thumb } of Object.values(bars)) {
        thumb.dataset.state = "hover";
        document.addEventListener("pointermove", () => delete thumb.dataset.state, { once: true, capture: true });
      }
    }

    const x = parseFloat(scroll.dataset.scrollX);
    const y = parseFloat(scroll.dataset.scrollY);
    if (x) viewport.scrollLeft = x;
    if (y) viewport.scrollTop = y;
    state.last = axes.map(offset).join();
    viewport.addEventListener("scroll", scrolled, { passive: true });
    new ResizeObserver(layout).observe(scroll);
    layout();
    scroll.pennaScroll = { layout };
    return scroll.pennaScroll;
  }

  function settleAll(root = document) {
    root.querySelectorAll(".penna-scroll").forEach((scroll) => mount(scroll).layout());
  }

  settleAll();
  document.fonts.ready.then(() => settleAll());
  window.pennaScroll = { mount, settle: settleAll };
})();
