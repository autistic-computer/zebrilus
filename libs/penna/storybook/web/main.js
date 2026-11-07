import init, * as wasm from "./wasm/storybook.js";

const CREATOR = "theme-creator";
const GLASS_KNOBS = [
    ["glass-blur", "Blur", 0, 40, 0.05],
    ["glass-tint", "Surface tint", 0, 1, 0.01],
    ["glass-refraction", "Refraction", -60, 150, 1],
    ["glass-bezel", "Bezel", 0, 60, 1],
    ["glass-dispersion", "Dispersion", 0, 0.6, 0.01],
    ["glass-saturation", "Saturation", 0.5, 2.5, 0.05],
    ["glass-brightness", "Brightness", -0.3, 0.3, 0.01],
    ["glass-contrast", "Contrast", 0.5, 2, 0.05],
    ["glass-specular", "Glint", 0, 1.5, 0.05],
    ["glass-glint-width", "Glint width", 0, 6, 0.1],
    ["glass-light", "Light angle", -180, 180, 5],
    ["glass-light-directionality", "Directional light", 0, 1, 0.01],
    ["glass-fresnel", "Rim glow", 0, 0.5, 0.01],
    ["glass-edge", "Edge shadow", 0, 1, 0.05],
    ["glass-edge-width", "Edge shadow width", 0, 4, 0.05],
    ["glass-edge-spread", "Edge shadow spread", 0, 1, 0.05],
    ["glass-noise", "Grain", 0, 0.1, 0.005],
    ["glass-inactive-flattening", "Inactive flattening", 0, 1, 0.01],
];
const GLASS_NAMES = GLASS_KNOBS.map(([name]) => name);
const PLAIN_THEME_KNOBS = ["name", "neutral-hue", "neutral-amount", "radius", "smoothing", "glass", "face", "lock"];
const TINT_KNOBS = ["tint-color", "light-tint-color", "dark-tint-color"];
const PERSON_KNOBS = ["theme", "accent", "contrast", "reduce-transparency", "zoom", "motion", "backdrop"];
const STEPS = ["look", "person", "export", "app"];
const HASH_LIMIT = 2000;
const KNOBS_MIN_WIDTH = 280;
const DEFAULTS = {
    "interface-theme": "default",
    theme: "system",
    accent: "",
    contrast: "1",
    "reduce-transparency": "0",
    zoom: "1",
    motion: "default",
    backdrop: "plain",
};

const nav = document.getElementById("nav");
const storyRoot = document.getElementById("story");
const regions = document.getElementById("regions");
const form = document.getElementById("knob-form");
const importDialog = document.getElementById("import-dialog");

let book = { title: "Storybook", name: "", storage: "storybook", knobs: [] };
let stories = [];
let glassPresets = {};
let themes = {};
let accents = [];
let appKnobs = [];
let current = null;
let step = STEPS[0];
let inspected = null;
let focusRetries = 0;
let solo = false;
const timeline = { past: [], future: [], last: null };

function parseHash() {
    const [path, query = ""] = location.hash.replace(/^#\/?/, "").split("?");
    const [story, section] = path.split("/");
    return { story, section, query, params: new URLSearchParams(query) };
}

function field(name) {
    return form.elements[name];
}

function knobValue(name) {
    const input = field(name);
    return input.type === "checkbox" ? (input.checked ? "1" : "0") : input.value;
}

function setKnobValue(name, value) {
    const input = field(name);
    if (!input || value === undefined || value === null) return;
    if (input.type === "checkbox") input.checked = value !== "0" && value !== "false";
    else input.value = String(value);
}

function same(name, value, other) {
    const input = field(name);
    if (input?.type !== "range") return String(value) === String(other);
    const step = Number(input.step) || 0.01;
    return Math.abs(Number(value) - Number(other)) < step / 2;
}

function preset() {
    return themes[knobValue("interface-theme")] ?? themes[DEFAULTS["interface-theme"]];
}

function accentNamed(value) {
    return accents.find((accent) => accent.name === value);
}

function tintOf(knobs) {
    let light = null;
    let dark = null;
    const named = knobs["tint-color"];
    if (named) {
        const accent = accentNamed(named);
        light = accent?.light ?? named;
        dark = accent?.dark ?? named;
    }
    light = knobs["light-tint-color"] ?? light ?? knobs["dark-tint-color"] ?? null;
    dark = knobs["dark-tint-color"] ?? dark ?? light;
    if (light === null) return { choice: accents[0]?.name ?? "custom", light: accents[0]?.light ?? null, dark: accents[0]?.dark ?? null };
    const lower = (color) => color.toLowerCase();
    const accent = accents.find((each) => lower(each.light) === lower(light) && lower(each.dark) === lower(dark));
    return { choice: accent?.name ?? "custom", light, dark };
}

function shownTint() {
    const choice = knobValue("tint-color");
    return { choice, light: knobValue("light-tint-color"), dark: knobValue("dark-tint-color") };
}

function setTint(tint) {
    setKnobValue("tint-color", tint.choice);
    if (tint.light) setKnobValue("light-tint-color", tint.light);
    if (tint.dark) setKnobValue("dark-tint-color", tint.dark);
}

function cornersOf(controlRadius, radius) {
    if (controlRadius === "full") return { corners: "full", value: radius };
    if (controlRadius === undefined || Number(controlRadius) === Number(radius)) return { corners: "same", value: radius };
    return { corners: "own", value: controlRadius };
}

function setCorners({ corners, value }) {
    setKnobValue("control-corners", corners);
    setKnobValue("control-radius", value);
}

function controlRadius() {
    return { same: knobValue("radius"), own: knobValue("control-radius"), full: "full" }[knobValue("control-corners")];
}

function glassDefaults(glass = knobValue("glass")) {
    return glassPresets[glass] ?? null;
}

function resetGlassSliders() {
    const material = glassDefaults();
    if (!material) return;
    for (const name of GLASS_NAMES) setKnobValue(name, material[name]);
}

function themeParams() {
    const params = new URLSearchParams();
    for (const name of [...PERSON_KNOBS, ...appKnobs]) {
        if (!same(name, knobValue(name), DEFAULTS[name])) params.set(name, knobValue(name));
    }
    const base = preset();
    if (knobValue("interface-theme") !== DEFAULTS["interface-theme"]) params.set("interface-theme", knobValue("interface-theme"));
    for (const name of PLAIN_THEME_KNOBS) {
        if (!same(name, knobValue(name), base[name] ?? "")) params.set(name, knobValue(name));
    }
    const tint = shownTint();
    const baseTint = tintOf(base);
    const moved = tint.choice !== baseTint.choice || (tint.choice === "custom" && (tint.light !== baseTint.light || tint.dark !== baseTint.dark));
    if (moved && tint.choice === "custom") {
        params.set("light-tint-color", tint.light);
        params.set("dark-tint-color", tint.dark);
    } else if (moved) {
        params.set("tint-color", tint.choice);
    }
    const baseCorners = cornersOf(base["control-radius"], base.radius);
    const corners = knobValue("control-corners");
    if (corners !== baseCorners.corners || (corners === "own" && !same("control-radius", knobValue("control-radius"), baseCorners.value))) {
        params.set("control-radius", controlRadius());
    }
    const material = glassDefaults();
    if (material) {
        for (const name of GLASS_NAMES) {
            if (!same(name, knobValue(name), material[name])) params.set(name, knobValue(name));
        }
    }
    return params;
}

function knobQuery() {
    return themeParams().toString();
}

function hashQuery() {
    const params = themeParams();
    if (params.toString().length > HASH_LIMIT) for (const name of GLASS_NAMES) params.delete(name);
    if (step !== STEPS[0]) params.set("step", step);
    if (inspected) params.set("inspect", inspected);
    if (solo) params.set("solo", "1");
    return params.toString();
}

function loadKnobs(params) {
    const id = themes[params.get("interface-theme")] ? params.get("interface-theme") : DEFAULTS["interface-theme"];
    setKnobValue("interface-theme", id);
    const knobs = { ...themes[id] };
    if (params.has("tint-color")) for (const name of TINT_KNOBS) delete knobs[name];
    for (const [name, value] of params) knobs[name] = value;
    if (knobs.face) addFontOption(knobs.face);
    for (const name of [...PERSON_KNOBS, ...appKnobs]) setKnobValue(name, knobs[name] ?? DEFAULTS[name]);
    for (const name of PLAIN_THEME_KNOBS) setKnobValue(name, knobs[name] ?? "");
    setTint(tintOf(knobs));
    setCorners(cornersOf(knobs["control-radius"], knobs.radius));
    resetGlassSliders();
    for (const name of GLASS_NAMES) if (params.has(name)) setKnobValue(name, params.get(name));
}

function personParams() {
    const params = themeParams();
    for (const name of [...params.keys()]) {
        if (!PERSON_KNOBS.includes(name) && !appKnobs.includes(name)) params.delete(name);
    }
    return params;
}

function resetThemeKnobs() {
    const params = personParams();
    params.set("interface-theme", knobValue("interface-theme"));
    loadKnobs(params);
}

function writeHash(story, section) {
    const query = hashQuery();
    const path = section ? `${story}/${section}` : story;
    const hash = `#/${path}${query ? `?${query}` : ""}`;
    if (location.hash !== hash) history.replaceState(null, "", hash);
}

function element(tag, attributes = {}, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attributes)) {
        if (key === "text") node.textContent = value;
        else node.setAttribute(key, value);
    }
    node.append(...children);
    return node;
}

function renderNav() {
    nav.replaceChildren(element("h1", { text: book.title }));
    const groups = new Map();
    for (const story of stories) {
        if (!groups.has(story.group)) groups.set(story.group, []);
        groups.get(story.group).push(story);
    }
    for (const [group, items] of groups) {
        const list = element("ul");
        for (const story of items) {
            const sections = element("ul", { class: "sections" });
            if (story.id !== CREATOR) {
                for (const section of story.sections) {
                    sections.append(
                        element("li", {}, element("a", { href: `#/${story.id}/${section.id}`, text: section.name })),
                    );
                }
            }
            list.append(
                element(
                    "li",
                    { "data-story": story.id },
                    element("a", { href: `#/${story.id}`, text: story.name }),
                    sections,
                ),
            );
        }
        nav.append(element("h2", { text: group }), list);
    }
}

function markCurrent(storyId) {
    for (const item of nav.querySelectorAll("li[data-story]")) {
        const active = item.dataset.story === storyId;
        item.classList.toggle("current", active);
        item.querySelector("a").toggleAttribute("aria-current", active);
    }
}

function show(storyId, sectionId) {
    const story = stories.find((candidate) => candidate.id === storyId) ?? stories[0];
    if (!story) return;
    const creator = story.id === CREATOR;
    if (current !== story.id) {
        document.body.classList.toggle("creator", creator);
        if (creator) {
            storyRoot.replaceChildren(
                element("h1", { class: "visually-hidden", text: story.name }),
                element(
                    "div",
                    { id: "focus-note", role: "status", hidden: "" },
                    element("span"),
                    element("button", { type: "button", "data-action": "show-all", text: "Show all" }),
                ),
                ...story.sections.map((section) =>
                    element(
                        "section",
                        {
                            id: `${story.id}-${section.id}`,
                            class: "region",
                            "data-region": section.id,
                            "aria-labelledby": `${story.id}-${section.id}-title`,
                        },
                        element("h2", { id: `${story.id}-${section.id}-title`, text: section.name }),
                        element("div", { id: section.mount, class: "mount canvas", "data-section": section.id }),
                    ),
                ),
            );
            regions.replaceChildren(
                ...story.sections.map((section) =>
                    element("a", {
                        href: `#/${story.id}/${section.id}`,
                        title: section.summary,
                        text: section.name,
                        "data-region": section.id,
                    }),
                ),
            );
        } else {
            const sections = story.sections.map((section) =>
                element(
                    "section",
                    { id: `${story.id}-${section.id}`, "aria-labelledby": `${story.id}-${section.id}-title` },
                    element("h2", { id: `${story.id}-${section.id}-title`, text: section.name }),
                    element("p", { text: section.summary }),
                    element("div", { id: section.mount, class: "mount", "data-section": section.id }),
                ),
            );
            storyRoot.replaceChildren(
                element("header", {}, element("h1", { text: story.name }), element("p", { text: story.summary })),
                ...sections,
            );
        }
        wasm.show(story.id);
        current = story.id;
        markCurrent(story.id);
        focusRetries = 0;
        focusShowcase();
        flash();
        document.title = creator ? `Theme creator · ${book.name}` : `${story.name} · ${book.title}`;
    }
    if (sectionId) {
        document.getElementById(`${story.id}-${sectionId}`)?.scrollIntoView({ block: "start" });
    } else {
        storyRoot.scrollTo?.(0, 0);
    }
    writeHash(story.id, sectionId);
}

function paintSliders() {
    for (const input of form.querySelectorAll("input[type=range]")) {
        const min = Number(input.min);
        const max = Number(input.max);
        const fill = ((Number(input.value) - min) / (max - min)) * 100;
        input.parentElement.style.setProperty("--fill", `${Math.max(0, Math.min(100, fill))}%`);
    }
}

function applyKnobs() {
    form.querySelector("#control-radius").hidden = knobValue("control-corners") !== "own";
    form.querySelector("#glass-material").hidden = glassDefaults() === null;
    for (const output of form.querySelectorAll("output[name$='-value']")) {
        output.value = knobValue(output.name.slice(0, -"-value".length));
    }
    paintSliders();
    wasm.set_knobs(knobQuery());
    paintChrome();
    paintStep();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function paintStep() {
    for (const tab of form.querySelectorAll("[role=tab]")) {
        const active = tab.dataset.step === step;
        tab.setAttribute("aria-selected", String(active));
        tab.tabIndex = active ? 0 : -1;
    }
    for (const panel of form.querySelectorAll(".step")) panel.hidden = panel.dataset.step !== step;
    if (step === "export") document.getElementById("theme-json").textContent = wasm.theme_file();
}

function showStep(name) {
    if (!STEPS.includes(name) || name === step) return;
    step = name;
    paintStep();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function flash() {
    wasm.inspect(current === CREATOR ? (inspected ?? "") : "");
}

function focusShowcase() {
    if (current !== CREATOR) return;
    const id = inspected;
    const drawn = id ? JSON.parse(wasm.drawn_roles()) : {};
    const sections = [...storyRoot.querySelectorAll("section.region")];
    const found = sections.filter((section) => drawn[section.dataset.region]?.includes(id));
    const filtering = Boolean(id) && found.length > 0;
    for (const section of sections) {
        const names = drawn[section.dataset.region];
        section.hidden = filtering && Boolean(names) && !names.includes(id);
    }
    for (const link of regions.children) {
        link.hidden = filtering && !found.some((section) => section.dataset.region === link.dataset.region);
    }
    const note = document.getElementById("focus-note");
    note.hidden = !id;
    storyRoot.style.setProperty("--focus-note", note.hidden ? "0px" : `${note.offsetHeight}px`);
    if (id) {
        const names = found.map((section) => section.querySelector("h2").textContent);
        note.querySelector("span").textContent = `${id} · ${found.length ? `in ${names.join(", ")}` : "not in the showcase"}`;
    }
    const pending = id && sections.some((section) => !drawn[section.dataset.region]);
    if (pending && focusRetries < 25) {
        focusRetries += 1;
        setTimeout(focusShowcase, 200);
    }
}

function inspect(id) {
    inspected = id || null;
    focusRetries = 0;
    focusShowcase();
    storyRoot.scrollTo?.(0, 0);
    flash();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function remember() {
    const query = themeParams().toString();
    if (query === timeline.last) return;
    if (timeline.last !== null) timeline.past.push(timeline.last);
    timeline.past = timeline.past.slice(-100);
    timeline.future = [];
    timeline.last = query;
    paintHistory();
}

function travel(from, to) {
    if (from.length === 0) return;
    to.push(timeline.last);
    timeline.last = from.pop();
    loadKnobs(new URLSearchParams(timeline.last));
    applyKnobs();
    paintHistory();
}

function paintHistory() {
    document.querySelector("[data-action=undo]").disabled = timeline.past.length === 0;
    document.querySelector("[data-action=redo]").disabled = timeline.future.length === 0;
}

function paintChrome() {
    const theme = JSON.parse(wasm.theme());
    const root = document.documentElement.style;
    for (const key of ["background", "raised", "raised2", "raised3", "foreground", "muted", "border", "accent"]) {
        root.setProperty(`--penna-${key}`, theme[key]);
    }
    root.setProperty("--penna-radius", `${Math.min(theme.radius, 14)}px`);
    document.documentElement.dataset.mode = theme.mode;
}

function loadView(params) {
    solo = params.has("solo") && params.get("solo") !== "0";
    document.body.classList.toggle("solo", solo);
    if (STEPS.includes(params.get("step"))) step = params.get("step");
    inspected = params.get("inspect") || null;
}

function title(name) {
    return name.charAt(0).toUpperCase() + name.slice(1);
}

function renderKnobs() {
    const choices = document.getElementById("theme-choices");
    for (const [id, theme] of Object.entries(themes)) {
        choices.append(
            element(
                "label",
                {},
                element("input", { type: "radio", name: "interface-theme", value: id }),
                element("span", { text: theme.name }),
            ),
        );
    }
    document.querySelector(".start").hidden = Object.keys(themes).length < 2;
    const custom = field("tint-color").querySelector("option[value=custom]");
    for (const accent of accents) {
        custom.before(element("option", { value: accent.name, text: title(accent.name) }));
        document.getElementById("accent-choices").append(element("option", { value: accent.name, text: title(accent.name) }));
    }
    setTint({ choice: accents[0]?.name ?? "custom", light: accents[0]?.light, dark: accents[0]?.dark });
    document.getElementById("glass-knobs").append(
        ...GLASS_KNOBS.map(([name, label, min, max, step]) =>
            element(
                "label",
                { class: "row slider" },
                element("span", { text: label }),
                element("output", { name: `${name}-value` }),
                element("input", { type: "range", name, min, max, step }),
            ),
        ),
    );
    const app = document.getElementById("step-app");
    const preview = document.getElementById("preview-knobs");
    for (const knob of book.knobs) {
        let section = [...app.querySelectorAll("section")].find((each) => each.querySelector("h3")?.textContent === knob.section);
        if (!section) {
            section = element("section", {}, element("h3", { text: knob.section }));
            preview.before(section);
        }
        section.append(
            knob.kind === "check"
                ? element(
                      "label",
                      { class: "row check" },
                      element("span", { text: knob.label }),
                      element("input", { type: "checkbox", name: knob.name }),
                  )
                : element(
                      "label",
                      { class: "row slider" },
                      element("span", { text: knob.label }),
                      element("output", { name: `${knob.name}-value` }),
                      element("input", { type: "range", name: knob.name, min: knob.min, max: knob.max, step: knob.step, value: knob.value }),
                  ),
        );
        appKnobs.push(knob.name);
        DEFAULTS[knob.name] = knob.kind === "check" ? (knob.value ? "1" : "0") : String(knob.value);
    }
}

function addFontOption(name) {
    const select = field("face");
    if (![...select.options].some((option) => option.value === name)) {
        select.append(element("option", { value: name, text: name }));
    }
}

function toast(message) {
    const node = document.getElementById("toast");
    node.textContent = message;
    node.hidden = false;
    clearTimeout(toast.timer);
    toast.timer = setTimeout(() => (node.hidden = true), 1800);
}

function shuffle() {
    const pick = (name) => {
        const input = field(name);
        const min = Number(input.min);
        const max = Number(input.max);
        const step = Number(input.step) || 0.01;
        const value = min + Math.round((Math.random() * (max - min)) / step) * step;
        setKnobValue(name, String(Number(value.toFixed(4))));
    };
    const any = (list) => list[Math.floor(Math.random() * list.length)];
    for (const name of ["neutral-hue", "radius", "control-radius", "smoothing"]) pick(name);
    setKnobValue("neutral-amount", String(Math.round(Math.random() * 40) / 100));
    setKnobValue("control-corners", any(["same", "own", "full"]));
    const tint = any(accents.map((accent) => accent.name));
    setTint(tintOf({ "tint-color": tint }));
    setKnobValue("glass", any(["none", "liquid"]));
    resetGlassSliders();
}

function importTheme(json) {
    const params = personParams();
    params.set("interface-theme", knobValue("interface-theme"));
    for (const [name, value] of Object.entries(JSON.parse(wasm.import_theme(json)))) params.set(name, String(value));
    loadKnobs(params);
}

function setKnobs(knobs) {
    const params = themeParams();
    if ("interface-theme" in knobs) {
        for (const name of [...params.keys()]) if (!PERSON_KNOBS.includes(name) && !appKnobs.includes(name)) params.delete(name);
    }
    if ("glass" in knobs) for (const name of GLASS_NAMES) params.delete(name);
    if ("tint-color" in knobs) for (const name of TINT_KNOBS) params.delete(name);
    for (const [name, value] of Object.entries(knobs)) params.set(name, String(value));
    loadKnobs(params);
}

async function copyTheme() {
    const json = wasm.theme_file();
    try {
        await navigator.clipboard.writeText(json);
        toast("Theme copied as JSON");
    } catch {
        document.getElementById("import-json").value = json;
        importDialog.showModal();
    }
}

const actions = {
    undo: () => travel(timeline.past, timeline.future),
    redo: () => travel(timeline.future, timeline.past),
    reset: () => {
        resetThemeKnobs();
        applyKnobs();
        remember();
    },
    shuffle: () => {
        shuffle();
        applyKnobs();
        remember();
    },
    mode: () => {
        const dark = document.documentElement.dataset.mode === "dark";
        setKnobValue("theme", dark ? "light" : "dark");
        applyKnobs();
        remember();
    },
    import: () => {
        document.getElementById("import-error").value = "";
        importDialog.showModal();
    },
    copy: copyTheme,
    "show-all": () => inspect(null),
};

function setKnobsWidth(width) {
    const widest = Math.max(KNOBS_MIN_WIDTH, window.innerWidth - 360);
    const clamped = Math.round(Math.min(widest, Math.max(KNOBS_MIN_WIDTH, width)));
    document.body.style.setProperty("--knobs-width", `${clamped}px`);
    return clamped;
}

function storeKnobsWidth(width) {
    try {
        if (width === null) localStorage.removeItem(`${book.storage}-knobs-width`);
        else localStorage.setItem(`${book.storage}-knobs-width`, String(width));
    } catch {}
}

function resizableKnobs() {
    const handle = document.getElementById("knobs-resize");
    const aside = document.getElementById("knobs");
    let stored = null;
    try {
        stored = Number(localStorage.getItem(`${book.storage}-knobs-width`));
    } catch {}
    if (stored > 0) setKnobsWidth(stored);
    handle.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        handle.setPointerCapture(event.pointerId);
        handle.classList.add("dragging");
        document.body.classList.add("resizing");
        const right = aside.getBoundingClientRect().right;
        const done = new AbortController();
        const end = () => {
            done.abort();
            handle.classList.remove("dragging");
            document.body.classList.remove("resizing");
            storeKnobsWidth(Math.round(aside.getBoundingClientRect().width));
        };
        handle.addEventListener("pointermove", (moved) => setKnobsWidth(right - moved.clientX), { signal: done.signal });
        handle.addEventListener("pointerup", end, { signal: done.signal });
        handle.addEventListener("pointercancel", end, { signal: done.signal });
    });
    handle.addEventListener("dblclick", () => {
        document.body.style.removeProperty("--knobs-width");
        storeKnobsWidth(null);
    });
    handle.addEventListener("keydown", (event) => {
        const step = { ArrowLeft: 16, ArrowRight: -16 }[event.key];
        if (!step) return;
        event.preventDefault();
        storeKnobsWidth(setKnobsWidth(aside.getBoundingClientRect().width + step));
    });
    window.addEventListener("resize", () => {
        const width = parseFloat(document.body.style.getPropertyValue("--knobs-width"));
        if (width > 0) setKnobsWidth(width);
    });
}

function waitFor(predicate) {
    return new Promise((resolve) => {
        const tick = () => (predicate() ? resolve() : setTimeout(tick, 16));
        tick();
    });
}

await init();
wasm.run();
book = JSON.parse(wasm.catalog());
resizableKnobs();
await waitFor(wasm.is_ready);
stories = JSON.parse(wasm.stories());
glassPresets = JSON.parse(wasm.glass_presets());
themes = JSON.parse(wasm.interface_themes());
accents = JSON.parse(wasm.accents());
renderNav();
renderKnobs();
const initial = parseHash();
loadKnobs(initial.params);
loadView(initial.params);
applyKnobs();
remember();
show(initial.story, initial.section);

form.addEventListener("input", (event) => {
    const { name } = event.target;
    if (name === "interface-theme") resetThemeKnobs();
    else if (name === "glass") resetGlassSliders();
    else if (name === "tint-color" && accentNamed(event.target.value)) setTint(tintOf({ "tint-color": event.target.value }));
    else if (name === "light-tint-color" || name === "dark-tint-color") setKnobValue("tint-color", "custom");
    else if (name === "control-corners" && event.target.value === "own" && knobValue("control-radius") === knobValue("radius")) {
        setKnobValue("control-radius", Math.max(0, Number(knobValue("radius")) - 4));
    }
    applyKnobs();
});
form.addEventListener("change", () => remember());
form.addEventListener("click", (event) => {
    const tab = event.target.closest("[data-step]");
    if (tab && !tab.classList.contains("step")) showStep(tab.dataset.step);
});
form.addEventListener("keydown", (event) => {
    const tab = event.target.closest("[role=tab]");
    if (!tab || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const index = STEPS.indexOf(step);
    const next = {
        ArrowLeft: Math.max(0, index - 1),
        ArrowRight: Math.min(STEPS.length - 1, index + 1),
        Home: 0,
        End: STEPS.length - 1,
    }[event.key];
    showStep(STEPS[next]);
    form.querySelector(`[role=tab][data-step="${STEPS[next]}"]`)?.focus();
});
form.addEventListener("reset", () =>
    setTimeout(() => {
        loadKnobs(new URLSearchParams());
        applyKnobs();
        remember();
    }),
);
document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-action]");
    if (button && actions[button.dataset.action]) {
        event.preventDefault();
        actions[button.dataset.action]();
    }
});
document.addEventListener("keydown", (event) => {
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "z") return;
    if (event.target.closest?.("textarea, input[type=text]")) return;
    event.preventDefault();
    actions[event.shiftKey ? "redo" : "undo"]();
});
importDialog.addEventListener("close", () => {
    if (importDialog.returnValue !== "apply") return;
    try {
        importTheme(document.getElementById("import-json").value);
        applyKnobs();
        remember();
        toast("Theme imported");
    } catch (error) {
        document.getElementById("import-error").value = String(error);
        importDialog.showModal();
    }
});
document.getElementById("font-file").addEventListener("change", async (event) => {
    const [file] = event.target.files;
    const status = document.getElementById("font-status");
    if (!file) return;
    try {
        const names = JSON.parse(wasm.add_font(new Uint8Array(await file.arrayBuffer())));
        if (names.length === 0) throw new Error("no new font family in that file");
        names.forEach(addFontOption);
        setKnobValue("face", names[0]);
        status.value = names[0];
        applyKnobs();
        remember();
    } catch (error) {
        status.value = String(error.message ?? error);
    }
    event.target.value = "";
});
window.addEventListener("hashchange", () => {
    const { story, section, query, params } = parseHash();
    if (query) {
        loadKnobs(params);
        loadView(params);
        applyKnobs();
        remember();
    }
    show(story, section);
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyKnobs);
regions.addEventListener("click", (event) => {
    const link = event.target.closest("a");
    if (!link) return;
    event.preventDefault();
    show(CREATOR, link.getAttribute("href").split("/").pop());
});

window.storybook = {
    stories: () => stories,
    current: () => current,
    show: (story, section) => {
        show(story, section);
        return globalThis.ala?.idle();
    },
    setKnobs: (knobs) => {
        setKnobs(knobs);
        applyKnobs();
        remember();
        return globalThis.ala?.idle();
    },
    knobs: () => Object.fromEntries(themeParams()),
    step: (name) => {
        if (name !== undefined) showStep(name);
        return step;
    },
    inspect: (id) => {
        inspect(id);
        return globalThis.ala?.idle();
    },
    theme: () => JSON.parse(wasm.theme_file()),
    importTheme: (theme) => {
        importTheme(typeof theme === "string" ? theme : JSON.stringify(theme));
        applyKnobs();
        remember();
        return globalThis.ala?.idle();
    },
    themes: () => themes,
    glassPresets: () => glassPresets,
};
