(() => {
  const only = new URLSearchParams(location.search).get("only");
  if (only) {
    document.documentElement.dataset.only = only;
    for (const take of document.querySelectorAll(".sp-take")) take.hidden = take.dataset.take !== only;
  }

  const CWD = "~/dev/autistic-computer/zebrilus";
  const TILED = [["panel-bottom", "Split bottom"], ["panel-right", "Split right"], ["picture-in-picture", "Float"], ["expand", "Zoom"], ["xmark", "Close pane"]];
  const FLOATED = [["layout-columns", "Tile back"], ["expand", "Zoom"], ["xmark", "Close pane"]];
  const DEVICE = [["house", "Home"], ["rotate", "Rotate"], ["camera", "Screenshot"]];

  const buttons = (list) => list.map(([icon, name]) => `<button data-penna="icon-button.bar" title="${name}"><i data-icon="${icon}"></i></button>`).join("");
  const bar = ({ icon, name, cwd, chevron, device, actions, state }) => `
    <div data-penna="bar.pane">
      <div data-penna="button.title"${state ? ` data-state="${state}"` : ""}><i data-icon="${icon}"></i><span class="sp-name">${name}</span>${cwd ? `<span class="sp-cwd">${cwd}</span>` : ""}${chevron ? `<i data-icon="chevron-down"></i>` : ""}</div>
      ${device ? `<div class="sp-device-actions">${buttons(DEVICE)}</div>` : ""}
      <span class="sp-grow"></span>
      ${actions ? `<div class="sp-actions">${buttons(actions)}</div>` : ""}
    </div>`;
  const header = (options) => `<div class="sp-head">${bar(options)}</div>`;
  const simulator = (extra = {}) => ({ icon: "smartphone", name: "iPhone 18 Pro", chevron: true, device: true, ...extra });

  const screen = (attrs = "") => `<div class="sp-screen" ${attrs}><img src="screen.jpg" alt=""></div>`;

  const bubble = (text) => `<div class="penna-chat-message"><div class="penna-card penna-chat-bubble" data-penna="card.bubble"><span>${text}</span></div></div>`;
  const say = (text) => `<p>${text}</p>`;
  const step = (icon, verb, what) => `<div class="sp-step"><i data-icon="${icon}"></i><b>${verb}</b>${what}</div>`;
  const THREAD = [
    bubble("The phone app won't connect to the rig. Can you look?"),
    step("square-terminal", "Ran", "just ios run"),
    say("The app is up on iPhone 18 Pro. I'll try the rig from the address step."),
    step("keyboard", "Typed", "demfabris@macbook"),
    step("hand", "Tapped", "Connect"),
    step("camera", "Captured", "iPhone 18 Pro"),
    `<div class="sp-shot" data-penna="card.image"><img src="screen.jpg" alt=""></div>`,
    say("It refuses the handshake. The app speaks protocol <code>v125</code> and the rig daemon still answers <code>v124</code>, the binary from before today's bump."),
    say("Restarting the rig daemon on the new build fixes it. Want me to?"),
  ].join("");
  const dock = `
    <div class="sp-dock">
      <div class="sp-composer" data-penna="field.composer">
        <button class="sp-plus" data-penna="icon-button"><i data-icon="plus"></i></button>
        <div class="sp-input penna-muted">Message Claude Code...</div>
        <button data-penna="button.send" data-prominent data-state="disabled"><i data-icon="arrow-up"></i></button>
      </div>
      <div class="sp-foot">
        <span class="sp-grow"></span>
        <span class="sp-chip" data-penna="chip"><i data-icon="circle-user"></i>Ask</span>
        <span class="sp-chip" data-penna="chip">Opus 5.5<span class="sp-dim">High</span><i data-icon="chevron-down"></i></span>
      </div>
    </div>`;
  const chat = (active) => `
    <section data-penna="pane" class="sp-pane"${active ? " data-active" : ""}>
      <div class="sp-body"><div class="sp-thread"><div class="sp-rows">${THREAD}</div></div>${dock}</div>
      ${header({ icon: "asterisk", name: "Phone won't connect", cwd: CWD, actions: TILED })}
    </section>`;

  const P = `<span class="c">zz</span> <span class="g">❯</span> `;
  const SHELL = [
    `${P}just ios rig`,
    `rig daemon on /tmp/zz-ios-rig.sock`,
    `${P}just ios run`,
    `<span class="g">   Compiling</span> zz-protocol v0.28.0`,
    `<span class="g">   Compiling</span> zz-client v0.28.0`,
    `<span class="g">   Compiling</span> zz-ui v0.28.0`,
    `<span class="g">   Compiling</span> zz-app v0.28.0`,
    `<span class="g">   Compiling</span> zz-ios v0.28.0`,
    `<span class="g">    Finished</span> \`dev\` profile in 38.4s`,
    `<span class="b">==&gt;</span> <span class="w">Booting iPhone 18 Pro</span>`,
    `<span class="b">==&gt;</span> <span class="w">Installing dev.zz.ios</span>`,
    `<span class="b">==&gt;</span> <span class="w">Launching dev.zz.ios</span>`,
    `dev.zz.ios: 80568`,
    `<span class="d">[zz-ios] ala 0.9, metal renderer</span>`,
    `<span class="d">[zz-ios] onboarding: page 2</span>`,
    `<span class="d">[zz-ios] dial demfabris@macbook</span>`,
    `<span class="y w">warning</span><span class="w">: handshake refused</span>`,
    `<span class="d">  client protocol 125, daemon 124</span>`,
    `<span class="cur"> </span>`,
  ];
  const shell = (active) => `
    <section data-penna="pane" class="sp-pane"${active ? " data-active" : ""}>
      <div class="sp-body"><pre class="sp-term">${SHELL.map((line) => `<div>${line}</div>`).join("")}</pre></div>
      ${header({ icon: "square-terminal", name: "just", cwd: CWD, actions: TILED })}
    </section>`;

  const tiled = `
    <section data-penna="pane" class="sp-pane" data-active>
      <div class="sp-stagebody">${screen()}</div>
      ${header(simulator({ actions: [["ellipsis", "More"]] }))}
    </section>`;
  const pip = (parked) => `
    <div class="sp-pip" data-penna="floating.pip"${parked ? " data-parked" : ""}>
      ${bar(simulator({ device: false, actions: [["ellipsis", "More"], ...FLOATED] }))}
      <div class="sp-pip-body">${screen()}</div>
    </div>`;

  const layouts = {
    tiled: () => `<div class="sp-split">${chat(false)}${shell(false)}${tiled}</div>`,
    pip: () => `<div class="sp-split">${chat(false)}${shell(false)}</div>${pip(false)}`,
    parked: () => `<div class="sp-split">${chat(false)}${shell(true)}</div>${pip(true)}`,
  };
  for (const win of document.querySelectorAll(".sp-window")) {
    win.innerHTML = `
      <div class="penna-lights"><i></i><i></i><i></i></div>
      <header data-penna="titlebar">
        <div data-penna="tab.strip">
          <div data-penna="tab.window" aria-selected="true" style="width: 180px"><i data-icon="sparkle"></i>zebrilus</div>
          <div data-penna="tab.window" style="width: 180px"><i data-icon="globe"></i>docs</div>
        </div>
      </header>
      ${layouts[win.dataset.layout]()}`;
  }

  const devices = [
    ["iPhone 18 Pro", "iOS 27.0", true],
    ["iPhone 17e", "iOS 27.0", true],
    ["iPad Pro 11-inch (M5)", "iOS 27.0", false],
    ["iPhone Air", "iOS 27.0", false],
  ];
  const rows = (current) => devices
    .map(([name, system, booted]) => `<div data-penna="menu-item"${name === current ? ` data-current data-state="selected"` : ""}><span class="sp-dot"${booted ? "" : " data-off"}></span><span${booted ? "" : ` class="sp-dim"`}>${name}</span><span class="sp-detail">${system}</span>${current ? `<i data-icon="check"></i>` : ""}</div>`)
    .join("");
  const solo = (inner) => `<div class="sp-solo"><section data-penna="pane" class="sp-pane" data-active>${inner}</section></div>`;
  const plain = { icon: "smartphone", name: "Simulator", actions: [["xmark", "Close pane"]] };
  const close = [["xmark", "Close pane"]];
  const states = [
    ["Nothing attached", "The pane opens on the list, booted devices first. Picking a shut down one boots it.",
      solo(`<div class="sp-list penna-menu">${rows(null)}</div>${header(plain)}`)],
    ["Picker", "The device name opens the same list.",
      solo(`<div class="sp-stagebody">${screen()}</div>${header(simulator({ actions: close, state: "current" }))}<div class="sp-menu" data-penna="popover"><div class="penna-menu">${rows("iPhone 18 Pro")}</div></div>`)],
    ["Booting", "",
      solo(`<div class="sp-stagebody"><div class="sp-ghost"><i data-icon="loader"></i></div></div>${header(simulator({ name: "iPhone Air", device: false, actions: close }))}`)],
    ["Landscape", "The screen fits the pane in both directions.",
      solo(`<div class="sp-stagebody">${screen(`data-wide style="--sp-aspect: 2622 / 1206"`)}</div>${header(simulator({ actions: close }))}`)],
    ["Narrow pane", "The three device buttons fold into the ellipsis first.",
      `<div class="sp-solo" style="width: 220px"><section data-penna="pane" class="sp-pane" data-active><div class="sp-stagebody">${screen()}</div>${header(simulator({ device: false, actions: [["ellipsis", "More"]] }))}</section></div>`],
    ["No Xcode", "",
      solo(`<div class="sp-stagebody"><div class="sp-note"><i data-icon="triangle-alert"></i>Xcode is not installed</div></div>${header(plain)}`)],
  ];
  const stage = document.querySelector("[data-states]");
  if (stage) stage.innerHTML = states.map(([name, note, pane]) => `<figure>${pane}<figcaption><b>${name}</b>${note}</figcaption></figure>`).join("");

  const settle = () => window.pennaKit?.roundAll?.();
  document.getElementById("penna-tokens")?.addEventListener("load", () => requestAnimationFrame(settle));
  addEventListener("load", () => requestAnimationFrame(settle));
  document.fonts?.ready.then(() => requestAnimationFrame(settle));
  document.addEventListener("penna-theme", () => setTimeout(settle, 200));
})();
