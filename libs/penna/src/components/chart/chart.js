(() => {
const NS = "http://www.w3.org/2000/svg";
const DOT_RADIUS = 4;

function drawLines(svg) {
  const raw = svg.dataset.series;
  if (!raw) return;
  let series;
  try {
    series = JSON.parse(raw);
  } catch {
    return;
  }
  const plot = svg.parentElement;
  if (!plot) return;
  const width = plot.clientWidth;
  const height = plot.clientHeight || 160;
  if (!width) return;
  const low = Number(svg.dataset.min ?? 0);
  const high = Number(svg.dataset.max ?? 1);
  const span = high - low || 1;
  const count = Math.max(1, Number(svg.dataset.count) || Math.max(...series.map((s) => s.length)));
  const band = width / count;
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.replaceChildren();
  const spot = (index, value) => [
    band * (index + 0.5),
    height * (1 - (value - low) / span),
  ];
  series.slice(0, 4).forEach((values, sIndex) => {
    const color = `var(--penna-series-${sIndex})`;
    let d = "";
    let run = 0;
    values.forEach((value, index) => {
      if (typeof value === "number" && Number.isFinite(value)) {
        const [x, y] = spot(index, value);
        d += `${run === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
        run += 1;
      } else {
        run = 0;
      }
    });
    if (d) {
      const path = document.createElementNS(NS, "path");
      path.setAttribute("d", d);
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", color);
      path.setAttribute("stroke-width", "2");
      path.setAttribute("stroke-linecap", "butt");
      path.setAttribute("stroke-linejoin", "miter");
      svg.append(path);
    }
    values.forEach((value, index) => {
      if (typeof value !== "number" || !Number.isFinite(value)) return;
      const prev = index > 0 && typeof values[index - 1] === "number" && Number.isFinite(values[index - 1]);
      const next = index + 1 < values.length && typeof values[index + 1] === "number" && Number.isFinite(values[index + 1]);
      if (prev || next) return;
      const [x, y] = spot(index, value);
      const dot = document.createElementNS(NS, "circle");
      dot.setAttribute("cx", x.toFixed(1));
      dot.setAttribute("cy", y.toFixed(1));
      dot.setAttribute("r", String(DOT_RADIUS));
      dot.setAttribute("fill", color);
      svg.append(dot);
    });
  });
}

function wireChart(chart) {
  const svgs = [...chart.querySelectorAll("svg.penna-chart-svg[data-series]")];
  if (!svgs.length) return;
  const render = () => svgs.forEach(drawLines);
  render();
  if (typeof ResizeObserver !== "undefined") {
    const observer = new ResizeObserver(render);
    svgs.forEach((svg) => svg.parentElement && observer.observe(svg.parentElement));
  }
}

function settleCharts() {
  document.querySelectorAll(".penna-chart").forEach(wireChart);
}

settleCharts();
window.addEventListener("resize", settleCharts);
})();
