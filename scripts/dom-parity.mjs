#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const USAGE = `usage: scripts/dom-parity.mjs [options]

Loads the ala web client and the HTML client against the same daemon in
headless Chrome, then matches every named node in the ala accessibility tree
(role and name) with the HTML client's named elements (buttons by title or
aria-label, text by [data-text]) and reports what is missing or moved.

options:
  --ala <url>        ala web client (default http://127.0.0.1:8093/)
  --dom <url>         HTML client (default http://127.0.0.1:8092/)
  --mode <dark|light> color scheme for both (default dark)
  --theme <id>        interface theme for the HTML client (default default)
  --width <px>        viewport width (default 1400)
  --height <px>       viewport height (default 900)
  --threshold <px>    largest allowed offset (default 2)
  --out <dir>         also save ala.png and dom.png there
  --ala-eval <js>    run in the ala page before collecting
  --dom-eval <js>     run in the HTML page before collecting
  --settle <ms>       wait after setup (default 600)
  --report-only       exit 0 even when nodes differ
  --timeout <ms>      give up after this long (default 90000)

environment: CHROME overrides the browser binary.`;

function parseArgs(argv) {
    const options = {
        ala: "http://127.0.0.1:8093/",
        dom: "http://127.0.0.1:8092/",
        mode: "dark",
        theme: "default",
        width: 1400,
        height: 900,
        threshold: 2,
        out: "",
        "ala-eval": "",
        "dom-eval": "",
        settle: 600,
        "report-only": false,
        timeout: 90000,
    };
    for (let i = 0; i < argv.length; i++) {
        const arg = argv[i];
        if (arg === "-h" || arg === "--help") {
            console.log(USAGE);
            process.exit(0);
        }
        if (!arg.startsWith("--")) throw new Error(USAGE);
        const key = arg.slice(2);
        if (!(key in options)) throw new Error(`unknown option ${arg}`);
        if (typeof options[key] === "boolean") options[key] = true;
        else options[key] = typeof options[key] === "number" ? Number(argv[++i]) : argv[++i];
    }
    return options;
}

function chromeBinary() {
    if (process.env.CHROME) return process.env.CHROME;
    if (process.platform === "darwin") return "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    return "google-chrome";
}

function launch(width, height) {
    const profile = mkdtempSync(join(tmpdir(), "zz-dom-parity-"));
    const args = [
        "--headless=new",
        "--remote-debugging-port=0",
        `--user-data-dir=${profile}`,
        "--enable-unsafe-webgpu",
        "--hide-scrollbars",
        "--no-first-run",
        "--no-default-browser-check",
        "--use-mock-keychain",
        "--password-store=basic",
        `--window-size=${width},${height}`,
        "--force-device-scale-factor=2",
        "about:blank",
    ];
    if (process.platform === "darwin") args.splice(4, 0, "--use-angle=metal");
    if (process.platform === "linux") args.splice(4, 0, "--enable-features=Vulkan", "--use-angle=vulkan");
    const child = spawn(chromeBinary(), args, { stdio: ["ignore", "ignore", "pipe"] });
    const endpoint = new Promise((resolve, reject) => {
        let buffer = "";
        child.stderr.on("data", (chunk) => {
            buffer += chunk;
            const match = buffer.match(/DevTools listening on (ws:\/\/\S+)/);
            if (match) resolve(match[1]);
        });
        child.on("exit", (code) => reject(new Error(`chrome exited with ${code} before DevTools started`)));
    });
    const stop = () => {
        child.kill("SIGKILL");
        try {
            rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
        } catch {}
    };
    return { endpoint, stop };
}

function connect(url) {
    const socket = new WebSocket(url);
    let next = 1;
    const pending = new Map();
    socket.addEventListener("message", (event) => {
        const message = JSON.parse(event.data);
        if (message.id && pending.has(message.id)) {
            const { resolve, reject } = pending.get(message.id);
            pending.delete(message.id);
            if (message.error) reject(new Error(message.error.message));
            else resolve(message.result);
        }
    });
    const opened = new Promise((resolve, reject) => {
        socket.addEventListener("open", resolve, { once: true });
        socket.addEventListener("error", reject, { once: true });
    });
    const send = (method, params = {}, sessionId) =>
        new Promise((resolve, reject) => {
            const id = next++;
            pending.set(id, { resolve, reject });
            socket.send(JSON.stringify({ id, method, params, sessionId }));
        });
    return { opened, send, close: () => socket.close() };
}

async function page(cdp, url, options) {
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const send = (method, params) => cdp.send(method, params, sessionId);
    await send("Runtime.enable");
    await send("Page.enable");
    await send("Emulation.setEmulatedMedia", {
        features: [{ name: "prefers-color-scheme", value: options.mode }],
    });
    await send("Page.navigate", { url });
    const evaluate = async (expression) => {
        const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
        if (result.exceptionDetails) {
            throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
        }
        return result.result.value;
    };
    const shot = async () => Buffer.from((await send("Page.captureScreenshot", { format: "png" })).data, "base64");
    const front = () => send("Page.bringToFront");
    return { evaluate, shot, front };
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const ALA_READY = `(async () => {
    for (let i = 0; i < 300 && !(globalThis.ala && ala.windows().length); i++) await new Promise((r) => setTimeout(r, 100));
    if (!globalThis.ala) return "no ala";
    if (!ala.windows().length) return "no windows";
    await Promise.race([ala.enableAccessibility(), new Promise((r) => setTimeout(r, 2000))]);
    let names = [];
    for (let i = 0; i < 100; i++) {
        await Promise.race([ala.idle(), new Promise((r) => setTimeout(r, 500))]);
        names = ala.find({}).map((node) => node.name || node.text);
        if (names.includes("zz daemon")) return true;
        await new Promise((r) => setTimeout(r, 100));
    }
    return "no tree: " + names.slice(0, 12).join(", ");
})()`;

const DOM_READY = `(async () => {
    for (let i = 0; i < 200; i++) {
        if (globalThis.zzDom?.state?.tree && document.querySelector('[data-zz="sidebar.row"]')) return true;
        await new Promise((r) => setTimeout(r, 50));
    }
    return false;
})()`;

const ALA_NODES = `(async () => {
    await Promise.race([ala.idle(), new Promise((r) => setTimeout(r, 500))]);
    return ala.find({}).filter((node) => node.bounds && (node.name || node.text)).map((node) => ({
        role: node.role === "button" ? "button" : "text",
        name: String(node.name || node.text).trim(),
        x: node.bounds.x, y: node.bounds.y, w: node.bounds.width, h: node.bounds.height,
    }));
})()`;

const DOM_NODES = `(() => {
    const nodes = [];
    for (const element of document.querySelectorAll("button[title], [aria-label]:not(.ime), [data-text]")) {
        if (element.closest("[data-zz-switcher]")) continue;
        let rect = element.getBoundingClientRect();
        if (!rect.width || !rect.height) continue;
        if (element.matches("[data-text]")) {
            const range = document.createRange();
            range.selectNodeContents(element);
            const text = range.getBoundingClientRect();
            if (text.width) rect = { x: text.x, y: rect.y, width: text.width, height: rect.height };
        }
        const button = element.matches("button, [role=button]");
        const name = (element.getAttribute("aria-label") || element.getAttribute("title") || element.textContent || "").trim();
        if (!name) continue;
        nodes.push({ role: button ? "button" : "text", name, x: rect.x, y: rect.y, w: rect.width, h: rect.height });
    }
    return nodes;
})()`;

function key(node) {
    return `${node.role}:${node.name}`;
}

function compare(ala, dom, threshold) {
    const order = (a, b) => a.y - b.y || a.x - b.x;
    const pools = new Map();
    for (const node of [...dom].sort(order)) {
        if (!pools.has(key(node))) pools.set(key(node), []);
        pools.get(key(node)).push(node);
    }
    const matched = [];
    const missing = [];
    for (const node of [...ala].sort(order)) {
        const pool = pools.get(key(node));
        const twin = pool?.shift();
        if (!twin) {
            missing.push(node);
            continue;
        }
        const offset = Math.max(
            Math.abs(node.x - twin.x),
            Math.abs(node.y - twin.y),
            Math.abs(node.x + node.w - twin.x - twin.w),
            Math.abs(node.y + node.h - twin.y - twin.h),
        );
        matched.push({ ...node, dx: twin.x - node.x, dy: twin.y - node.y, dw: twin.w - node.w, dh: twin.h - node.h, offset, moved: offset > threshold });
    }
    const extra = [...pools.values()].flat();
    return { matched, missing, extra };
}

function round(value) {
    return Math.round(value * 10) / 10;
}

async function main() {
    const options = parseArgs(process.argv.slice(2));
    const chrome = launch(options.width, options.height);
    const deadline = setTimeout(() => {
        console.error(`timed out after ${options.timeout} ms`);
        chrome.stop();
        process.exit(1);
    }, options.timeout);
    try {
        const cdp = connect(await chrome.endpoint);
        await cdp.opened;
        const domUrl = new URL(options.dom);
        domUrl.searchParams.set("theme", options.theme);
        domUrl.searchParams.set("mode", options.mode);
        domUrl.searchParams.set("bare", "");
        const ala = await page(cdp, options.ala, options);
        const dom = await page(cdp, domUrl.toString(), options);
        await ala.front();
        console.error("waiting for the ala client");
        const ready = await ala.evaluate(ALA_READY);
        if (ready !== true) throw new Error(`the ala client never showed its tree (${ready})`);
        if (options["ala-eval"]) await ala.evaluate(`(async () => { ${options["ala-eval"]} })()`);
        await sleep(options.settle);
        const alaNodes = await ala.evaluate(ALA_NODES);
        const alaShot = options.out ? await ala.shot() : null;
        await dom.front();
        console.error("waiting for the HTML client");
        if (!(await dom.evaluate(DOM_READY))) throw new Error("the HTML client never showed its tree");
        if (options["dom-eval"]) await dom.evaluate(`(async () => { ${options["dom-eval"]} })()`);
        await sleep(options.settle);
        const result = compare(alaNodes, await dom.evaluate(DOM_NODES), options.threshold);
        if (options.out) {
            mkdirSync(options.out, { recursive: true });
            writeFileSync(join(options.out, "ala.png"), alaShot);
            writeFileSync(join(options.out, "dom.png"), await dom.shot());
        }
        for (const node of result.matched) {
            const mark = node.moved ? "MOVED" : "ok   ";
            console.log(`${mark} ${node.role.padEnd(6)} ${node.name.slice(0, 40).padEnd(40)} at ${round(node.x)},${round(node.y)} ${round(node.w)}x${round(node.h)}  d ${round(node.dx)},${round(node.dy)} ${round(node.dw)}x${round(node.dh)}`);
        }
        for (const node of result.missing) {
            console.log(`MISS  ${node.role.padEnd(6)} ${node.name.slice(0, 40).padEnd(40)} at ${round(node.x)},${round(node.y)} ${round(node.w)}x${round(node.h)}`);
        }
        for (const node of result.extra) {
            console.log(`EXTRA ${node.role.padEnd(6)} ${node.name.slice(0, 40).padEnd(40)} at ${round(node.x)},${round(node.y)} ${round(node.w)}x${round(node.h)}`);
        }
        const moved = result.matched.filter((node) => node.moved).length;
        console.log(`\n${result.matched.length - moved} match, ${moved} moved, ${result.missing.length} missing, ${result.extra.length} extra`);
        cdp.close();
        if (!options["report-only"] && (moved || result.missing.length)) process.exitCode = 1;
    } finally {
        clearTimeout(deadline);
        chrome.stop();
    }
}

main().catch((error) => {
    console.error(error.message);
    process.exit(1);
});
