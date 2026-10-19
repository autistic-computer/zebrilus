const [port, expr] = process.argv.slice(2);
let list;
for (let i = 0; i < 50; i++) {
  try { list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json(); break; } catch { await new Promise((r) => setTimeout(r, 200)); }
}
const page = list?.find((t) => t.type === 'page' && t.url.startsWith('file:'));
if (!page) process.exit(2);
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.addEventListener('open', r); ws.addEventListener('error', j); });
ws.addEventListener('message', (e) => {
  const m = JSON.parse(e.data);
  if (m.id === 1) { console.log(JSON.stringify(m.result?.result?.value ?? null)); ws.close(); process.exit(0); }
});
ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression: expr, returnByValue: true } }));
