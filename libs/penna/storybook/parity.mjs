#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { createReadStream, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, extname, join, normalize, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const THEMES = ["default", "macos", "yaru"];
const MODES = ["light", "dark"];
const VIEWPORT = { width: 1200, height: 900 };
const SCALE = 2;
const STEP_PAD = 4;
const USAGE = `usage: node storybook/parity.mjs <story>...|--all [options]

Compares each story's hand-written DOM page (storybook/src/kit/<story>/<page>) with
the penna render of its storybook story, section by section, on every
interface theme in light and dark, and scores each section 0..100.
Use Rust story module names, such as buttons, selection and markdown.

A section scores 100 x sqrt(geometry x pixels) x height factor. Geometry gives
each canvas node credit 1 - offset/8 px, offsets taken against the previous
node so one wrong width counts once. Pixels is 1 - mismatched / canvas ink,
compared after a 0.5px blur at 3 levels per channel. Steps (hover, press, key)
score only the target's box and also need the DOM to change where the canvas did.

A step targets a node by id ("hover": "ok"), the nth node sharing an id
({"id": "ok", "nth": 2}, counted from 1 in tree order), a point in the section
({"at": [x, y]} in CSS px from its top-left corner) or the centre of a DOM
element ({"selector": ".thumb"}), whose section-relative point the canvas gets
too. Point and selector steps score the whole section.

Each story loads fresh. A step that leaves its canvas section
changed reloads both pages before the next step and the next theme and mode;
"keep": true runs a step on that state instead (key steps keep by default,
"keep": false starts one fresh).

The DOM page loads with ?bare&flat&motion=0 and prefers-reduced-motion: reduce.

options:
  --all              every storybook/src/kit/<story>/parity.json
  --theme <t>        default, macos or yaru (default all three)
  --mode <m>         light or dark (default both)
  --section <s>      only this section
  --threshold <px>   offset reported as off (default 2)
  --update           write storybook/src/kit/<story>/parity.baseline.json (not when --check fails)
  --check            exit 1 when a score drops more than 0.5 below the baseline or has none
  --out <dir>        output folder (default target/parity)
  --timeout <ms>     give up on a story after this long (default 900000)

environment: CHROME overrides the browser binary.`;

function parseArgs(argv) {
    const options = { components: [], all: false, theme: "", mode: "", section: "", threshold: 2, update: false, check: false, out: join(ROOT, "target/parity"), timeout: 900000 };
    for (let i = 0; i < argv.length; i++) {
        const arg = argv[i];
        if (arg === "-h" || arg === "--help") {
            console.log(USAGE);
            process.exit(0);
        }
        if (!arg.startsWith("--")) {
            options.components.push(arg);
            continue;
        }
        const key = arg.slice(2);
        if (!(key in options) || key === "components") throw new Error(`unknown option ${arg}\n\n${USAGE}`);
        if (typeof options[key] === "boolean") options[key] = true;
        else if (typeof options[key] === "number") options[key] = Number(argv[++i]);
        else options[key] = argv[++i];
    }
    if (options.all) {
        options.components = readdirSync(join(ROOT, "storybook/src/kit")).filter((name) => existsSync(join(ROOT, "storybook/src/kit", name, "parity.json"))).sort();
    }
    if (!options.components.length) throw new Error(USAGE);
    for (const name of options.components) {
        if (!/^[\w-]+$/.test(name)) throw new Error(`bad component name ${name}`);
    }
    options.out = resolve(options.out);
    return options;
}

const cleanups = [];
function cleanup() {
    while (cleanups.length) {
        try {
            cleanups.pop()();
        } catch {}
    }
}
process.on("exit", cleanup);
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, () => {
        cleanup();
        process.exit(130);
    });
}

const PAGE_FILES = new Set([".html", ".css", ".js", ".mjs"]);

function newest(path, keep = () => true) {
    if (!existsSync(path)) return 0;
    const stat = statSync(path);
    if (!stat.isDirectory()) return keep(path) ? stat.mtimeMs : 0;
    return readdirSync(path).reduce((max, name) => Math.max(max, newest(join(path, name), keep)), 0);
}

function oldest(paths) {
    return Math.min(...paths.map((path) => (existsSync(path) ? statSync(path).mtimeMs : 0)));
}

function rustInput(path) {
    const name = path.split(sep).pop();
    return !PAGE_FILES.has(extname(path)) && !name.startsWith("parity.");
}

function prepare() {
    const kit = Math.max(newest(join(ROOT, "src"), rustInput), newest(join(ROOT, "themes")), newest(join(ROOT, "examples/theme_css.rs")), newest(join(ROOT, "Cargo.toml")), newest(join(ROOT, "Cargo.lock")));
    const tokens = THEMES.flatMap((theme) => [`${theme}-icons`, ...MODES.map((mode) => `${theme}-${mode}`)]).map((name) => join(ROOT, "web/tokens", `${name}.css`));
    if (oldest(tokens) < kit) {
        console.error("generating web/tokens");
        const run = spawnSync("cargo", ["run", "--quiet", "--example", "theme_css"], { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] });
        if (run.status !== 0) throw new Error("cargo run --example theme_css failed");
    }
    const story = join(ROOT, "storybook");
    const sources = Math.max(kit, ...["src", "web", "fonts", "icons", "Cargo.toml", "Cargo.lock", "build.sh"].map((name) => newest(join(story, name))));
    const built = oldest(["wasm/storybook_bg.wasm", "wasm/storybook.js", "index.html", "main.js", "style.css"].map((name) => join(story, "dist", name)));
    if (built < sources) {
        console.error("building the storybook");
        const run = spawnSync(join(story, "build.sh"), [], { cwd: ROOT, stdio: ["ignore", "inherit", "inherit"] });
        if (run.status !== 0) throw new Error("storybook/build.sh failed");
    }
}

const MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".mjs": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json",
    ".wasm": "application/wasm",
    ".ttf": "font/ttf",
    ".otf": "font/otf",
    ".woff2": "font/woff2",
    ".svg": "image/svg+xml",
    ".png": "image/png",
};

function serve(root, allowed, cache = "no-store") {
    const server = createServer((request, response) => {
        const path = normalize(decodeURIComponent(new URL(request.url, "http://x").pathname));
        let file = join(root, path);
        const inside = relative(root, file);
        if (inside.startsWith("..") || !allowed(inside.split(sep).join("/"))) {
            response.writeHead(403).end();
            return;
        }
        if (existsSync(file) && statSync(file).isDirectory()) file = join(file, "index.html");
        if (!existsSync(file)) {
            response.writeHead(404).end();
            return;
        }
        response.writeHead(200, { "content-type": MIME[extname(file)] ?? "application/octet-stream", "cache-control": cache });
        createReadStream(file).pipe(response);
    });
    return new Promise((done) => server.listen(0, "127.0.0.1", () => {
        cleanups.push(() => server.close());
        server.unref();
        done(`http://127.0.0.1:${server.address().port}`);
    }));
}

function domFiles() {
    const prefixes = ["web/", "storybook/fonts/"];
    return (path) =>
        prefixes.some((prefix) => path.startsWith(prefix)) ||
        ((path.startsWith("src/components/") || path.startsWith("src/blocks/") || path.startsWith("storybook/src/kit/")) && PAGE_FILES.has(extname(path)) && !path.split("/").pop().startsWith("parity."));
}

function chromeBinaries() {
    if (process.env.CHROME) return [process.env.CHROME];
    if (process.platform === "darwin") return ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
    return ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"];
}

function chromeBinary() {
    const all = chromeBinaries();
    if (all.length === 1) return all[0];
    const found = all.find((name) => spawnSync("sh", ["-c", `command -v ${name}`], { stdio: "ignore" }).status === 0);
    if (!found) throw new Error(`no browser found (tried ${all.join(", ")}); set CHROME`);
    return found;
}

function alive(pid) {
    try {
        process.kill(pid, 0);
        return true;
    } catch (error) {
        return error.code === "EPERM";
    }
}

function sweepProfiles() {
    for (const name of readdirSync(tmpdir())) {
        const match = name.match(/^penna-parity-(\d+)-/);
        if (match && !alive(Number(match[1]))) rmSync(join(tmpdir(), name), { recursive: true, force: true });
    }
}

function launch() {
    sweepProfiles();
    const binary = chromeBinary();
    const profile = mkdtempSync(join(tmpdir(), `penna-parity-${process.pid}-`));
    const args = [
        "--headless=new",
        "--remote-debugging-pipe",
        `--user-data-dir=${profile}`,
        "--enable-unsafe-webgpu",
        "--hide-scrollbars",
        "--no-first-run",
        "--no-default-browser-check",
        "--use-mock-keychain",
        "--password-store=basic",
        "--font-render-hinting=none",
        `--window-size=${VIEWPORT.width},${VIEWPORT.height}`,
        `--force-device-scale-factor=${SCALE}`,
        "about:blank",
    ];
    if (process.platform === "darwin") args.splice(4, 0, "--use-angle=metal");
    if (process.platform === "linux") args.splice(4, 0, "--enable-features=Vulkan", "--use-angle=vulkan");
    const child = spawn(binary, args, { stdio: ["ignore", "ignore", "ignore", "pipe", "pipe"] });
    cleanups.push(() => {
        child.kill("SIGKILL");
        rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    });
    const failed = new Promise((_, fail) => {
        child.on("error", (error) => fail(new Error(`cannot start ${binary}: ${error.message}`)));
        child.on("exit", (code, signal) => fail(new Error(`chrome exited (${signal ?? code})`)));
    });
    failed.catch(() => {});
    return { input: child.stdio[3], output: child.stdio[4], failed };
}

function connect({ input, output, failed }) {
    let next = 1;
    const pending = new Map();
    const listeners = new Set();
    let buffer = "";
    output.setEncoding("utf8");
    output.on("data", (chunk) => {
        buffer += chunk;
        let end;
        while ((end = buffer.indexOf("\0")) >= 0) {
            const message = JSON.parse(buffer.slice(0, end));
            buffer = buffer.slice(end + 1);
            if (message.id && pending.has(message.id)) {
                const { done, fail } = pending.get(message.id);
                pending.delete(message.id);
                if (message.error) fail(new Error(message.error.message));
                else done(message.result);
            } else if (message.method) {
                for (const listener of listeners) listener(message);
            }
        }
    });
    input.on("error", () => {});
    failed.catch((error) => {
        for (const { fail } of pending.values()) fail(error);
        pending.clear();
    });
    const send = (method, params = {}, sessionId) =>
        new Promise((done, fail) => {
            const id = next++;
            pending.set(id, { done, fail });
            input.write(`${JSON.stringify({ id, method, params, sessionId })}\0`);
        });
    return { send, listeners, failed };
}

async function tab(cdp) {
    const { targetId } = await Promise.race([cdp.send("Target.createTarget", { url: "about:blank" }), cdp.failed]);
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const send = (method, params) => cdp.send(method, params, sessionId);
    await send("Runtime.enable");
    await send("Page.enable");
    await send("Emulation.setDeviceMetricsOverride", { ...VIEWPORT, deviceScaleFactor: SCALE, mobile: false });
    await send("Emulation.setFocusEmulationEnabled", { enabled: true });
    const front = async () => {
        if (cdp.front === sessionId) return;
        await send("Page.bringToFront");
        cdp.front = sessionId;
    };
    const evaluate = async (expression) => {
        await front();
        const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
        if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
        return result.result.value;
    };
    const call = (fn, ...args) => evaluate(`(${fn})(...${JSON.stringify(args)})`);
    const media = (mode) =>
        send("Emulation.setEmulatedMedia", {
            features: [
                { name: "prefers-color-scheme", value: mode },
                { name: "prefers-reduced-motion", value: "reduce" },
            ],
        });
    const navigate = async (url, mode) => {
        await media(mode);
        const loaded = new Promise((done) => {
            const listener = (message) => {
                if (message.sessionId === sessionId && message.method === "Page.loadEventFired") {
                    cdp.listeners.delete(listener);
                    done();
                }
            };
            cdp.listeners.add(listener);
        });
        await front();
        await send("Page.navigate", { url });
        await loaded;
    };
    const mouse = async (type, x, y) => (await front(), send("Input.dispatchMouseEvent", { type, x, y, button: type === "mouseMoved" ? "none" : "left", buttons: type === "mousePressed" ? 1 : 0, clickCount: 1 }));
    const key = async (name) => {
        const codes = { Backspace: 8, Tab: 9, Enter: 13, Escape: 27, " ": 32, PageUp: 33, PageDown: 34, End: 35, Home: 36, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Delete: 46 };
        await front();
        const base = { key: name, code: name === " " ? "Space" : name, windowsVirtualKeyCode: codes[name] ?? 0 };
        await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
        if (name.length === 1) await send("Input.dispatchKeyEvent", { type: "char", text: name, ...base });
        await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
    };
    const shot = async (clip) => {
        await front();
        const result = await send("Page.captureScreenshot", { format: "png", clip: { ...clip, scale: 1 }, captureBeyondViewport: true });
        return `data:image/png;base64,${result.data}`;
    };
    return { send, evaluate, call, media, navigate, mouse, key, shot };
}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

const SETTLE = `globalThis.parityIdle ??= async () => {
    const quiet = () => Promise.race([ala.idle(), new Promise((r) => setTimeout(r, 200))]);
    const sizes = () => JSON.stringify(ala.windows().map((each) => [each.width, each.height]));
    let last = "";
    for (let i = 0; i < 40; i++) {
        await quiet();
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        const now = sizes();
        if (now === last) return;
        last = now;
    }
}`;

async function canvasReady(page, story) {
    await page.evaluate(SETTLE);
    const ready = await page.call(async (story) => {
        for (let i = 0; i < 600 && !(globalThis.storybook && globalThis.ala && ala.windows().length); i++) await new Promise((r) => setTimeout(r, 50));
        if (!globalThis.storybook) return "no storybook";
        if (storybook.current() !== story) await Promise.race([storybook.show(story), new Promise((r) => setTimeout(r, 200))]);
        if (storybook.current() !== story) return `no story ${story}`;
        await Promise.race([ala.enableAccessibility({ interactive: false }), new Promise((r) => setTimeout(r, 200))]);
        await document.fonts.ready;
        await parityIdle();
        return true;
    }, story);
    if (ready !== true) throw new Error(`storybook: ${ready}`);
}

async function canvasSections(page, only) {
    return page.call(async (only) => {
        await parityIdle();
        const sections = [];
        for (const info of ala.windows()) {
            const mount = info.mount && document.querySelector(info.mount);
            const section = mount?.dataset.section;
            if (!section || (only && section !== only)) continue;
            const origin = mount.querySelector("canvas").getBoundingClientRect();
            const nodes = [];
            const walk = (node) => {
                if (!node) return;
                const id = node.id ?? (node.name && node.role !== "text" ? `${node.role}:${node.name}` : null);
                if (id && node.bounds) {
                    nodes.push({ id, x: node.bounds.x - origin.x, y: node.bounds.y - origin.y, width: node.bounds.width, height: node.bounds.height });
                }
                for (const child of node.children ?? []) walk(child);
            };
            walk(ala.tree(info.id));
            sections.push({ section, window: info.id, width: info.width, height: info.height, nodes, png: await ala.capture(info.id) });
        }
        return sections;
    }, only);
}

async function canvasPoint(page, section, target) {
    return page.call(async (section, target) => {
        const info = ala.windows().find((each) => each.mount && document.querySelector(each.mount)?.dataset.section === section);
        if (!info) return null;
        const locate = () => {
            if (target.at) {
                const origin = document.querySelector(info.mount).querySelector("canvas").getBoundingClientRect();
                return { x: origin.x + target.at[0], y: origin.y + target.at[1] };
            }
            let seen = 0;
            let found = null;
            const walk = (node) => {
                if (!node || found) return;
                const id = node.id ?? (node.name && node.role !== "text" ? `${node.role}:${node.name}` : null);
                if (id === target.id && node.bounds && ++seen === target.nth) found = node.bounds;
                for (const child of node.children ?? []) walk(child);
            };
            walk(ala.tree(info.id));
            return found && { x: found.x + found.width / 2, y: found.y + found.height / 2 };
        };
        const point = locate();
        if (!point) return null;
        scrollBy(0, point.y - innerHeight / 2);
        await parityIdle();
        return locate();
    }, section, target);
}

async function domReady(page) {
    const ready = await page.call(async () => {
        for (let i = 0; i < 200 && !(document.readyState === "complete" && document.getElementById("penna-tokens")?.sheet && document.getElementById("penna-icons")?.sheet); i++) {
            await new Promise((r) => setTimeout(r, 25));
        }
        if (!document.getElementById("penna-tokens")?.sheet) return "token stylesheet never loaded (kit.js and data-tokens?)";
        document.head.append(Object.assign(document.createElement("style"), { textContent: "*, *::before, *::after { transition: none !important; }" }));
        await document.fonts.ready;
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        return true;
    });
    if (ready !== true) throw new Error(`dom page: ${ready}`);
}

const FORBIDDEN = async (sections) => {
    const unquote = (text) => text.replace(/\\(.)/gs, "$1");
    const urls = (text, base) =>
        [...text.matchAll(/url\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|([^)"'\s]*))\s*\)/g)].map((match) => {
            const raw = unquote(match[1] ?? match[2] ?? match[3]);
            try {
                return new URL(raw, base).href;
            } catch {
                return raw;
            }
        });
    const allowed = new Set();
    for (const sheet of document.styleSheets) {
        if (!sheet.href || !new URL(sheet.href).pathname.startsWith("/web/")) continue;
        const walk = (rules) => {
            for (const rule of rules) {
                if (rule.cssRules) walk(rule.cssRules);
                if (rule.type === CSSRule.FONT_FACE_RULE) continue;
                for (const url of urls(rule.cssText, sheet.href)) allowed.add(url);
            }
        };
        try {
            walk(sheet.cssRules);
        } catch {}
    }
    const PROPS = ["background-image", "mask-image", "-webkit-mask-image", "mask-border-source", "-webkit-mask-box-image-source", "border-image-source", "list-style-image", "content", "filter", "backdrop-filter", "-webkit-box-reflect"];
    const TAGS = "img, picture, canvas, iframe, frame, object, embed, video, audio, image, feImage, foreignObject";
    const name = (element) => `${element.localName}${element.dataset?.id ? `[data-id=${element.dataset.id}]` : ""}`;
    const global = [...document.querySelectorAll("feImage, use")].filter((element) => element.localName === "feImage" || !(element.getAttribute("href") ?? element.getAttribute("xlink:href") ?? "#").startsWith("#"));
    const out = {};
    for (const section of document.querySelectorAll("section[data-section]")) {
        if (sections && !sections.includes(section.dataset.section)) continue;
        const found = [...section.querySelectorAll(TAGS)].map((element) => `<${element.localName}>`);
        found.push(...global.map((element) => `<${element.localName}> in the page`));
        for (const element of [section, ...section.querySelectorAll("*")]) {
            for (const pseudo of [null, "::before", "::after", "::marker"]) {
                const style = getComputedStyle(element, pseudo);
                for (const prop of PROPS) {
                    for (const url of urls(style.getPropertyValue(prop), location.href)) {
                        if (!allowed.has(url)) found.push(`${prop} url on ${name(element)}${pseudo ?? ""}`);
                    }
                }
            }
        }
        if (found.length) out[section.dataset.section] = [...new Set(found)].slice(0, 4);
    }
    return out;
};

async function domSections(page, widths, only) {
    const sections = await page.call(async (widths, only, scale) => {
        const all = [...document.querySelectorAll("section[data-section]")].filter((element) => !only || element.dataset.section === only);
        for (const element of all) {
            const width = widths[element.dataset.section];
            if (width) element.style.width = `${width}px`;
        }
        globalThis.pennaKit?.roundAll();
        await document.fonts.ready;
        await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        return all.map((element) => {
            const box = element.getBoundingClientRect();
            const shown = (node) => {
                const rect = node.getBoundingClientRect();
                return node.checkVisibility({ visibilityProperty: true }) && (rect.width > 0 || rect.height > 0);
            };
            const nodes = [...element.querySelectorAll("[data-id]")].filter(shown).map((node) => {
                const rect = node.getBoundingClientRect();
                return { id: node.dataset.id, x: rect.x - box.x, y: rect.y - box.y, width: rect.width, height: rect.height };
            });
            const snap = (value) => Math.round(value * scale) / scale;
            const [x0, y0, x1, y1] = [box.x + scrollX, box.y + scrollY, box.right + scrollX, box.bottom + scrollY].map(snap);
            return { section: element.dataset.section, width: box.width, height: box.height, clip: { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }, nodes };
        });
    }, widths, only, SCALE);
    const forbidden = await page.call(FORBIDDEN, sections.map((section) => section.section));
    for (const section of sections) {
        section.forbidden = forbidden[section.section] ?? null;
        section.png = section.clip.width && section.clip.height ? await page.shot(section.clip) : null;
    }
    return sections;
}

async function domPoint(page, section, target) {
    return page.call((section, target) => {
        const root = document.querySelector(`section[data-section="${CSS.escape(section)}"]`);
        if (!root) return null;
        const locate = () => {
            const box = root.getBoundingClientRect();
            if (target.at) return { x: box.x + target.at[0], y: box.y + target.at[1] };
            const element = root.querySelectorAll(target.selector ?? `[data-id="${CSS.escape(target.id)}"]`)[target.nth - 1];
            if (!element) return null;
            const rect = element.getBoundingClientRect();
            const x = Math.floor(rect.x + rect.width / 2);
            const y = Math.floor(rect.y + rect.height / 2);
            return { x, y, at: [x - box.x, y - box.y] };
        };
        const point = locate();
        if (!point) return null;
        scrollBy(0, point.y - innerHeight / 2);
        return locate();
    }, section, target);
}

function stepTarget(step) {
    const raw = step.hover ?? step.press ?? step.click;
    if (raw === undefined) return null;
    const target = typeof raw === "string" ? { id: raw } : raw;
    const kinds = ["id", "at", "selector"].filter((key) => target[key] !== undefined);
    const nth = target.nth ?? 1;
    const at = target.at;
    if (kinds.length !== 1 || !Number.isInteger(nth) || nth < 1 || (at && !(Array.isArray(at) && at.length === 2 && at.every(Number.isFinite)))) {
        throw new Error(`step ${step.name ?? JSON.stringify(step)}: a target is an id, {"id", "nth"}, {"at": [x, y]} or {"selector"}`);
    }
    return { ...target, nth };
}

function describeTarget(target) {
    if (target.at) return `@${target.at.join(",")}`;
    if (target.selector) return target.nth > 1 ? `${target.selector}#${target.nth}` : target.selector;
    return target.nth > 1 ? `${target.id}#${target.nth}` : target.id;
}

function overlaps(a0, a1, b0, b1) {
    return Math.min(a1, b1) - Math.max(a0, b0) > 0.5;
}

function geometry(canvas, dom, threshold) {
    const pool = new Map();
    for (const node of dom) {
        if (!pool.has(node.id)) pool.set(node.id, []);
        pool.get(node.id).push(node);
    }
    const matched = [];
    const missing = [];
    let previous = null;
    for (const node of canvas) {
        const twin = pool.get(node.id)?.shift();
        if (!twin) {
            missing.push(node.id);
            continue;
        }
        const d = { dx: twin.x - node.x, dy: twin.y - node.y, dw: twin.width - node.width, dh: twin.height - node.height };
        let ex = Math.abs(d.dx);
        let ey = Math.abs(d.dy);
        if (previous) {
            const [p, pd] = previous;
            const besideX = p.x + p.width <= node.x + 0.5 && overlaps(p.y, p.y + p.height, node.y, node.y + node.height);
            const belowY = p.y + p.height <= node.y + 0.5 && overlaps(p.x, p.x + p.width, node.x, node.x + node.width);
            ex = Math.min(ex, Math.abs(d.dx - pd.dx - (besideX ? pd.dw : 0)));
            ey = Math.min(ey, Math.abs(d.dy - pd.dy - (belowY ? pd.dh : 0)));
        }
        previous = [node, d];
        const offset = Math.max(ex, ey, Math.abs(d.dw), Math.abs(d.dh));
        const credit = Math.max(0, 1 - offset / 8);
        matched.push({ id: node.id, x: round(node.x, 2), y: round(node.y, 2), width: round(node.width, 2), height: round(node.height, 2), ...Object.fromEntries(Object.entries(d).map(([k, v]) => [k, round(v)])), offset: round(offset, 2), credit: round(credit, 3), ok: offset <= threshold });
    }
    const extra = [...pool.values()].flat().map((node) => node.id);
    const total = canvas.length + extra.length;
    const credit = matched.reduce((sum, node) => sum + node.credit, 0);
    return { score: total ? credit / total : null, matched, missing, extra };
}

const DIFF = async (canvasPng, domPng, canvasBeforePng, domBeforePng, box) => {
    const BLUR = 0.5;
    const TOLERANCE = 3;
    const INK = 2;
    const GROW = 2;
    const load = (src) => new Promise((done, fail) => {
        if (!src) return done(null);
        const image = new Image();
        image.onload = () => done(image);
        image.onerror = fail;
        image.src = src;
    });
    const images = await Promise.all([canvasPng, domPng, canvasBeforePng, domBeforePng].map(load));
    const width = Math.max(...images.map((image) => image?.width ?? 0));
    const height = Math.max(...images.map((image) => image?.height ?? 0));
    const pixels = (image, blur) => {
        const canvas = new OffscreenCanvas(width, height);
        const context = canvas.getContext("2d", { willReadFrequently: true });
        if (blur) context.filter = `blur(${blur}px)`;
        if (image) context.drawImage(image, 0, 0);
        return context.getImageData(0, 0, width, height).data;
    };
    const far = (p, i, q, j, limit) => Math.abs(p[i] - q[j]) > limit || Math.abs(p[i + 1] - q[j + 1]) > limit || Math.abs(p[i + 2] - q[j + 2]) > limit || Math.abs(p[i + 3] - q[j + 3]) > limit;
    const raw = pixels(images[0], 0);
    const canvas = pixels(images[0], BLUR);
    const dom = pixels(images[1], BLUR);
    const corner = (Math.min(4, height - 1) * width + Math.min(4, width - 1)) * 4;
    const background = raw.slice(corner, corner + 4);
    const count = width * height;
    let ink = new Uint8Array(count);
    for (let k = 0; k < count; k++) ink[k] = far(raw, k * 4, background, 0, INK);
    for (const [step, span] of [[1, width], [width, height]]) {
        const grown = new Uint8Array(count);
        for (let k = 0; k < count; k++) {
            if (!ink[k]) continue;
            const at = step === 1 ? k % width : Math.floor(k / width);
            for (let o = -GROW; o <= GROW; o++) if (at + o >= 0 && at + o < span) grown[k + o * step] = 1;
        }
        ink = grown;
    }
    const [x0, y0, x1, y1] = box ? [Math.max(0, Math.floor(box[0])), Math.max(0, Math.floor(box[1])), Math.min(width, Math.ceil(box[2])), Math.min(height, Math.ceil(box[3]))] : [0, 0, width, height];
    const before = images[2] ? [pixels(images[2], BLUR), pixels(images[3], BLUR)] : null;
    const out = new OffscreenCanvas(width, height);
    const context = out.getContext("2d");
    const image = context.createImageData(width, height);
    let inked = 0;
    let mismatched = 0;
    let canvasChanged = 0;
    let domChanged = 0;
    let bothChanged = 0;
    for (let k = 0; k < count; k++) {
        const i = k * 4;
        const x = k % width;
        const y = Math.floor(k / width);
        const inside = x >= x0 && x < x1 && y >= y0 && y < y1;
        const miss = inside && far(canvas, i, dom, i, TOLERANCE);
        if (inside) {
            inked += ink[k];
            mismatched += miss;
            if (before) {
                const c = far(canvas, i, before[0], i, TOLERANCE);
                const d = far(dom, i, before[1], i, TOLERANCE);
                canvasChanged += c;
                domChanged += d;
                bothChanged += c && d;
            }
        }
        const gray = (raw[i] + raw[i + 1] + raw[i + 2]) / 3;
        const dim = (inside ? 128 : 200) + (gray - 128) * 0.25;
        image.data.set(miss ? [255, 0, 64, 255] : [dim, dim, dim, 255], i);
    }
    context.putImageData(image, 0, 0);
    const blob = await out.convertToBlob({ type: "image/png" });
    const diff = await new Promise((done) => {
        const reader = new FileReader();
        reader.onload = () => done(reader.result);
        reader.readAsDataURL(blob);
    });
    return { width, height, ink: inked, mismatched, canvasChanged, domChanged, bothChanged, diff };
};

function round(value, digits = 1) {
    const factor = 10 ** digits;
    return Math.round(value * factor) / factor;
}

function writePng(path, url) {
    if (!url) return;
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, Buffer.from(url.slice(url.indexOf(",") + 1), "base64"));
}

async function compare(lab, canvas, dom, threshold, folder, name, step) {
    writePng(join(folder, `${name}.canvas.png`), canvas.png);
    const empty = { score: 0, matched: [], missing: canvas.nodes.map((node) => node.id), extra: [] };
    if (!dom) return { section: name, step: Boolean(step), score: 0, missingSection: true, geometry: empty, size: { canvas: [canvas.width, canvas.height], dom: null } };
    writePng(join(folder, `${name}.dom.png`), dom.png);
    const target = step?.target?.id ? step.target : null;
    const nth = (nodes) => (target ? nodes.filter((node) => node.id === target.id).slice(target.nth - 1, target.nth) : nodes);
    const canvasNode = target ? nth(canvas.nodes)[0] : null;
    const geo = geometry(nth(canvas.nodes), nth(dom.nodes), threshold);
    const box = canvasNode ? [canvasNode.x - STEP_PAD, canvasNode.y - STEP_PAD, canvasNode.x + canvasNode.width + STEP_PAD, canvasNode.y + canvasNode.height + STEP_PAD].map((value) => value * SCALE) : null;
    const pixels = await lab.call(DIFF, canvas.png, dom.png, step?.canvasBefore ?? null, step?.domBefore ?? null, box);
    writePng(join(folder, `${name}.diff.png`), pixels.diff);
    const pixelScore = Math.max(0, 1 - pixels.mismatched / Math.max(1, pixels.ink));
    const heightDelta = round(dom.height - canvas.height);
    const sizeFactor = Math.max(0, 1 - Math.abs(heightDelta) / Math.max(1, canvas.height));
    const notes = [];
    let parts = geo.score === null ? [pixelScore] : [geo.score, pixelScore];
    let change = null;
    if (step) {
        const union = pixels.canvasChanged + pixels.domChanged - pixels.bothChanged;
        change = union ? pixels.bothChanged / union : 0;
        parts = [...parts, change];
        if (!pixels.canvasChanged) notes.push("canvas did not change: the step tests nothing");
        if (target && !canvasNode) notes.push(`canvas has no node ${describeTarget(target)}`);
    }
    if (dom.forbidden) notes.push(`forbidden: ${dom.forbidden.join(", ")}`);
    const fails = dom.forbidden || (step && !pixels.canvasChanged);
    const mean = parts.reduce((product, value) => product * value, 1) ** (1 / parts.length);
    const score = fails ? 0 : round(100 * mean * sizeFactor, 2);
    return {
        section: name,
        step: Boolean(step),
        score,
        geometry: geo,
        pixels: { score: round(pixelScore, 4), ink: pixels.ink, mismatched: pixels.mismatched, box, ...(step ? { change: round(change, 4), canvasChanged: pixels.canvasChanged, domChanged: pixels.domChanged, bothChanged: pixels.bothChanged } : {}) },
        size: { canvas: [canvas.width, canvas.height], dom: [round(dom.width), round(dom.height)], heightDelta },
        ...(notes.length ? { note: notes.join("; ") } : {}),
    };
}

async function runSteps(spec, pages, only, threshold, folder, canvasSet, reload) {
    const results = [];
    const widths = Object.fromEntries(canvasSet.map((each) => [each.section, each.width]));
    let dirty = false;
    for (const step of spec.steps ?? []) {
        const section = step.section ?? canvasSet[0]?.section;
        if (only && section !== only) continue;
        if (!canvasSet.some((each) => each.section === section)) continue;
        const target = stepTarget(step);
        const type = step.hover ? "hover" : step.press ? "press" : step.click ? "click" : "key";
        if (dirty && !(step.keep ?? type === "key")) {
            await reload();
            dirty = false;
        }
        const name = `${section}@${step.name ?? (target ? describeTarget(target) : step.key)}`.replace(/[\\/]/g, "_");
        const act = async (page, point) => {
            if (type === "key") return page.key(step.key);
            if (!point) return;
            await page.mouse("mouseMoved", point.x, point.y);
            if (type === "press" || type === "click") await page.mouse("mousePressed", point.x, point.y);
            if (type === "click") await page.mouse("mouseReleased", point.x, point.y);
        };
        const canvasBefore = (await canvasSections(pages.canvas, section))[0];
        const domBefore = (await domSections(pages.dom, widths, section))[0];
        const domHit = target ? await domPoint(pages.dom, section, target) : null;
        const canvasTarget = target?.selector ? domHit && { at: domHit.at } : target;
        const canvasHit = canvasTarget ? await canvasPoint(pages.canvas, section, canvasTarget) : null;
        await act(pages.canvas, canvasHit);
        await sleep(step.wait ?? 250);
        const canvasAfter = (await canvasSections(pages.canvas, section))[0];
        await act(pages.dom, domHit);
        await sleep(step.wait ?? 250);
        const domAfter = (await domSections(pages.dom, widths, section))[0];
        const result = await compare(pages.lab, canvasAfter, domAfter, threshold, folder, name, { target, canvasBefore: canvasBefore.png, domBefore: domBefore?.png ?? null });
        if (target && !domHit) result.note = `${result.note ? `${result.note}; ` : ""}dom has no ${target.selector ?? `[data-id=${describeTarget(target)}]`}`;
        results.push(result);
        for (const [page, point] of [[pages.canvas, canvasHit], [pages.dom, domHit]]) {
            if (type === "press" && point) await page.mouse("mouseReleased", point.x, point.y);
            await page.mouse("mouseMoved", 1, 1);
        }
        await sleep(100);
        if (!dirty) dirty = (await canvasSections(pages.canvas, section))[0].png !== canvasBefore.png;
    }
    return { results, dirty };
}

function mean(values) {
    return values.length ? round(values.reduce((sum, value) => sum + value, 0) / values.length, 2) : null;
}

async function loadCanvas(pages, bases, story, theme, mode, loads) {
    if (pages.canvasState === "clean") {
        await pages.canvas.media(mode);
        await pages.canvas.call(async (story, theme, mode) => {
            await Promise.race([storybook.setKnobs({ "interface-theme": theme, theme: mode, motion: "0" }), new Promise((r) => setTimeout(r, 200))]);
            if (storybook.current() !== story) await Promise.race([storybook.show(story), new Promise((r) => setTimeout(r, 200))]);
            scrollTo(0, 0);
        }, story, theme, mode);
        await canvasReady(pages.canvas, story);
        return;
    }
    const started = Date.now();
    const knobs = new URLSearchParams({ solo: "1", theme: mode, "interface-theme": theme, motion: "0" });
    await pages.canvas.navigate(`${bases.story}/?load=${++pages.canvasLoads}#/${story}?${knobs}`, mode);
    await canvasReady(pages.canvas, story);
    pages.canvasState = "clean";
    loads.count++;
    loads.ms += Date.now() - started;
}

async function loadDom(pages, url, mode) {
    await pages.dom.navigate(`${url}&load=${++pages.domLoads}`, mode);
    await domReady(pages.dom);
}

async function runComponent(name, options, pages, bases, combos) {
    const specPath = join(ROOT, "storybook/src/kit", name, "parity.json");
    if (!existsSync(specPath)) throw new Error(`${relative(ROOT, specPath)} is missing`);
    const spec = JSON.parse(readFileSync(specPath, "utf8"));
    const report = { component: name, story: spec.story, page: spec.page ?? "index.html", threshold: options.threshold, combos: {}, loads: { count: 0, ms: 0 } };
    pages.canvasState = "stale";
    for (const [theme, mode] of combos) {
        const combo = `${theme}-${mode}`;
        const folder = join(options.out, name, combo);
        rmSync(folder, { recursive: true, force: true });
        await loadCanvas(pages, bases, spec.story, theme, mode, report.loads);
        const canvasSet = await canvasSections(pages.canvas, options.section);
        if (!canvasSet.length) throw new Error(`story ${spec.story} has no section${options.section ? ` ${options.section}` : "s"}`);
        const domUrl = `${bases.dom}/storybook/src/kit/${name}/${report.page}?theme=${theme}&mode=${mode}&bare&flat&motion=0`;
        await loadDom(pages, domUrl, mode);
        const widths = Object.fromEntries(canvasSet.map((each) => [each.section, each.width]));
        const domSet = await domSections(pages.dom, widths, options.section);
        const sections = [];
        for (const canvas of canvasSet) {
            sections.push(await compare(pages.lab, canvas, domSet.find((each) => each.section === canvas.section), options.threshold, folder, canvas.section));
        }
        const reload = async () => {
            pages.canvasState = "stale";
            await loadCanvas(pages, bases, spec.story, theme, mode, report.loads);
            await loadDom(pages, domUrl, mode);
        };
        const steps = await runSteps(spec, pages, options.section, options.threshold, folder, canvasSet, reload);
        if (steps.dirty) pages.canvasState = "stale";
        sections.push(...steps.results);
        const unknown = domSet.map((each) => each.section).filter((section) => !canvasSet.some((each) => each.section === section));
        report.combos[combo] = { sections, unknownSections: unknown };
    }
    const all = Object.values(report.combos).flatMap((combo) => combo.sections);
    report.score = mean(all.filter((section) => !section.step).map((section) => section.score)) ?? 0;
    report.steps = mean(all.filter((section) => section.step).map((section) => section.score));
    mkdirSync(join(options.out, name), { recursive: true });
    writeAtomic(join(options.out, name, "report.json"), JSON.stringify(report, null, 2));
    return report;
}

function writeAtomic(path, text) {
    const temp = `${path}.${process.pid}.tmp`;
    writeFileSync(temp, text);
    renameSync(temp, path);
}

function scores(report) {
    const flat = {};
    for (const [combo, { sections }] of Object.entries(report.combos)) {
        for (const section of sections) flat[`${combo}/${section.section}`] = section.score;
    }
    return flat;
}

function baseline(report, options) {
    const path = join(ROOT, "storybook/src/kit", report.component, "parity.baseline.json");
    const current = scores(report);
    const saved = existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : null;
    const failures = [];
    if (options.check) {
        if (!saved && !options.update) failures.push(`${report.component}: no ${relative(ROOT, path)}; run with --update`);
        for (const [key, value] of Object.entries(current)) {
            if (saved && key in saved) {
                if (value < saved[key] - 0.5) failures.push(`${report.component} ${key}: ${value} < baseline ${saved[key]}`);
            } else if (saved && !options.update) {
                failures.push(`${report.component} ${key}: ${value} has no baseline`);
            }
        }
    }
    if (options.update) {
        if (failures.length) {
            console.error(`not updating ${relative(ROOT, path)}: --check failed`);
        } else {
            const merged = Object.fromEntries(Object.entries({ ...(saved ?? {}), ...current }).sort(([a], [b]) => a.localeCompare(b)));
            writeAtomic(path, `${JSON.stringify(merged, null, 2)}\n`);
        }
    }
    return failures;
}

function table(report) {
    const lines = [];
    const pad = (value, width) => String(value).padEnd(width);
    lines.push(`${report.component}  sections ${report.score}${report.steps === null ? "" : `  steps ${report.steps}`}`);
    lines.push(`  ${pad("combo", 14)}${pad("section", 28)}${pad("score", 8)}${pad("geo", 18)}${pad("px", 9)}${pad("dh", 6)}note`);
    const row = (combo, section) => {
        const geo = section.geometry;
        const bad = geo.matched.filter((node) => !node.ok);
        const worst = geo.matched.reduce((max, node) => Math.max(max, node.offset), 0);
        const geoText = geo.score === null ? "-" : `${round(geo.score * 100)}% ${geo.matched.length - bad.length}/${geo.matched.length + geo.missing.length}${geo.extra.length ? `+${geo.extra.length}` : ""} ${round(worst)}px`;
        const pxText = section.pixels ? `${round(section.pixels.score * 100)}%${section.pixels.change === undefined ? "" : ` c${round(section.pixels.change * 100)}%`}` : "-";
        const notes = [];
        if (section.missingSection) notes.push("no DOM section");
        if (geo.missing.length) notes.push(`missing ${geo.missing.slice(0, 4).join(",")}${geo.missing.length > 4 ? ",..." : ""}`);
        if (bad.length) notes.push(`off ${bad.slice(0, 3).map((node) => `${node.id}(${node.dx},${node.dy},${node.dw},${node.dh})`).join(" ")}`);
        if (geo.extra.length) notes.push(`extra ${geo.extra.slice(0, 3).join(",")}`);
        if (section.note) notes.push(section.note);
        return `  ${pad(combo, 14)}${pad(section.section, 28)}${pad(section.score, 8)}${pad(geoText, 18)}${pad(pxText, section.step ? 15 : 9)}${pad(section.size?.heightDelta ?? "-", 6)}${notes.join("; ")}`;
    };
    for (const [combo, { sections, unknownSections }] of Object.entries(report.combos)) {
        for (const section of sections.filter((each) => !each.step)) lines.push(row(combo, section));
        if (unknownSections.length) lines.push(`  ${pad(combo, 14)}DOM sections the story lacks: ${unknownSections.join(", ")}`);
    }
    const steps = Object.entries(report.combos).flatMap(([combo, { sections }]) => sections.filter((each) => each.step).map((section) => [combo, section]));
    if (steps.length) {
        lines.push(`  steps (target box only; c = DOM changed where the canvas did)`);
        for (const [combo, section] of steps) lines.push(row(combo, section));
    }
    return lines.join("\n");
}

function html(out) {
    const entries = [];
    for (const name of existsSync(out) ? readdirSync(out).sort() : []) {
        const path = join(out, name, "report.json");
        if (!existsSync(path)) continue;
        const report = JSON.parse(readFileSync(path, "utf8"));
        for (const [combo, { sections }] of Object.entries(report.combos)) {
            for (const section of sections) {
                const base = `${name}/${combo}/${section.section}`;
                entries.push({
                    component: name,
                    combo,
                    section: section.section,
                    score: section.score,
                    canvas: `${base}.canvas.png`,
                    dom: section.missingSection ? null : `${base}.dom.png`,
                    diff: section.missingSection ? null : `${base}.diff.png`,
                    detail: section,
                });
            }
        }
    }
    entries.sort((a, b) => a.score - b.score);
    const page = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Penna parity</title>
<style>
:root { color-scheme: light dark; --bg: #f7f7f7; --fg: #1d1d1f; --muted: #6b6b6f; --line: #dcdcdc; --card: #fff; --bad: #d22f3c; --ok: #1f8a4c; }
@media (prefers-color-scheme: dark) { :root { --bg: #111; --fg: #ececec; --muted: #9a9a9e; --line: #2a2a2a; --card: #1a1a1a; } }
* { box-sizing: border-box; }
body { margin: 0; padding: 24px 16px 80px; background: var(--bg); color: var(--fg); font: 13px/1.5 system-ui, sans-serif; }
h1 { font-size: 18px; margin: 0 0 4px; }
header p { margin: 0 0 20px; color: var(--muted); }
.controls { display: flex; gap: 16px; align-items: center; flex-wrap: wrap; margin-bottom: 20px; }
article { background: var(--card); border: 1px solid var(--line); border-radius: 10px; padding: 14px; margin: 0 0 16px; }
article h2 { display: flex; gap: 10px; align-items: baseline; margin: 0 0 10px; font-size: 14px; font-weight: 600; }
.score { font-variant-numeric: tabular-nums; min-width: 52px; }
.meta { color: var(--muted); font-weight: 400; font-size: 12px; }
.pair { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
.pair figure, .overlay { margin: 0; }
figcaption { color: var(--muted); font-size: 11px; margin-bottom: 4px; }
img { display: block; max-width: 100%; image-rendering: auto; border: 1px solid var(--line); }
.overlay { position: relative; margin-top: 10px; }
.overlay img.top { position: absolute; inset: 0; clip-path: inset(0 calc(100% - var(--cut, 50%)) 0 0); }
.overlay input { width: 100%; margin: 6px 0 0; }
.issues { margin: 8px 0 0; color: var(--muted); font-size: 12px; white-space: pre-wrap; }
@media (max-width: 760px) { .pair { grid-template-columns: 1fr; } }
</style>
</head>
<body>
<header><h1>Penna parity</h1><p>Canvas (penna drawn by ala) against the hand-written DOM page, worst first. Score = 100 × √(geometry × pixels) × height factor; steps also multiply in how well the DOM changed where the canvas did, inside the target's box.</p></header>
<div class="controls"><label><input type="checkbox" id="diff"> Show diff instead of DOM in the overlay</label></div>
<main id="list"></main>
<script>
const entries = ${JSON.stringify(entries).replace(/</g, "\\u003c")};
const list = document.getElementById("list");
const toggle = document.getElementById("diff");
function issues(detail) {
  const lines = [];
  if (detail.missingSection) lines.push("no DOM section");
  const geo = detail.geometry;
  if (geo.missing.length) lines.push("missing: " + geo.missing.join(", "));
  if (geo.extra.length) lines.push("extra: " + geo.extra.join(", "));
  for (const node of geo.matched.filter((node) => !node.ok)) lines.push(node.id + " off by dx " + node.dx + " dy " + node.dy + " dw " + node.dw + " dh " + node.dh);
  if (detail.size && detail.size.dom) lines.push("size canvas " + detail.size.canvas.join("×") + " dom " + detail.size.dom.join("×"));
  if (detail.note) lines.push(detail.note);
  return lines.join("\\n");
}
for (const entry of entries) {
  const article = document.createElement("article");
  const color = entry.score >= 90 ? "var(--ok)" : entry.score < 60 ? "var(--bad)" : "inherit";
  const pixels = entry.detail.pixels ? " · pixels " + Math.round(entry.detail.pixels.score * 1000) / 10 + "%" : "";
  const change = entry.detail.pixels && entry.detail.pixels.change !== undefined ? " · change " + Math.round(entry.detail.pixels.change * 1000) / 10 + "%" : "";
  const geo = entry.detail.geometry;
  const geoText = geo.score === null || geo.score === undefined ? "no nodes" : Math.round(geo.score * 1000) / 10 + "%";
  article.innerHTML =
    '<h2><span class="score" style="color:' + color + '">' + entry.score + '</span>' + entry.component + " / " + entry.section +
    '<span class="meta">' + entry.combo + " · geometry " + geoText + pixels + change + "</span></h2>" +
    '<div class="pair"><figure><figcaption>canvas</figcaption><img src="' + entry.canvas + '"></figure>' +
    '<figure><figcaption>dom</figcaption>' + (entry.dom ? '<img src="' + entry.dom + '">' : "<p>no DOM section</p>") + "</figure></div>" +
    (entry.dom ? '<div class="overlay"><img class="under" src="' + entry.canvas + '"><img class="top" data-dom="' + entry.dom + '" data-diff="' + entry.diff + '" src="' + entry.dom + '"><input type="range" min="0" max="100" value="50" aria-label="Overlay split"></div>' : "") +
    '<div class="issues"></div>';
  article.querySelector(".issues").textContent = issues(entry.detail);
  const range = article.querySelector("input[type=range]");
  if (range) range.addEventListener("input", () => article.querySelector(".overlay").style.setProperty("--cut", range.value + "%"));
  list.append(article);
}
toggle.addEventListener("change", () => {
  for (const image of document.querySelectorAll("img.top")) image.src = toggle.checked ? image.dataset.diff : image.dataset.dom;
});
</script>
</body>
</html>
`;
    mkdirSync(out, { recursive: true });
    writeAtomic(join(out, "report.html"), page);
}

async function main() {
    const options = parseArgs(process.argv.slice(2));
    let deadline;
    const arm = (what) => {
        clearTimeout(deadline);
        deadline = setTimeout(() => {
            console.error(`${what} timed out after ${options.timeout} ms`);
            process.exit(1);
        }, options.timeout);
        deadline.unref();
    };
    const themes = options.theme ? [options.theme] : THEMES;
    const modes = options.mode ? [options.mode] : MODES;
    const combos = themes.flatMap((theme) => modes.map((mode) => [theme, mode]));
    prepare();
    arm("starting the browser");
    const bases = { story: await serve(join(ROOT, "storybook/dist"), () => true, "max-age=3600"), dom: await serve(ROOT, domFiles()) };
    const cdp = connect(launch());
    const pages = { canvas: await tab(cdp), dom: await tab(cdp), lab: await tab(cdp), canvasState: "stale", canvasLoads: 0, domLoads: 0 };
    const failures = [];
    for (const name of options.components) {
        arm(name);
        const started = Date.now();
        const report = await runComponent(name, options, pages, bases, combos);
        console.log(table(report));
        console.log(`  (${((Date.now() - started) / 1000).toFixed(1)}s, ${report.loads.count} story loads ${(report.loads.ms / 1000).toFixed(1)}s)\n`);
        failures.push(...baseline(report, options));
    }
    html(options.out);
    console.log(`report: ${join(options.out, "report.html")}`);
    if (failures.length) {
        console.error(`\ncheck failed:\n${failures.join("\n")}`);
        process.exitCode = 1;
    }
}

main()
    .catch((error) => {
        console.error(error.message ?? error);
        process.exitCode = 1;
    })
    .finally(() => {
        cleanup();
        process.exit();
    });
