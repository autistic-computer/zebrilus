async function presentScroll() {
  const strip = document.createElement('div');
  strip.style.cssText = 'position:fixed;top:50%;left:0;width:100%;height:12px;display:flex;z-index:1';
  const bars = Array.from({length:13}, () => {
    const bar = document.createElement('i');
    bar.style.cssText = 'flex:1 1 0;background:#808080';
    strip.append(bar);
    return bar;
  });
  document.body.append(strip);
  area.width = innerWidth;
  paint(0);
  await frame();
  await frame();
  await report('ready', {geometry:geometry(), ua:navigator.userAgent});
  const {seconds} = await (await fetch('/next')).json();
  const tainted = [];
  if (document.hidden) tainted.push('hidden at start');
  const onVisibility = () => { if (document.hidden) tainted.push('hidden during scroll'); };
  document.addEventListener('visibilitychange', onVisibility);
  const times = [];
  let now = await frame();
  const start = now;
  for (let index = 0; now - start < 1000 * seconds; index++) {
    bars[0].style.background = 'white';
    for (let bit = 0; bit < 12; bit++) {
      if (!index || ((index ^ (index-1)) & (1 << bit)))
        bars[bit+1].style.background = (index & (1<<bit)) ? 'white' : 'black';
    }
    scrollTo(0, 16 * (index % 500 + 1));
    const next = await frame();
    times.push(next-now);
    now = next;
  }
  document.removeEventListener('visibilitychange', onVisibility);
  bars.forEach(bar => { bar.style.background = '#808080'; });
  await report('report', {frames:times.length, wall_ms:now-start, engine_fps:1000*times.length/(now-start),
    intervals_ms:times, tainted, y:scrollY, geometry:geometry()});
}
