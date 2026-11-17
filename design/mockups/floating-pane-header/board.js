(() => {
  const params = new URLSearchParams(location.search);
  const only = params.get("only");
  if (only) {
    document.documentElement.dataset.only = only;
    for (const take of document.querySelectorAll(".hf-take")) take.hidden = take.id !== only;
  }

  const HEAD = 34, PAD = 6, LINE = 16;
  const ACTIONS = ["panel-bottom", "panel-right", "picture-in-picture", "expand", "xmark"];
  const CWD = "~/dev/autistic-computer/zebrilus";

  const header = ({ icon, name, cwd, folded }) => `
    <div class="hf-head">
      <div data-penna="bar.pane">
        <div data-penna="button.title"><i data-icon="${icon}"></i><span class="hf-name">${name}</span>${cwd ? `<span class="hf-cwd">${cwd}</span>` : ""}</div>
        <span class="hf-grow"></span>
        <div class="hf-actions">${(folded ? ["ellipsis"] : ACTIONS).map((a) => `<button data-penna="icon-button.bar"><i data-icon="${a}"></i></button>`).join("")}</div>
      </div>
    </div>`;

  const dock = (text) => `
    <div class="hf-dock">
      <div class="hf-composer" data-penna="field.composer">
        <button class="hf-plus" data-penna="icon-button"><i data-icon="plus"></i></button>
        <div class="hf-input penna-muted">${text}</div>
        <button data-penna="button.send" data-prominent data-state="disabled"><i data-icon="arrow-up"></i></button>
      </div>
      <div class="hf-foot">
        <span class="hf-grow"></span>
        <span class="hf-chip" data-penna="chip"><i data-icon="circle-user"></i>Ask</span>
        <span class="hf-chip" data-penna="chip">Opus 5.5<span class="hf-dim">High</span><i data-icon="chevron-down"></i></span>
      </div>
    </div>`;

  const sky = (seed) => `
    <div class="hf-body" data-sky="${seed}">
      <canvas class="hf-sky"></canvas>
      <div class="hf-empty">
        <div class="hf-hero"><i data-icon="asterisk" style="--penna-icon-size: 40px"></i><b>Claude Code is ready</b><span>Ask it anything, or pick up a chat.</span></div>
        <div class="hf-project">
          <button class="hf-pbtn" data-penna="button"><i data-icon="folder"></i><span>zebrilus</span><i data-icon="chevron-down"></i></button>
          <button class="hf-pbtn" data-penna="button.quiet"><i data-icon="history"></i><span>Resume</span><i data-icon="chevron-down"></i></button>
        </div>
      </div>
      ${dock("Message Claude Code...")}
    </div>`;

  const bubble = (text, anchor) => `<div class="penna-chat-message"${anchor ? " data-anchor" : ""}><div class="penna-card penna-chat-bubble" data-penna="card.bubble"><span>${text}</span></div></div>`;
  const say = (text) => `<p>${text}</p>`;
  const step = (icon, verb, what) => `<div class="hf-step"><i data-icon="${icon}"></i><b>${verb}</b>${what}</div>`;
  const code = (file, body) => `<div class="hf-code" data-penna="card.code"><div class="hf-code-head"><i data-icon="file"></i>${file}</div><pre>${body}</pre></div>`;

  const THREAD = [
    bubble("The header cuts the sky off at its bottom edge, and chat text stops dead there too. Can it sit over the content instead?"),
    say("Today a pane is two flex rows: the cached header view, 34px tall, then the content. Nothing paints under the header, so whatever the content draws ends at its edge."),
    step("file", "Read", "crates/zz/src/workspace/view.rs"),
    step("file", "Read", "crates/zz-ui/src/pane.rs"),
    say("If the header goes absolute, the content takes the whole pane and gets a 34px top inset instead:"),
    code("crates/zz/src/workspace/view.rs", `div()
    .relative()
    .size_full()
    .child(content.inset_top(header))
    .child(header_view.absolute().top_0())`),
    say("The terminal keeps its rows: the inset is the header's height, so the PTY size does not change."),
    bubble("Keep the first row of a full-screen program clear. Scrollback is fine under it, and so are chat messages when I scroll up.", true),
    say("Then the live screen starts right under the header, and history rows draw into the band above it once you scroll back. In a chat the list keeps its <code>inset_top</code> padding, so the first message rests below the header and only scrolls under it."),
    step("pencil", "Edited", "crates/zz-ui/src/pane.rs"),
    step("pencil", "Edited", "crates/zz/src/workspace/view.rs"),
    code("crates/zz-ui/src/pane.rs", `pub fn pane_header_band(
    look: HeaderBand,
    surface: Hsla,
    cx: &App,
) -> AnyElement {
    match look {
        HeaderBand::Solid => div().into_any(),
        HeaderBand::Float => band(surface, cx),
    }
}`),
    say("Browser, view and editor panes pass <code>HeaderBand::Solid</code>, so they keep today's bar."),
    bubble("Show me the three takes on a board first."),
    say("On it. I'll draw them on the real theme with the sky, a long chat, a shell in scrollback and Neovim."),
  ].join("");

  const chat = (scrolled) => `
    <div class="hf-body">
      <div class="hf-thread"><div class="hf-rows" style="padding-top: ${scrolled ? 0 : HEAD + 12}px">${THREAD}</div></div>
      ${dock("Message Claude Code...")}
    </div>`;

  const P = `<span class="c">zz</span> <span class="g">❯</span> `;
  const SHELL = [
    `${P}cargo build -p zz-ui`,
    `<span class="g">   Compiling</span> penna v0.9.0`,
    `<span class="g">   Compiling</span> zz-ui v0.22.0`,
    `<span class="y w">warning</span><span class="w">: unused import: \`px\`</span>`,
    `<span class="b">  --&gt;</span> crates/zz-ui/src/`,
    `      pane.rs:12:5`,
    `<span class="b">   |</span>`,
    `<span class="b">12 |</span> use ala::{px, Pixels};`,
    `<span class="b">   |</span>           <span class="y">^^</span>`,
    `<span class="b">   |</span>`,
    `<span class="b">   =</span> <span class="w">note</span>: on by default`,
    `<span class="y w">warning</span><span class="w">: \`zz-ui\` (lib)</span>`,
    `<span class="w">generated 1 warning</span>`,
    `<span class="g">    Finished</span> \`dev\` in 41.3s`,
    `${P}git status -s`,
    `<span class="r"> M</span> crates/zz-ui/src/pane.rs`,
    `<span class="r"> M</span> crates/zz/src/workspace/`,
    `       view.rs`,
    `<span class="r">??</span> design/mockups/floating-p`,
    `ane-header/`,
    `${P}git log --oneline -9`,
    `<span class="y">1954c97</span> Add the agent mentio`,
    `ns design`,
    `<span class="y">2380a89</span> Refresh the storyboo`,
    `k lockfile`,
    `<span class="y">c763456</span> Merge branch 'tab-sq`,
    `uares'`,
    `<span class="y">2d6821e</span> Size the chat sideba`,
    `r's icon column`,
    `<span class="y">650007e</span> Size chat row stacks`,
    `<span class="y">d65370b</span> Board the agent sign`,
    `-in steps`,
    `<span class="y">d96bf32</span> Add the chat fluff b`,
    `oard`,
    `<span class="y">abc8966</span> Polish the phone cli`,
    `ent`,
    `<span class="y">8b1e0aa</span> Fold header actions`,
    `${P}ls`,
    `Cargo.lock  <span class="b w">bench</span>    <span class="b w">design</span>`,
    `Cargo.toml  <span class="b w">clients</span>  <span class="b w">knowledge</span>`,
    `Justfile    <span class="b w">compat</span>   <span class="b w">libs</span>`,
    `README.md   <span class="b w">crates</span>   <span class="b w">scripts</span>`,
    `${P}rg -c pane_header crates`,
    `<span class="m">crates/zz-ui/src/pane.rs</span>:9`,
    `<span class="m">crates/zz-ui/src/pip.rs</span>:1`,
    `<span class="m">crates/zz/src/workspace/vie</span>`,
    `<span class="m">w.rs</span>:6`,
    `<span class="m">clients/app/src/app.rs</span>:4`,
    `${P}just design serve`,
    `serving on 127.0.0.1:8090`,
    `^C`,
    `${P}<span class="cur"> </span>`,
  ];

  const term = (lines, top) => `<pre class="hf-term" style="top: ${top}px">${lines.map((l) => `<div>${l}</div>`).join("")}</pre>`;

  const shell = (scrolled, height) => {
    const rows = Math.floor((height - HEAD - 2 * PAD) / LINE);
    if (!scrolled) return `<div class="hf-body">${term(SHELL.slice(-rows), HEAD + PAD)}</div>`;
    const above = Math.ceil((HEAD + PAD) / LINE);
    const start = 2;
    return `<div class="hf-body">${term(SHELL.slice(start, start + above + rows), HEAD + PAD - above * LINE)}</div>`;
  };

  const n = (num) => `<span class="f">${String(num).padStart(5)} </span>`;
  const CODE = [
    `                <span class="b">.child</span>(`,
    `                    <span class="c">AnyView</span>::<span class="b">from</span>(header_view.<span class="b">clone</span>()).<span class="b">cached</span>(`,
    `                        <span class="c">StyleRefinement</span>::<span class="b">default</span>()`,
    `                            .<span class="b">h</span>(cx.<span class="b">theme</span>().<span class="b">pane_header_height</span>())`,
    `                            .<span class="b">w_full</span>()`,
    `                            .<span class="b">flex_none</span>(),`,
    `                    ),`,
    `                )`,
    `                .<span class="b">child</span>(`,
    `                    <span class="b">div</span>()`,
    `                        .<span class="b">relative</span>()`,
    `                        .<span class="b">flex_1</span>()`,
    `                        .<span class="b">min_h_0</span>()`,
    `                        .<span class="b">min_w_0</span>()`,
    `                        .<span class="b">child</span>(content)`,
    `                        .<span class="b">children</span>(<span class="m">self</span>.<span class="b">tile_size_report</span>(*pane, pane_snapshot)),`,
  ];
  const nvim = (height) => {
    const rows = Math.floor((height - HEAD - 2 * PAD) / LINE);
    const lines = [`<div class="tl"><span class="ts"> view.rs </span> pane.rs  sky.rs  chat.rs ${" ".repeat(60)}</div>`];
    const body = rows - 3;
    for (let i = 0; i < body; i++) lines.push(i < CODE.length ? `${n(3846 + i)}${CODE[i]}` : `<span class="f">~</span>`);
    lines.push(`<div class="sl"><span class="sm"> NORMAL </span><span class="sb">  header-float </span> crates/zz/src/workspace/view.rs${" ".repeat(3)}rust  3849:21 </div>`);
    lines.push(``);
    return `<div class="hf-body">${term(lines, HEAD + PAD)}</div>`;
  };

  const panes = (state, slots) => {
    const scrolled = state !== "rest";
    const make = {
      chat: () => `<section data-penna="pane" class="hf-pane" data-slot="chat"${scrolled ? " data-scrolled" : ""}>${chat(scrolled)}${header({ icon: "asterisk", name: "Floating pane header", cwd: CWD })}</section>`,
      sky: () => `<section data-penna="pane" class="hf-pane" data-slot="sky" data-active>${sky(0x4f2a17)}${header({ icon: "asterisk", name: "Claude Code", cwd: CWD })}</section>`,
      tui: () => `<section data-penna="pane" class="hf-pane" data-slot="tui">${nvim(361)}${header({ icon: "square-terminal", name: "Neovim", cwd: CWD })}</section>`,
      shell: () => `<section data-penna="pane" class="hf-pane" data-slot="shell" data-hover${scrolled ? " data-scrolled" : ""}>${shell(scrolled, 728)}${header({ icon: "square-terminal", name: "fish", cwd: "~/d/a/zebrilus", folded: true })}</section>`,
    };
    return slots.map((slot) => make[slot]()).join("");
  };

  for (const win of document.querySelectorAll(".hf-window")) {
    const slots = (win.dataset.slots ?? "chat sky tui shell").split(" ");
    win.innerHTML = `
      <div class="penna-lights"><i></i><i></i><i></i></div>
      <header data-penna="titlebar">
        <div data-penna="tab.strip">
          <div data-penna="tab.window" aria-selected="true" style="width: 180px"><i data-icon="sparkle"></i>zebrilus</div>
          <div data-penna="tab.window" style="width: 180px"><i data-icon="globe"></i>docs</div>
        </div>
      </header>
      <div class="hf-split">${panes(win.dataset.state ?? "scrolled", slots)}</div>`;
  }

  const anchor = () => {
    for (const pane of document.querySelectorAll(".hf-pane[data-slot='chat'][data-scrolled]")) {
      const rows = pane.querySelector(".hf-rows");
      const mark = rows.querySelector("[data-anchor]");
      rows.style.transform = "";
      const offset = mark.getBoundingClientRect().top - rows.getBoundingClientRect().top;
      rows.style.transform = `translateY(${10 - offset}px)`;
    }
  };

  const settle = () => {
    window.pennaKit?.roundAll?.();
    anchor();
    window.hfSky?.();
  };
  document.getElementById("penna-tokens")?.addEventListener("load", () => requestAnimationFrame(settle));
  addEventListener("load", () => requestAnimationFrame(settle));
  document.fonts?.ready.then(() => requestAnimationFrame(settle));
  document.addEventListener("penna-theme", () => setTimeout(settle, 200));
  window.hfSettle = settle;
})();
