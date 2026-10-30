import init, * as wasm from "./wasm/storybook.js";

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
    "glass-light-directionality",
    "glass-fresnel",
    "glass-edge",
    "glass-edge-width",
    "glass-edge-spread",
    "glass-noise",
    "glass-inactive-flattening",
];
const LIGHT_GLASS_KNOBS = GLASS_KNOBS.map((name) => `light-${name}`);
const SHADOW_KNOBS = [
    "contact-y",
    "contact-blur",
    "contact-spread",
    "contact-strength",
    "float-y",
    "float-blur",
    "float-spread",
    "float-strength",
];
const MATERIAL_KNOBS = ["name", "radius", "control-radius", "smoothing", "smoothed", "outline", "outlines", "row-inset", "shadows", ...SHADOW_KNOBS, "primary", "highlights", "surfaces", "highlight-strength", "font"];
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
const SHUFFLED = ["radius", "outline", "row-inset", "contact-strength", "float-y", "float-blur", "float-strength"];
const STEPS = ["colors", "materials", "roles", "export", "app"];
const PROPS = [
    ["fill", "Fill"],
    ["outline", "Outline"],
    ["shadow", "Shadow"],
    ["shape", "Shape"],
    ["smoothing", "Smoothing"],
    ["hover", "Hover"],
    ["selected", "Selected"],
    ["ink", "Ink"],
];
const HASH_LIMIT = 2000;
const KNOBS_MIN_WIDTH = 280;
const KNOBS = [
    "theme",
    "contrast",
    "tint-hue",
    "tint",
    "zoom",
    "motion",
    "speed",
    "backdrop",
    "interface-theme",
    ...MATERIAL_KNOBS,
    ...COLOR_KNOBS,
    "glass",
    ...GLASS_KNOBS,
    ...LIGHT_GLASS_KNOBS,
];
const DEFAULTS = {
    theme: "system",
    "interface-theme": "default",
    contrast: "1",
    "tint-hue": "260",
    tint: "0",
    zoom: "1",
    motion: "1",
    speed: "1",
    backdrop: "plain",
    glass: "theme",
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
let current = null;
let roles = {};
let roleTree = [];
let step = "colors";
let selectedRole = null;
let focusRetries = 0;
const openRegions = new Set();
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

function lightGlassMaterial(glass = knobValue("glass"), theme = knobValue("interface-theme")) {
    return (glass === "theme" ? themes[theme]?.["light-glass"] : null) ?? null;
}

function editsLightGlass() {
    return shownMode() === "light" && glassMaterial() !== null && lightGlassMaterial() !== null;
}

function knobDefault(name, glass = knobValue("glass"), theme = knobValue("interface-theme")) {
    if (MATERIAL_KNOBS.includes(name) || COLOR_KNOBS.includes(name)) {
        return String(themes[theme]?.[name] ?? "");
    }
    if (LIGHT_GLASS_KNOBS.includes(name)) {
        const value = lightGlassMaterial(glass, theme)?.[name.slice("light-".length)];
        return value === undefined ? undefined : String(value);
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
    if (LIGHT_GLASS_KNOBS.includes(name) && lightGlassMaterial() === null) return true;
    if (!isRange(name)) return value === fallback;
    const step = Number(field(name).step) || 0.01;
    return Math.abs(Number(value) - Number(fallback)) < step / 2;
}

function propLabel(value) {
    if (value === true) return "on";
    if (value === false) return "off";
    return String(value);
}

function sortRoles(map) {
    const sorted = {};
    for (const id of Object.keys(map ?? {}).sort()) {
        const patch = {};
        for (const [prop] of PROPS) {
            if (map[id]?.[prop] !== undefined) patch[prop] = map[id][prop];
        }
        if (Object.keys(patch).length > 0) sorted[id] = patch;
    }
    return sorted;
}

function presetRoles(theme = knobValue("interface-theme")) {
    return sortRoles(themes[theme]?.roles);
}

function themeParams() {
    const params = new URLSearchParams();
    for (const name of KNOBS) {
        if (!isDefault(name)) params.set(name, knobValue(name));
    }
    const encoded = JSON.stringify(roles);
    if (encoded !== JSON.stringify(presetRoles())) params.set("roles", encoded);
    return params;
}

function focusedRole() {
    return step === "roles" ? selectedRole : null;
}

function knobQuery() {
    return themeParams().toString();
}

function hashQuery() {
    const params = themeParams();
    if (params.toString().length > HASH_LIMIT) params.delete("roles");
    if (step !== STEPS[0]) params.set("step", step);
    if (step === "roles" && selectedRole) params.set("inspect", selectedRole);
    return params.toString();
}

function storeRoles() {
    try {
        const preset = presetRoles();
        const edits = {};
        for (const id of new Set([...Object.keys(preset), ...Object.keys(roles)])) {
            const patch = roles[id] ?? {};
            if (JSON.stringify(patch) !== JSON.stringify(preset[id] ?? {})) edits[id] = patch;
        }
        localStorage.setItem(`${book.storage}-role-edits`, JSON.stringify({ theme: knobValue("interface-theme"), edits }));
    } catch {}
}

function validRoles(map) {
    try {
        if (typeof map !== "object" || map === null) return null;
        wasm.import_theme(JSON.stringify({ roles: map }));
        return sortRoles(map);
    } catch {
        return null;
    }
}

function storedRoles(theme) {
    try {
        const stored = JSON.parse(localStorage.getItem(`${book.storage}-role-edits`) ?? "null");
        return stored?.theme === theme ? validRoles({ ...presetRoles(theme), ...stored.edits }) : null;
    } catch {
        return null;
    }
}

function resetGlassSliders() {
    for (const name of [...GLASS_KNOBS, ...LIGHT_GLASS_KNOBS]) {
        const value = knobDefault(name);
        if (value !== undefined) setKnobValue(name, value);
    }
}

function resetThemeKnobs() {
    for (const name of [...MATERIAL_KNOBS, ...COLOR_KNOBS]) setKnobValue(name, knobDefault(name));
    setKnobValue("glass", "theme");
    resetGlassSliders();
    roles = presetRoles();
}

function shownMode() {
    const mode = knobValue("theme");
    if (mode !== "system") return mode;
    return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
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
        wasm.inspect("");
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
    form.querySelector("#glass-knobs").hidden = glassMaterial() === null || editsLightGlass();
    form.querySelector("#light-glass-knobs").hidden = !editsLightGlass();
    form.querySelector("#outline-note").hidden = glassMaterial() === null;
    form.querySelector("#smoothed-note").hidden = knobValue("smoothed") !== "picked";
    form.querySelector("#outlines-note").hidden = knobValue("outlines") !== "picked";
    form.querySelector("#shadows-note").hidden = knobValue("shadows") !== "picked";
    const mode = shownMode();
    for (const each of MODES) form.querySelector(`#colors-${each}`).hidden = each !== mode;
    for (const name of KNOBS) {
        const output = form.elements[`${name}-value`];
        if (output) output.value = knobValue(name);
    }
    paintSliders();
    wasm.set_knobs(knobQuery());
    storeRoles();
    paintChrome();
    paintStep();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function rolesChanged() {
    return JSON.stringify(roleTree.map((region) => region.roles.map((role) => [role.patch, role.inherited])));
}

function paintStep() {
    for (const tab of form.querySelectorAll("[role=tab]")) {
        const active = tab.dataset.step === step;
        tab.setAttribute("aria-selected", String(active));
        tab.tabIndex = active ? 0 : -1;
    }
    for (const panel of form.querySelectorAll(".step")) panel.hidden = panel.dataset.step !== step;
    if (step === "roles") {
        const before = rolesChanged();
        roleTree = JSON.parse(wasm.roles());
        if (before !== rolesChanged() || !document.getElementById("role-tree").hasChildNodes()) renderRoles();
    }
    focusShowcase();
    if (step === "export") document.getElementById("theme-json").textContent = wasm.theme_file();
}

function showStep(name, flashing = true) {
    if (!STEPS.includes(name) || name === step) return;
    step = name;
    focusRetries = 0;
    paintStep();
    if (flashing) flash();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function flash() {
    wasm.inspect(current === CREATOR ? (focusedRole() ?? "") : "");
}

function focusShowcase() {
    if (current !== CREATOR) return;
    const id = focusedRole();
    const drawn = id ? JSON.parse(wasm.drawn_roles()) : {};
    const sections = [...storyRoot.querySelectorAll("section.region")];
    const shows = sections.map((section) => {
        const names = drawn[section.dataset.region];
        return !id || !names || names.includes(id);
    });
    const found = sections.filter((section, index) => shows[index] && drawn[section.dataset.region]?.includes(id));
    const filtering = Boolean(id) && found.length > 0;
    sections.forEach((section, index) => {
        section.hidden = filtering && !shows[index];
    });
    for (const link of regions.children) {
        link.hidden = filtering && !found.some((section) => section.dataset.region === link.dataset.region);
    }
    const note = document.getElementById("focus-note");
    note.hidden = !id;
    storyRoot.style.setProperty("--focus-note", note.hidden ? "0px" : `${note.offsetHeight}px`);
    if (id) {
        const role = findRole(id);
        const names = found.map((section) => section.querySelector("h2").textContent);
        note.querySelector("span").textContent = `${role?.label ?? id} · ${
            found.length ? `in ${names.join(", ")}` : "not in the showcase"
        }`;
    }
    const pending = id && sections.some((section) => !drawn[section.dataset.region]);
    if (pending && focusRetries < 25) {
        focusRetries += 1;
        setTimeout(focusShowcase, 200);
    }
}

function findRole(id) {
    for (const region of roleTree) {
        const role = region.roles.find((candidate) => candidate.id === id);
        if (role) return role;
    }
    return null;
}

function setRoleProp(id, prop, value) {
    const role = findRole(id);
    if (!role || !PROPS.some(([name]) => name === prop)) throw new Error(`no role ${id} or prop ${prop}`);
    const clears = value === "inherit" || value === null || value === undefined;
    if (!clears && !role.knobs[prop].includes(value)) throw new Error(`${id} can't set ${prop} to ${value}`);
    const patch = { ...(roles[id] ?? {}) };
    if (clears || (value === role.inherited[prop].value && role.members === 0)) {
        delete patch[prop];
    } else {
        patch[prop] = value;
    }
    roles = sortRoles({ ...roles, [id]: patch });
}

function selectRole(id, flashing = true) {
    selectedRole = id && findRole(id) ? id : null;
    if (selectedRole) {
        const region = roleTree.find((candidate) => candidate.roles.some((role) => role.id === selectedRole));
        if (region) openRegions.add(region.title);
    }
    renderRoles();
    focusRetries = 0;
    focusShowcase();
    storyRoot.scrollTo?.(0, 0);
    if (flashing) flash();
    const { story, section } = parseHash();
    writeHash(story || current, section);
}

function matchesFilter(role, query) {
    if (!query) return true;
    return [role.id, role.label, role.about, role.kind].some((text) => text.toLowerCase().includes(query));
}

function roleSummary(role) {
    const style = role.style;
    const parts = [style.fill === "none" ? "no fill" : style.fill];
    if (style.outline) parts.push("outline");
    if (style.shadow !== "none") parts.push(style.shadow);
    parts.push(style.shape);
    return parts.join(" · ");
}

function propControl(role, [prop, label]) {
    const values = role.knobs[prop];
    const set = role.patch[prop];
    const inherited = role.inherited[prop];
    const name = `role-${prop}`;
    const option = (value, text, checked, extra = "") =>
        element(
            "label",
            { class: extra },
            element("input", Object.assign(
                { type: "radio", name, value: propLabel(value), "data-role": role.id, "data-prop": prop },
                checked ? { checked: "" } : {},
            )),
            element("span", { text }),
        );
    const segments = element(
        "div",
        { class: "segmented compact", role: "radiogroup", "aria-label": label },
        option("inherit", "inherit", set === undefined),
        ...values.map((value) =>
            option(
                value,
                propLabel(value),
                set === value,
                set === undefined && value === inherited.value ? "inherited" : "",
            ),
        ),
    );
    return element(
        "div",
        { class: "prop" },
        element(
            "div",
            { class: "prop-head" },
            element("span", { text: label }),
            element(
                "small",
                {},
                set === undefined ? "inherits " : "set here · inherits ",
                element("b", { text: propLabel(inherited.value) }),
                ` from ${inherited.from}${inherited.themed ? " in this theme" : ""}`,
            ),
        ),
        segments,
    );
}

function renderRoles() {
    const tree = document.getElementById("role-tree");
    const query = document.getElementById("role-filter").value.trim().toLowerCase();
    const focused = document.activeElement?.closest?.("#role-tree") ? document.activeElement : null;
    const focus = focused && { name: focused.name, value: focused.value, role: focused.dataset.roleHead };
    tree.replaceChildren(
        ...roleTree.flatMap((region) => {
            const members = region.roles.filter((role) => matchesFilter(role, query));
            if (members.length === 0) return [];
            const edited = members.filter((role) => Object.keys(role.patch).length > 0).length;
            const details = element(
                "details",
                { class: "region", "data-region": region.title },
                element(
                    "summary",
                    {},
                    element("span", { text: region.title }),
                    element("small", { text: edited ? `${edited} set · ${members.length}` : String(members.length) }),
                ),
                ...members.map((role) => {
                    const selected = role.id === selectedRole;
                    const next = region.roles[region.roles.indexOf(role) + 1];
                    const grouping = role.depth === 0 ? "kind" : (next?.depth ?? 0) > role.depth ? "group" : "";
                    const patched = Object.keys(role.patch).length > 0;
                    const head = element(
                        "button",
                        {
                            type: "button",
                            class: "role-head",
                            "data-role-head": role.id,
                            "aria-expanded": String(selected),
                            title: role.about,
                        },
                        element("span", { class: "role-label", text: role.label }),
                        element("span", { class: patched ? "role-dot" : "role-dot hidden", "aria-hidden": "true" }),
                        element("small", { class: "role-kind", text: grouping }),
                        element("small", { class: "role-summary", text: roleSummary(role) }),
                    );
                    const body = selected
                        ? [
                              element(
                                  "div",
                                  { class: "role-body" },
                                  element("p", {}, element("code", { text: role.id }), ` ${role.about}.`),
                                  ...(PROPS.some(([prop]) => role.knobs[prop].length > 0)
                                      ? []
                                      : [element("p", { text: "Themes don't change this one." })]),
                                  ...PROPS.filter(([prop]) => role.knobs[prop].length > 0).map((spec) =>
                                      propControl(role, spec),
                                  ),
                              ),
                          ]
                        : [];
                    return element(
                        "div",
                        { class: selected ? "role selected" : "role", "data-role": role.id, style: `--depth: ${query ? 0 : role.depth}` },
                        head,
                        ...body,
                    );
                }),
            );
            details.open = query !== "" || openRegions.has(region.title);
            return [details];
        }),
    );
    if (focus) {
        const selector = focus.role
            ? `[data-role-head="${CSS.escape(focus.role)}"]`
            : `input[name="${CSS.escape(focus.name)}"][value="${CSS.escape(focus.value)}"]`;
        tree.querySelector(selector)?.focus();
    }
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
    loadKnobs(new URLSearchParams(timeline.last), false);
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

function loadKnobs(params, stored) {
    const glass = params.get("glass") ?? DEFAULTS.glass;
    const theme = params.get("interface-theme") ?? DEFAULTS["interface-theme"];
    setKnobValue("interface-theme", theme);
    for (const name of KNOBS) {
        const value = params.get(name) ?? knobDefault(name, glass, theme);
        if (value !== undefined) setKnobValue(name, value);
    }
    let loaded = null;
    try {
        loaded = params.has("roles") ? validRoles(JSON.parse(params.get("roles"))) : null;
    } catch {}
    roles = loaded ?? (stored ? storedRoles(theme) : null) ?? presetRoles(theme);
}

function loadView(params) {
    if (STEPS.includes(params.get("step"))) step = params.get("step");
    if (params.has("inspect")) selectedRole = findRole(params.get("inspect")) ? params.get("inspect") : null;
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
    const glass = document.getElementById("glass-knobs");
    const light = glass.cloneNode(true);
    light.id = "light-glass-knobs";
    for (const node of light.querySelectorAll("[name]")) node.setAttribute("name", `light-${node.getAttribute("name")}`);
    light.prepend(element("p", { class: "knob-note", text: "Light mode draws its own glass; these edit it." }));
    glass.after(light);
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
    document.querySelector(".start").hidden = Object.keys(themes).length < 2;
}

function renderAppKnobs() {
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
        KNOBS.splice(KNOBS.indexOf("motion"), 0, knob.name);
        DEFAULTS[knob.name] = knob.kind === "check" ? (knob.value ? "1" : "0") : String(knob.value);
    }
    for (const node of document.querySelectorAll("[data-catalog-name]")) node.textContent = book.name;
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
    setKnobValue("primary", Math.random() < 0.5 ? "foreground" : "accent");
    setKnobValue("highlights", Math.random() < 0.5 ? "lift" : "flat");
    setKnobValue("surfaces", Math.random() < 0.5 ? "darker" : "brighter");
    const glasses = ["theme", "off", ...Object.keys(glassPresets)];
    setKnobValue("glass", glasses[Math.floor(Math.random() * glasses.length)]);
    resetGlassSliders();
}

function importTheme(json) {
    const knobs = JSON.parse(wasm.import_theme(json));
    if ("glass" in knobs) {
        setKnobValue("glass", knobs.glass);
        resetGlassSliders();
    }
    roles = sortRoles(knobs.roles);
    for (const [name, value] of Object.entries(knobs)) {
        if (name === "font" && value) addFontOption(value);
        if (form.elements[name]) setKnobValue(name, String(value));
    }
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
renderNav();
renderThemes();
renderAppKnobs();
const initial = parseHash();
const font = initial.params.get("font");
if (font) addFontOption(font);
loadKnobs(initial.params, true);
roleTree = JSON.parse(wasm.roles());
loadView(initial.params);
if (selectedRole) {
    const region = roleTree.find((candidate) => candidate.roles.some((role) => role.id === selectedRole));
    if (region) openRegions.add(region.title);
} else {
    openRegions.add(roleTree[0]?.title);
}
applyKnobs();
remember();
show(initial.story, initial.section);
revealSelectedRole();

function revealSelectedRole() {
    const aside = document.getElementById("knobs");
    const target = document.querySelector("#role-tree .role.selected");
    if (!target) return;
    const offset = target.getBoundingClientRect().top - aside.getBoundingClientRect().top;
    aside.scrollTop += offset - 120;
}

form.addEventListener("input", (event) => {
    const target = event.target;
    if (target.id === "role-filter") {
        renderRoles();
        return;
    }
    if (target.dataset.prop) {
        const values = findRole(target.dataset.role)?.knobs[target.dataset.prop] ?? [];
        const value = target.value === "inherit" ? "inherit" : values.find((each) => propLabel(each) === target.value);
        setRoleProp(target.dataset.role, target.dataset.prop, value);
        applyKnobs();
        remember();
        return;
    }
    if (target.name === "interface-theme") resetThemeKnobs();
    else if (target.name === "glass") resetGlassSliders();
    applyKnobs();
});
form.addEventListener("change", (event) => {
    if (!event.target.dataset.prop && event.target.id !== "role-filter") remember();
});
form.addEventListener("click", (event) => {
    const tab = event.target.closest("[data-step], [data-goto]");
    if (tab && !tab.classList.contains("step")) {
        showStep(tab.dataset.goto ?? tab.dataset.step);
        if (tab.dataset.goto) document.getElementById("knobs").scrollTop = 0;
        return;
    }
    const head = event.target.closest("[data-role-head]");
    if (head) selectRole(head.dataset.roleHead === selectedRole ? null : head.dataset.roleHead);
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
const roleTreeNode = document.getElementById("role-tree");
roleTreeNode.addEventListener("toggle", (event) => {
    const region = event.target.dataset?.region;
    if (!region || document.getElementById("role-filter").value.trim()) return;
    if (event.target.open) openRegions.add(region);
    else openRegions.delete(region);
}, true);
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
        loadKnobs(params, false);
        loadView(params);
        applyKnobs();
        remember();
    }
    show(story, section);
});
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyKnobs);
storyRoot.addEventListener("click", (event) => {
    if (event.target.closest("[data-action=show-all]")) selectRole(null);
});
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
        if ("roles" in knobs) {
            const map = typeof knobs.roles === "string" ? JSON.parse(knobs.roles) : knobs.roles;
            roles = validRoles(map) ?? roles;
        }
        applyKnobs();
        remember();
        return globalThis.ala?.idle();
    },
    knobs: () => Object.fromEntries(KNOBS.map((name) => [name, knobValue(name)])),
    step: (name) => {
        if (name !== undefined) showStep(name, false);
        return step;
    },
    roles: () => (roleTree = JSON.parse(wasm.roles())),
    setRole: (id, prop, value) => {
        roleTree = JSON.parse(wasm.roles());
        setRoleProp(id, prop, value);
        applyKnobs();
        remember();
        return globalThis.ala?.idle();
    },
    inspect: (id) => {
        showStep("roles", false);
        roleTree = JSON.parse(wasm.roles());
        selectRole(id || null, false);
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
