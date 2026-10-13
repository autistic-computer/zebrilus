import init, * as zz from "./wasm/zz_storybook.js";

const CREATOR = "theme-creator";
const GLASS_KNOBS = [
    "glass-blur",
    "glass-tint",
    "glass-refraction",
    "glass-bezel",
    "glass-dispersion",
    "glass-saturation",
    "glass-brightness",
    "glass-contrast",
    "glass-specular",
    "glass-glint-width",
    "glass-light",
    "glass-fresnel",
    "glass-edge",
    "glass-edge-width",
    "glass-noise",
];
const LOOK_KNOBS = ["radius", "outline", "row-inset", "shadows", "selection", "primary", "field", "font"];
const COLORS = [
    ["background", "Background"],
    ["foreground", "Foreground"],
    ["accent", "Accent"],
    ["success", "Success"],
    ["warning", "Warning"],
    ["danger", "Danger"],
    ["on-accent", "Text on accent"],
    ["shadow-color", "Shadow"],
];
const MODES = ["light", "dark"];
const COLOR_KNOBS = MODES.flatMap((mode) => COLORS.map(([name]) => `${mode}-${name}`));
const SHUFFLED = ["radius", "outline", "row-inset"];
const KNOBS = [
    "theme",
    "contrast",
    "zoom",
    "pane-gaps",
    "pane-margin",
    "pane-corner-radius",
    "pane-border-width",
    "pane-opacity",
    "pane-inactive-opacity",
    "pane-glow",
    "motion",
    "backdrop",
    "interface-theme",
    ...LOOK_KNOBS,
    ...COLOR_KNOBS,
    "glass",
    ...GLASS_KNOBS,
];
const DEFAULTS = {
    theme: "system",
    "interface-theme": "default",
    contrast: "1",
    zoom: "1",
    "pane-gaps": "0",
    "pane-margin": "6",
    "pane-corner-radius": "13.5",
    "pane-border-width": "0.5",
    "pane-opacity": "0.5",
    "pane-inactive-opacity": "0.7",
    "pane-glow": "1",
    motion: "1",
    backdrop: "plain",
    glass: "theme",
};

const nav = document.getElementById("nav");
const storyRoot = document.getElementById("story");
const form = document.getElementById("knob-form");
const importDialog = document.getElementById("import-dialog");

let stories = [];
let glassPresets = {};
let themes = {};
let current = null;
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
    if (input.type === "checkbox") input.checked = value !== "0" && value !== "false";
    else input.value = value;
}

function isRange(name) {
    return field(name).type === "range";
}

function glassMaterial(glass = knobValue("glass"), theme = knobValue("interface-theme")) {
    return (glass === "theme" ? themes[theme]?.glass : glassPresets[glass]) ?? null;
}

function knobDefault(name, glass = knobValue("glass"), theme = knobValue("interface-theme")) {
    if (LOOK_KNOBS.includes(name) || COLOR_KNOBS.includes(name)) {
        return String(themes[theme]?.[name] ?? "");
    }
    if (!GLASS_KNOBS.includes(name)) return DEFAULTS[name];
    const value = glassMaterial(glass, theme)?.[name];
    return value === undefined ? undefined : String(value);
}

function isDefault(name) {
    const value = knobValue(name);
    const fallback = knobDefault(name);
    if (COLOR_KNOBS.includes(name)) return value.toLowerCase() === fallback.toLowerCase();
    if (GLASS_KNOBS.includes(name) && glassMaterial() === null) return true;
    if (!isRange(name)) return value === fallback;
    const step = Number(field(name).step) || 0.01;
    return Math.abs(Number(value) - Number(fallback)) < step / 2;
}

function knobQuery() {
    const params = new URLSearchParams();
    for (const name of KNOBS) {
        if (!isDefault(name)) params.set(name, knobValue(name));
    }
    return params.toString();
}

function resetGlassSliders() {
    for (const name of GLASS_KNOBS) {
        const value = knobDefault(name);
        if (value !== undefined) setKnobValue(name, value);
    }
}

function resetThemeKnobs() {
    for (const name of [...LOOK_KNOBS, ...COLOR_KNOBS]) setKnobValue(name, knobDefault(name));
    setKnobValue("glass", "theme");
    resetGlassSliders();
}

function shownMode() {
    const mode = knobValue("theme");
    if (mode !== "system") return mode;
    return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function writeHash(story, section) {
    const query = knobQuery();
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
    nav.replaceChildren(element("h1", { text: "zz storybook" }));
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
            const [canvas] = story.sections;
            storyRoot.replaceChildren(
                element("h1", { class: "visually-hidden", text: story.name }),
                element("div", { id: canvas.mount, class: "mount canvas", "data-section": canvas.id }),
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
        zz.show(story.id);
        current = story.id;
        markCurrent(story.id);
        document.title = creator ? "Theme creator · zz" : `${story.name} · zz storybook`;
    }
    if (sectionId && !creator) {
        document.getElementById(`${story.id}-${sectionId}`)?.scrollIntoView({ block: "start" });
    } else {
        storyRoot.scrollTo?.(0, 0);
    }
    writeHash(story.id, creator ? undefined : sectionId);
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
    form.querySelector("#glass-knobs").hidden = glassMaterial() === null;
    form.querySelector("#outline-note").hidden = glassMaterial() === null;
    const mode = shownMode();
    for (const each of MODES) form.querySelector(`#colors-${each}`).hidden = each !== mode;
    for (const name of KNOBS) {
        const output = form.elements[`${name}-value`];
        if (output) output.value = knobValue(name);
    }
    paintSliders();
    zz.set_knobs(knobQuery());
    paintChrome();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function remember() {
    const query = knobQuery();
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
    const theme = JSON.parse(zz.theme());
    const root = document.documentElement.style;
    for (const key of ["background", "raised", "raised2", "raised3", "foreground", "muted", "border", "accent"]) {
        root.setProperty(`--zz-${key}`, theme[key]);
    }
    root.setProperty("--zz-radius", `${Math.min(theme.radius, 14)}px`);
    document.documentElement.dataset.mode = theme.mode;
}

function loadKnobs(params) {
    const glass = params.get("glass") ?? DEFAULTS.glass;
    const theme = params.get("interface-theme") ?? DEFAULTS["interface-theme"];
    setKnobValue("interface-theme", theme);
    for (const name of KNOBS) {
        const value = params.get(name) ?? knobDefault(name, glass, theme);
        if (value !== undefined) setKnobValue(name, value);
    }
}

function renderThemes() {
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
    for (const mode of MODES) {
        const fieldset = document.getElementById(`colors-${mode}`);
        for (const [name, label] of COLORS) {
            fieldset.append(
                element(
                    "label",
                    { class: "row color" },
                    element("span", { text: label }),
                    element("input", { type: "color", name: `${mode}-${name}` }),
                ),
            );
        }
    }
}

function addFontOption(name) {
    const select = field("font");
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
    for (const name of SHUFFLED) pick(name);
    setKnobValue("shadows", ["none", "surfaces", "all"][Math.floor(Math.random() * 3)]);
    setKnobValue("selection", Math.random() < 0.5 ? "accent" : "wash");
    setKnobValue("primary", Math.random() < 0.5 ? "foreground" : "accent");
    setKnobValue("field", Math.random() < 0.5 ? "soft" : "outline");
    const glasses = ["theme", "off", ...Object.keys(glassPresets)];
    setKnobValue("glass", glasses[Math.floor(Math.random() * glasses.length)]);
    resetGlassSliders();
}

function importTheme(json) {
    const knobs = JSON.parse(zz.import_theme(json));
    if ("glass" in knobs) {
        setKnobValue("glass", knobs.glass);
        resetGlassSliders();
    }
    for (const [name, value] of Object.entries(knobs)) {
        if (name === "font" && value) addFontOption(value);
        if (form.elements[name]) setKnobValue(name, String(value));
    }
}

async function copyTheme() {
    const json = zz.theme_file();
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
};

function waitFor(predicate) {
    return new Promise((resolve) => {
        const tick = () => (predicate() ? resolve() : setTimeout(tick, 16));
        tick();
    });
}

await init();
zz.run();
await waitFor(zz.is_ready);
stories = JSON.parse(zz.stories());
glassPresets = JSON.parse(zz.glass_presets());
themes = JSON.parse(zz.interface_themes());
renderNav();
renderThemes();
const initial = parseHash();
const font = initial.params.get("font");
if (font) addFontOption(font);
loadKnobs(initial.params);
applyKnobs();
remember();
show(initial.story, initial.section);

form.addEventListener("input", (event) => {
    if (event.target.name === "interface-theme") resetThemeKnobs();
    else if (event.target.name === "glass") resetGlassSliders();
    applyKnobs();
});
form.addEventListener("change", remember);
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
        const names = JSON.parse(zz.add_font(new Uint8Array(await file.arrayBuffer())));
        if (names.length === 0) throw new Error("no new font family in that file");
        names.forEach(addFontOption);
        setKnobValue("font", names[0]);
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
        applyKnobs();
        remember();
    }
    show(story, section);
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyKnobs);

window.storybook = {
    stories: () => stories,
    current: () => current,
    show: (story, section) => {
        show(story, section);
        return globalThis.zzGpui?.idle();
    },
    setKnobs: (knobs) => {
        if ("interface-theme" in knobs) {
            setKnobValue("interface-theme", String(knobs["interface-theme"]));
            resetThemeKnobs();
        }
        if ("glass" in knobs) {
            setKnobValue("glass", String(knobs.glass));
            resetGlassSliders();
        }
        for (const [name, value] of Object.entries(knobs)) {
            if (form.elements[name]) setKnobValue(name, String(value));
        }
        applyKnobs();
        remember();
        return globalThis.zzGpui?.idle();
    },
    knobs: () => Object.fromEntries(KNOBS.map((name) => [name, knobValue(name)])),
    theme: () => JSON.parse(zz.theme_file()),
    importTheme: (theme) => {
        importTheme(typeof theme === "string" ? theme : JSON.stringify(theme));
        applyKnobs();
        remember();
        return globalThis.zzGpui?.idle();
    },
    themes: () => themes,
    glassPresets: () => glassPresets,
};
