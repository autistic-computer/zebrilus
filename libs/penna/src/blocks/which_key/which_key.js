(() => {
  const ROW_HEIGHT = 22;
  const TITLE_HEIGHT = 24;
  const LABEL_WIDTH = 150;
  const CAP_GAP = 8;
  const CAP_MIN = 24;
  const CAP_MAX = 112;
  const SET_GAP = 6;
  const COLUMN_GAP = 24;
  const LINE_GAP = 12;
  const STACK_GAP = 10;
  const PADDING_X = 16;
  const CHROME_HEIGHT = 56;
  const WIDTH_SLACK = 4;

  const owned = new WeakMap();

  const blockHeight = (rows, titled) => rows * ROW_HEIGHT + (titled ? TITLE_HEIGHT : 0);

  function groupsOf(rows) {
    const groups = [];
    for (const row of rows) {
      const title = row.dataset.group ?? null;
      const group = groups.find((each) => each.title === title);
      if (group) group.rows.push(row);
      else groups.push({ title, rows: [row] });
    }
    return groups;
  }

  function stacksOf(groups, perColumn) {
    perColumn = Math.max(1, perColumn);
    const stacks = [];
    let open = null;
    for (const group of groups) {
      const titled = group.title !== null;
      const whole = group.rows.length <= perColumn;
      const height = blockHeight(group.rows.length, titled);
      if (whole && open !== null && open + STACK_GAP + height <= blockHeight(perColumn, titled) && stacks.length) {
        stacks[stacks.length - 1].push({ title: group.title, group: group.title, part: 0, titled, rows: group.rows });
        open += STACK_GAP + height;
        continue;
      }
      for (let index = 0; index * perColumn < group.rows.length; index++) {
        stacks.push([{ title: index === 0 ? group.title : null, group: group.title, part: index, titled, rows: group.rows.slice(index * perColumn, (index + 1) * perColumn) }]);
      }
      open = whole ? height : null;
    }
    return stacks;
  }

  const stackHeight = (stack) => stack.reduce((sum, block) => sum + blockHeight(block.rows.length, block.titled), 0) + Math.max(0, stack.length - 1) * STACK_GAP;

  function layoutHeight(stacks, perLine) {
    perLine = Math.max(1, perLine);
    let total = 0;
    let count = 0;
    for (let start = 0; start < stacks.length; start += perLine) {
      total += Math.max(0, ...stacks.slice(start, start + perLine).map(stackHeight));
      count++;
    }
    return total + Math.max(0, count - 1) * LINE_GAP;
  }

  function perLineOf(width, columnWidth) {
    const inner = width - 2 * PADDING_X - WIDTH_SLACK;
    const fits = Math.floor((inner + COLUMN_GAP) / (columnWidth + COLUMN_GAP));
    return Number.isFinite(fits) && fits >= 1 ? fits : 1;
  }

  const lineWidth = (columns, columnWidth) => {
    columns = Math.max(1, columns);
    return columns * columnWidth + (columns - 1) * COLUMN_GAP + 2 * PADDING_X + WIDTH_SLACK;
  };

  function fitOf(groups, columnWidth, available) {
    const tallest = Math.max(1, ...groups.map((group) => group.rows.length));
    if (!available) return { perColumn: tallest, width: null };
    const perLine = perLineOf(available.width, columnWidth);
    const budget = available.height - CHROME_HEIGHT;
    let best = [Infinity, tallest, 1];
    for (let perColumn = tallest; perColumn >= 1; perColumn--) {
      const stacks = stacksOf(groups, perColumn);
      const height = layoutHeight(stacks, perLine);
      if (height <= budget) {
        best = [height, perColumn, stacks.length];
        break;
      }
      if (height < best[0]) best = [height, perColumn, stacks.length];
    }
    return { perColumn: best[1], width: lineWidth(Math.min(best[2], perLine), columnWidth) };
  }

  function textWidth(element) {
    const range = document.createRange();
    range.selectNodeContents(element);
    return range.getBoundingClientRect().width;
  }

  function capWidths(root) {
    const caps = [...root.querySelectorAll(".penna-which-key-row .penna-which-key-cap")];
    return new Map(caps.map((cap) => [cap, textWidth(cap)]));
  }

  function blockId(block) {
    const base = block.group === null ? "which-key-group" : `which-key-group-${block.group}`;
    return block.part === 0 ? base : `${base}-${block.part}`;
  }

  function available(view) {
    const [width, height] = (view.dataset.fit ?? "").split(/[\sx×]+/).map(Number);
    return width && height ? { width, height } : null;
  }

  function layout(view) {
    const body = view.querySelector(".penna-which-key-body");
    if (!owned.has(view)) owned.set(view, [...body.querySelectorAll(".penna-which-key-row")]);
    const rows = owned.get(view);
    const widths = capWidths(view);
    const capWidth = Math.ceil(
      Math.min(
        CAP_MAX,
        rows.reduce((widest, row) => {
          const caps = [...row.querySelectorAll(".penna-which-key-cap")];
          const width = caps.reduce((sum, cap) => sum + (widths.get(cap) ?? 0), 0) + Math.max(0, caps.length - 1) * SET_GAP;
          return Math.max(widest, width);
        }, CAP_MIN),
      ),
    );
    const columnWidth = capWidth + CAP_GAP + LABEL_WIDTH;
    const groups = groupsOf(rows);
    const room = available(view);
    const fit = fitOf(groups, columnWidth, room);
    view.style.maxWidth = fit.width === null ? "" : `${fit.width}px`;
    view.style.maxHeight = room ? `${room.height}px` : "";
    const columns = stacksOf(groups, fit.perColumn).map((stack) => {
      const column = document.createElement("div");
      column.className = "penna-which-key-column";
      column.style.width = `${columnWidth}px`;
      for (const block of stack) {
        const element = document.createElement("div");
        element.className = "penna-which-key-block";
        element.dataset.id = blockId(block);
        if (block.titled) {
          const title = document.createElement("div");
          title.className = "penna-which-key-title";
          title.style.paddingLeft = `${capWidth + CAP_GAP}px`;
          if (block.title !== null) {
            const text = document.createElement("span");
            text.textContent = block.title;
            text.dataset.pennaText = "";
            title.append(text);
          }
          element.append(title);
        }
        const list = document.createElement("div");
        list.className = "penna-which-key-rows";
        for (const row of block.rows) {
          row.querySelector(".penna-which-key-caps").style.width = `${capWidth}px`;
          list.append(row);
        }
        element.append(list);
        column.append(element);
      }
      return column;
    });
    body.replaceChildren(...columns);
    pennaKit.ceilText(view);
  }

  function layoutAll(root = document) {
    root.querySelectorAll(".penna-which-key").forEach((view) => {
      layout(view);
    });
  }

  function platform(root = document) {
    if (!globalThis.pennaKbd?.mac) return;
    root.querySelectorAll("[data-mac]").forEach((element) => {
      element.textContent = element.dataset.mac;
    });
  }

  globalThis.pennaWhichKey = { layout, layoutAll };
  platform();
  layoutAll();
  document.fonts.ready.then(() => layoutAll());
})();
