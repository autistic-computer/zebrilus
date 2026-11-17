(() => {
  const MARKS = {
    "zz-claude": "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'><path d='m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z'/></svg>",
    "zz-openai": "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'><path d='M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z'/></svg>",
  };
  const marks = document.createElement("style");
  marks.textContent = Object.entries(MARKS)
    .map(([name, svg]) => `[data-icon="${name}"] { --penna-icon: url("data:image/svg+xml,${encodeURIComponent(svg)}"); }`)
    .join("\n");
  document.head.append(marks);

  const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;");
  const ic = (name, extra = "") => `<i data-icon="${name}"${extra}></i>`;
  const spin = (size = "small") => `<span class="penna-spinner" data-size="${size}">${ic("loader")}</span>`;
  const MARK = { codex: "zz-openai", claude: "zz-claude" };
  const tile = (program, tone, size) => `<span class="zz-ptile" data-program="${program}"${size ? ` data-size="${size}"` : ""}>${ic(MARK[program])}${tone ? `<span class="zz-dot" data-tone="${tone}"></span>` : ""}</span>`;
  const ref = (text) => `<span class="zz-ref">${esc(text)}</span>`;
  const you = (html) => `<div class="penna-card penna-chat-bubble" data-penna="card.bubble" role="group"><span>${html}</span></div>`;
  const step = (glyph, label, detail = "", opts = {}) => `<div class="penna-chat-activity"${opts.clickable ? " data-clickable" : ""}><span class="penna-chat-glyph"${opts.tint ? ` data-tint="${opts.tint}"` : ""}>${glyph === "spin" ? spin() : ic(glyph)}</span><span class="penna-chat-label"${opts.strong ? " data-strong" : ""}>${label}</span>${detail ? `<span class="penna-chat-detail">${detail}</span>` : ""}${opts.end ?? ""}</div>`;
  const live = (label, time) => `<div class="penna-chat-activity"><span class="penna-chat-label">${label}</span><span class="penna-chat-detail">${time}</span></div>`;
  const quote = (text) => `<div class="zz-quote">${esc(text)}</div>`;
  const pointer = (x, y) => `<span class="zz-pointer" style="left: ${x}px; top: ${y}px"></span>`;
  const badge = (label, tone) => `<span class="penna-badge zz-tag" data-penna="badge" data-size="small"${tone ? ` data-tone="${tone}"` : ""}><span class="badge-text">${label}</span></span>`;
  const word = (text, tone) => `<span class="zz-word"${tone ? ` data-tone="${tone}"` : ""}>${text}</span>`;
  const time = (text) => (text ? `<span class="zz-time">${text}</span>` : "");
  const pipmark = `<i class="zz-pipmark" data-icon="picture-in-picture"></i>`;
  const states = (hover) => (hover ? ` data-state="hover"` : "");

  const PROMPT = `${ref("@codex")} can the retry loop in turn.rs send twice?`;
  const REPLY = "Yes. A retry scheduled before the ack lands resends the same prompt. Guard on turn id.";
  const COMMAND = "cargo test -p zz-daemon";
  const FAILURE = "stream disconnected before completion";
  const S = {
    working: { tone: "accent", word: "working", time: "42s", live: "Reading the retry loop" },
    asking: { tone: "warning", word: "asking", wordTone: "warning", time: "1m 5s" },
    allowed: { tone: "accent", word: "working", time: "1m 18s", live: `Running ${COMMAND}` },
    done: { tone: "success", word: "done", wordTone: "normal", time: "2m 31s" },
    failed: { tone: "danger", word: "failed", wordTone: "danger", time: "1m 12s" },
    closed: { tone: null, word: "closed", time: "58s", off: true },
  };

  const waiting = `<span class="zz-wait">${spin()}<span class="zz-word">waiting for claude</span></span>`;
  const askWell = () => `<div class="zz-ask" data-penna="well">${ic("square-terminal")}<span class="zz-mono">${COMMAND}</span>${waiting}</div>`;

  const take1 = (key, hover = false) => {
    const s = S[key];
    const head = `<div class="zz-agent penna-card" data-penna="card.activity" role="button"${s.off ? " data-off" : ""}${states(hover)}>${tile("codex", s.tone)}<span class="zz-agent-name">codex</span><span class="zz-sep">·</span>${word(s.word, s.wordTone)}<span class="zz-grow"></span>${time(s.time)}${s.off ? "" : pipmark}${hover ? pointer(200, 18) : ""}</div>`;
    const under = [];
    if (key === "asking") under.push(step("square-terminal", `<span class="zz-mono">${COMMAND}</span>`, "", { tint: "warning", end: `<span class="zz-grow"></span>${waiting}` }));
    if (key === "allowed" || key === "done") under.push(step("circle-check", `<span class="zz-mono">${COMMAND}</span>`, "allowed by claude"));
    if (key === "failed") under.push(step("triangle-alert", FAILURE, "", { tint: "danger" }));
    const rows = under.length ? `<div class="zz-under">${under.join("")}</div>` : "";
    return `<div class="zz-stack zz-rel">${head}${rows}${key === "done" ? quote(REPLY) : ""}</div>`;
  };

  const take2 = (key, hover = false) => {
    const s = S[key];
    const detail = {
      working: `<span>${s.live}</span>`,
      asking: `${word("asking", "warning")}`,
      allowed: `${word("allowed", "success")}<span class="zz-mono">${COMMAND}</span>`,
      done: `${word("done", "normal")}<span class="zz-sep">·</span>${word("1 allowed", "success")}`,
      failed: `${word("failed", "danger")}<span>${FAILURE}</span>`,
      closed: `${word("closed")}`,
    }[key];
    const glyph = key === "working" || key === "allowed" ? spin() : ic(MARK.codex);
    const line = `<div class="penna-chat-activity zz-one"${s.off ? " data-off" : " data-clickable"}${states(hover)} role="button"><span class="zz-mark">${glyph}</span><span class="penna-chat-label" data-strong>codex</span><span class="zz-one-detail">${detail}</span><span class="zz-grow"></span>${time(s.time)}${s.off ? "" : pipmark}${hover ? pointer(200, 12) : ""}</div>`;
    let grows = "";
    if (key === "asking") grows = `<div class="zz-grows">${askWell()}</div>`;
    if (key === "done") grows = `<div class="zz-grows">${quote(REPLY)}</div>`;
    return `<div class="zz-stack zz-rel">${line}${grows}</div>`;
  };

  const peekBody = (key) => {
    const tail = [
      `<p>Looking at how a retry is scheduled against the ack.</p>`,
      step("file", "Read turn.rs"),
      step("search", "Searched for ack_turn", "crates/zz-daemon"),
    ];
    switch (key) {
      case "working":
        return `<div class="zz-peek-body">${tail.join("")}${live("Reading the retry loop", "42s")}</div>`;
      case "asking":
        return `<div class="zz-peek-body">${tail.slice(1).join("")}${askWell()}</div>`;
      case "allowed":
        return `<div class="zz-peek-body">${tail.slice(1).join("")}${step("spin", `<span class="zz-mono">${COMMAND}</span>`, "", { strong: true, end: `<span class="zz-grow"></span>${badge("allowed by claude", "success")}` })}</div>`;
      case "done":
        return `<div class="zz-peek-body" data-fit>${quote(REPLY)}</div>`;
      case "failed":
        return `<div class="zz-peek-body">${tail.slice(1).join("")}${step("triangle-alert", FAILURE, "", { tint: "danger" })}</div>`;
      default:
        return "";
    }
  };
  const take3 = (key, hover = false) => {
    const s = S[key];
    const extra = key === "done" ? `${badge("1 allowed", "success")}` : "";
    const head = `<div class="zz-peek-head">${tile("codex", s.tone)}<span class="zz-agent-name">codex</span><span class="zz-sep">·</span>${word(s.word, s.wordTone)}<span class="zz-grow"></span>${extra}${time(s.time)}${s.off ? "" : pipmark}</div>`;
    return `<div class="zz-rel"><div class="zz-peek penna-card" data-penna="card.activity" role="button"${s.off ? " data-off" : ""}${states(hover)}>${head}${peekBody(key)}</div>${hover ? pointer(300, 70) : ""}</div>`;
  };

  const ring = (used) => `<span class="penna-chat-meter" style="--penna-chat-used: ${used / 100}"><span></span></span>`;
  const composer = ({ text = "", hint = "Message Claude Code…", send = false, over = "" } = {}) =>
    `<div class="penna-chat-composer">${over}<div class="penna-chat-field input-chrome" data-penna="field.composer"><button class="penna-button" data-penna="button.send" data-shape="icon" aria-label="Mention">${ic("plus")}</button><span class="penna-chat-input" role="textbox"${text ? "" : ` data-placeholder="${hint}"`}>${text}</span>${ring(34)}<button class="penna-button" data-penna="button.send" data-shape="icon"${send ? ` data-emphasis="solid" data-solid="primary"` : " disabled"} aria-label="Send">${ic("arrow-up")}</button></div></div>`;
  const foot = `<div class="penna-chat-frame-foot"><div></div><button class="penna-button penna-chat-chrome" data-penna="button.quiet" data-size="small"><i class="zz-dim" data-icon="hand"></i><span class="penna-label">Opus 5.5</span><span class="zz-dim">High</span>${ic("chevron-down")}</button></div>`;
  const frame = (opts) => `<div class="penna-chat-frame"><div class="penna-chat-frame-top">${composer(opts)}</div>${foot}</div>`;

  const round = () => {
    window.pennaKit?.roundAll?.();
    globalThis.pennaKbd?.fill?.(document);
  };
  addEventListener("load", round);

  window.zzm = { esc, ic, spin, tile, ref, you, step, live, quote, pointer, badge, word, time, pipmark, composer, frame, foot, ring, takes: [take1, take2, take3], PROMPT, REPLY, COMMAND, MARK, S, round };
})();
