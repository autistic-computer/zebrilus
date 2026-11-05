use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap, HashSet},
    env,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

use markdown::{
    ParseOptions,
    mdast::{AlignKind, Code, Node},
};
use penna::{
    ThemeMode,
    foundation::{
        highlighter::{Rope, SyntaxHighlighter},
        interface_theme::{self, InterfaceTheme},
        theme_css::{self, SYNTAX},
    },
};
use serde::Deserialize;

const STYLES: [&str; 7] = [
    "components/button/button.css",
    "components/kbd/kbd.css",
    "components/segmented/segmented.css",
    "components/text/text.css",
    "components/tooltip/tooltip.css",
    "blocks/sidebar/sidebar.css",
    "blocks/docs/docs.css",
];

const SCRIPTS: [&str; 6] = [
    "components/button/button.js",
    "components/kbd/kbd.js",
    "components/segmented/segmented.js",
    "components/text/text.js",
    "components/tooltip/tooltip.js",
    "blocks/docs/docs.js",
];

const LOOKS: [(&str, &str); 11] = [
    ("default", "{}"),
    ("blue", r#"{ "tint": "blue" }"#),
    ("pink", r#"{ "tint": "pink" }"#),
    ("radius-4", r#"{ "radius": 4 }"#),
    ("radius-20", r#"{ "radius": 20 }"#),
    ("pill", r#"{ "control-radius": "full" }"#),
    ("smoothing-2", r#"{ "radius": 24, "smoothing": 2 }"#),
    ("smoothing-3", r#"{ "radius": 24 }"#),
    ("smoothing-5", r#"{ "radius": 24, "smoothing": 5 }"#),
    ("glass-none", r#"{ "glass": "none" }"#),
    ("glass-liquid", r#"{ "glass": "liquid" }"#),
];

const LOOK_ROLES: [&str; 8] = [
    "button",
    "button.quiet",
    "field",
    "menu-item",
    "popover",
    "card",
    "tile.card",
    "chip",
];

const KIT: &str = "../../../../src/";
const STORYBOOK: &str = "assets/storybook/wasm/storybook.js";

#[derive(Deserialize)]
struct Site {
    title: String,
    source: String,
    groups: Vec<Group>,
}

#[derive(Deserialize)]
struct Group {
    title: String,
    pages: Vec<String>,
}

#[derive(Deserialize)]
struct Parity {
    story: String,
    page: String,
}

fn story_pages_last(assets: BTreeSet<String>) -> impl Iterator<Item = String> {
    let (pages, widgets): (Vec<_>, Vec<_>) = assets
        .into_iter()
        .partition(|asset| asset.starts_with("assets/kit/"));
    widgets.into_iter().chain(pages)
}

struct Page {
    slug: String,
    title: String,
    group: usize,
    body: String,
    styles: BTreeSet<String>,
    scripts: BTreeSet<String>,
}

struct Builder {
    root: PathBuf,
    native: bool,
    mounts: Cell<usize>,
    styles: RefCell<BTreeSet<String>>,
    scripts: RefCell<BTreeSet<String>>,
}

struct StorySection {
    story: String,
    section: String,
    html: String,
}

fn main() -> io::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = env::args()
        .nth(1)
        .map_or_else(|| root.join("target/docs"), PathBuf::from);
    let site: Site = serde_json::from_str(&fs::read_to_string(root.join("docs/site.json"))?)
        .map_err(io::Error::other)?;
    let wasm = root.join("storybook/dist/wasm");
    let builder = Builder {
        native: wasm.join("storybook.js").exists(),
        root,
        mounts: Cell::new(0),
        styles: RefCell::default(),
        scripts: RefCell::default(),
    };

    let mut pages = Vec::new();
    for (group, entry) in site.groups.iter().enumerate() {
        for slug in &entry.pages {
            let source = fs::read_to_string(builder.root.join(format!("docs/pages/{slug}.md")))?;
            let tree = markdown::to_mdast(&source, &ParseOptions::gfm())
                .map_err(|error| io::Error::other(error.to_string()))?;
            let title = first_heading(&tree).unwrap_or_else(|| slug.clone());
            builder.mounts.set(0);
            let body = builder.page(&tree)?;
            pages.push(Page {
                slug: slug.clone(),
                title,
                group,
                body,
                styles: builder.styles.take(),
                scripts: builder.scripts.take(),
            });
        }
    }

    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    let assets = out.join("assets");
    copy_assets(&builder.root, &assets)?;
    if builder.native {
        copy_dir(&wasm, &assets.join("storybook/wasm"))?;
    }
    for (index, page) in pages.iter().enumerate() {
        let html = document(&site, &pages, index, builder.native);
        fs::write(out.join(file_name(&page.slug)), &html)?;
        if index == 0 {
            fs::write(out.join("index.html"), &html)?;
        }
    }
    println!("{}", out.join("index.html").display());
    Ok(())
}

fn file_name(slug: &str) -> String {
    format!("{slug}.html")
}

fn copy_assets(root: &Path, assets: &Path) -> io::Result<()> {
    fs::create_dir_all(assets.join("web"))?;
    for file in ["penna.css", "kit.js", "story.css"] {
        fs::copy(root.join("web").join(file), assets.join("web").join(file))?;
    }
    theme_css::write_tokens(&assets.join("web/tokens"), &[])?;
    write_looks(&assets.join("looks"))?;
    copy_tree(&root.join("src"), &assets.join("src"), &["css", "js"])?;
    copy_tree(
        &root.join("storybook/src/kit"),
        &assets.join("kit"),
        &["css", "js"],
    )?;
    copy_dir(
        &root.join("storybook/fonts"),
        &assets.join("storybook/fonts"),
    )?;
    fs::copy(root.join("docs/site.css"), assets.join("site.css"))?;
    Ok(())
}

struct Rule {
    path: Vec<String>,
    decls: Vec<(String, String)>,
}

fn write_looks(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    for (name, source) in LOOKS {
        let look = interface_theme::parse_theme(source).map_err(io::Error::other)?;
        fs::write(dir.join(format!("{name}.css")), look_sheet(name, &look))?;
    }
    fs::write(dir.with_file_name(RESET_SHEET), reset_sheet())?;
    Ok(())
}

const RESET_SHEET: &str = "looks-reset.css";

const MODES: [ThemeMode; 2] = [ThemeMode::Light, ThemeMode::Dark];

fn mode_rules(theme: &InterfaceTheme, mode: ThemeMode) -> (Vec<Rule>, Vec<String>) {
    let css = theme_css::stylesheet(&theme_css::kit_theme(theme, mode), &[]);
    let local = css
        .lines()
        .filter_map(|line| line.strip_prefix("@property "))
        .filter(|line| line.contains("inherits: false"))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect();
    (expanded(rules(&css)), local)
}

fn look_sheet(name: &str, look: &InterfaceTheme) -> String {
    let mut css = String::new();
    for mode in MODES {
        let (base, _) = mode_rules(interface_theme::default_theme(), mode);
        let (shown, _) = mode_rules(look, mode);
        let scope = format!(
            ":root[data-penna-mode=\"{}\"] [data-look=\"{name}\"]",
            mode.name()
        );
        write_scope(&mut css, &scope, &changed(&base, &shown));
    }
    css
}

fn reset_sheet() -> String {
    let default = interface_theme::default_theme();
    let mut css = String::new();
    for (name, preset) in interface_theme::builtin_themes() {
        if std::ptr::eq(preset, default) {
            continue;
        }
        for mode in MODES {
            let (base, _) = mode_rules(preset, mode);
            let (shown, local) = mode_rules(default, mode);
            let mut reset = changed(&base, &shown);
            reset.extend(removed(&base, &shown, &local));
            let scope = format!(
                ":root[data-penna-theme=\"{name}\"][data-penna-mode=\"{}\"] [data-look]",
                mode.name()
            );
            write_scope(&mut css, &scope, &reset);
        }
    }
    css
}

fn write_scope(css: &mut String, scope: &str, rules: &[Rule]) {
    let _ = writeln!(css, "@scope ({scope}) {{");
    let mut outer: Vec<&str> = Vec::new();
    for rule in rules {
        if let Some(first) = rule.path.first()
            && !outer.contains(&first.as_str())
        {
            outer.push(first);
        }
    }
    let grouped = outer.iter().flat_map(|first| {
        rules
            .iter()
            .filter(move |rule| rule.path.first().map(String::as_str) == Some(*first))
    });
    let mut open: Vec<&str> = Vec::new();
    for rule in grouped.filter(|rule| drawn_in_looks(rule)) {
        let Some((leaf, parents)) = rule.path.split_last() else {
            continue;
        };
        let kept = open
            .iter()
            .zip(parents)
            .take_while(|(open, parent)| **open == parent.as_str())
            .count();
        css.push_str(&"}\n".repeat(open.len() - kept));
        open.truncate(kept);
        for parent in &parents[kept..] {
            let _ = writeln!(css, "{} {{", scoped(parent));
            open.push(parent.as_str());
        }
        let _ = write!(css, "{} {{", scoped(leaf));
        for (name, value) in &rule.decls {
            let _ = write!(css, " {name}: {value};");
        }
        css.push_str(" }\n");
    }
    css.push_str(&"}\n".repeat(open.len()));
    css.push_str("}\n");
}

fn role_of(selector: &str) -> Option<&str> {
    let rest = selector.strip_prefix("[data-penna=\"")?;
    Some(&selector[..selector.len() - rest.len() + rest.find("\"]")? + 2])
}

fn in_cells(rule: &Rule) -> bool {
    rule.path
        .iter()
        .all(|selector| !selector.contains("[data-ground]") && !selector.contains("[data-on="))
}

fn variants<'a>(rules: &'a [Rule], role: &str) -> Vec<&'a str> {
    rules
        .iter()
        .filter(|rule| rule.path.len() == 1 && role_of(&rule.path[0]) == Some(role))
        .map(|rule| &rule.path[0][role.len()..])
        .filter(|extra| !extra.is_empty() && !extra.starts_with(':'))
        .collect()
}

fn expanded(rules: Vec<Rule>) -> Vec<Rule> {
    let known: HashSet<Vec<String>> = rules.iter().map(|rule| rule.path.clone()).collect();
    let mut copies = Vec::new();
    for rule in rules.iter().filter(|rule| in_cells(rule)) {
        let Some(at) = rule.path.iter().rposition(|part| !part.starts_with('&')) else {
            continue;
        };
        let leaf = &rule.path[at];
        let targets: Vec<(String, String)> = match role_of(leaf) {
            Some(role) if at > 0 => vec![(leaf.clone(), role.to_owned())],
            None if leaf.starts_with(":is(") && at + 1 == rule.path.len() => LOOK_ROLES
                .iter()
                .map(|role| format!("[data-penna=\"{role}\"]"))
                .filter(|role| leaf.contains(role.as_str()))
                .map(|role| (format!("&{role}"), role))
                .collect(),
            _ => Vec::new(),
        };
        for (selector, role) in targets {
            for extra in std::iter::once("").chain(variants(&rules, &role)) {
                let mut path = rule.path.clone();
                if selector.starts_with('&') {
                    path.push(format!("{selector}{extra}"));
                } else {
                    path[at] = format!("{selector}{extra}");
                }
                if path != rule.path && !known.contains(&path) {
                    copies.push(Rule {
                        path,
                        decls: rule.decls.clone(),
                    });
                }
            }
        }
    }
    let mut rules = rules;
    rules.extend(copies);
    rules
}

fn without_not(selector: &str) -> String {
    let mut kept = String::new();
    let mut rest = selector;
    while let Some(start) = rest.find(":not(") {
        kept.push_str(&rest[..start]);
        let mut depth = 0;
        let mut end = rest.len();
        for (offset, char) in rest[start..].char_indices() {
            match char {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + offset + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &rest[end..];
    }
    kept.push_str(rest);
    kept
}

fn removed(base: &[Rule], shown: &[Rule], local: &[String]) -> Vec<Rule> {
    let after: HashMap<&[String], &[(String, String)]> = shown
        .iter()
        .map(|rule| (rule.path.as_slice(), rule.decls.as_slice()))
        .collect();
    let value = |paths: &[Vec<String>], name: &str| {
        paths
            .iter()
            .find_map(|path| {
                after.get(path.as_slice()).and_then(|decls| {
                    decls
                        .iter()
                        .find(|(other, _)| other == name)
                        .map(|(_, value)| value.clone())
                })
            })
            .unwrap_or_else(|| {
                if local.iter().any(|other| other == name) {
                    "initial".to_owned()
                } else {
                    "inherit".to_owned()
                }
            })
    };
    let picked = |top: &str| {
        shown
            .iter()
            .find(|rule| {
                rule.path.len() == 2
                    && rule.path[0] == top
                    && rule.path[1].contains("[data-state~=\"selected\"]")
            })
            .map(|rule| rule.path[1].clone())
    };
    base.iter()
        .filter(|rule| in_cells(rule) && drawn_in_looks(rule))
        .filter(|rule| !after.contains_key(rule.path.as_slice()))
        .filter_map(|rule| {
            let at = rule.path.iter().rposition(|part| !part.starts_with('&'))?;
            let leaf = &rule.path[at];
            let mut states = &rule.path[at + 1..];
            let (role, extra) = if let Some(role) = role_of(leaf) {
                (role, &leaf[role.len()..])
            } else {
                let copy = states.first()?.strip_prefix('&')?;
                let role = role_of(copy)?;
                states = &states[1..];
                (role, &copy[role.len()..])
            };
            let top = format!("{role}{}", without_not(extra));
            let mut paths = Vec::new();
            for top in [top.as_str(), role] {
                let mut path = vec![top.to_owned()];
                path.extend(states.iter().cloned());
                if states.is_empty()
                    && role_of(leaf).is_none()
                    && let Some(state) = picked(top)
                {
                    paths.push(vec![top.to_owned(), state]);
                }
                paths.push(path);
                paths.push(vec![top.to_owned()]);
            }
            Some(Rule {
                path: rule.path.clone(),
                decls: rule
                    .decls
                    .iter()
                    .map(|(name, _)| (name.clone(), value(&paths, name)))
                    .collect(),
            })
        })
        .collect()
}

fn scoped(selector: &str) -> &str {
    if selector == ":root" {
        ":scope"
    } else {
        selector
    }
}

fn drawn_in_looks(rule: &Rule) -> bool {
    rule.path
        .iter()
        .rev()
        .find(|selector| !selector.starts_with('&'))
        .is_some_and(|selector| {
            selector == ":root"
                || LOOK_ROLES.iter().any(|role| {
                    let named = format!("[data-penna=\"{role}\"]");
                    selector.starts_with(&named)
                        || (selector.starts_with(":is(") && selector.contains(&named))
                })
        })
}

fn rules(css: &str) -> Vec<Rule> {
    let mut path: Vec<String> = Vec::new();
    let mut rules: Vec<Rule> = Vec::new();
    for line in css.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("@property") {
            continue;
        }
        if line == "}" {
            path.pop();
            continue;
        }
        match line.split_once(" {") {
            Some((selector, rest)) => {
                let mut at = path.clone();
                at.push(selector.to_owned());
                let (rest, closed) = rest
                    .trim()
                    .strip_suffix('}')
                    .map_or((rest, false), |rest| (rest, true));
                rules.push(Rule {
                    path: at.clone(),
                    decls: decls(rest),
                });
                if !closed {
                    path = at;
                }
            }
            None => {
                if let Some(rule) = rules.iter_mut().rev().find(|rule| rule.path == path) {
                    rule.decls.extend(decls(line));
                }
            }
        }
    }
    rules
}

fn decls(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter_map(|decl| decl.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

fn changed(base: &[Rule], shown: &[Rule]) -> Vec<Rule> {
    let lookup = |rules: &[Rule]| {
        rules
            .iter()
            .map(|rule| (rule.path.clone(), rule.decls.clone()))
            .collect::<HashMap<_, _>>()
    };
    let before = lookup(base);
    let after = lookup(shown);
    shown
        .iter()
        .filter_map(|rule| {
            let was = before.get(&rule.path).cloned().unwrap_or_default();
            let fallback = |name: &str| {
                after
                    .get(&rule.path[..rule.path.len() - 1])
                    .and_then(|parent| parent.iter().find(|(other, _)| other == name))
                    .map_or_else(|| "initial".to_owned(), |(_, value)| value.clone())
            };
            let decls = rule
                .decls
                .iter()
                .filter(|decl| !was.contains(decl))
                .cloned()
                .chain(
                    was.iter()
                        .filter(|(name, _)| rule.decls.iter().all(|(other, _)| other != name))
                        .map(|(name, _)| (name.clone(), fallback(name))),
                )
                .collect::<Vec<_>>();
            (!decls.is_empty()).then(|| Rule {
                path: rule.path.clone(),
                decls,
            })
        })
        .collect()
}

fn copy_tree(from: &Path, to: &Path, extensions: &[&str]) -> io::Result<()> {
    for entry in fs::read_dir(from)? {
        let path = entry?.path();
        let target = to.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_tree(&path, &target, extensions)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            fs::create_dir_all(to)?;
            fs::copy(&path, target)?;
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()))?;
        } else {
            fs::copy(&path, to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn document(site: &Site, pages: &[Page], current: usize, native: bool) -> String {
    let page = &pages[current];
    let mut html = String::new();
    let _ = write!(
        html,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n\
         <title>{} · {}</title>\n<link rel=\"stylesheet\" href=\"assets/web/penna.css\" />\n\
         <link rel=\"stylesheet\" href=\"assets/web/story.css\" />\n",
        escape(&page.title),
        escape(&site.title)
    );
    let styles = STYLES
        .iter()
        .map(|style| format!("assets/src/{style}"))
        .chain(page.styles.iter().cloned())
        .collect::<BTreeSet<_>>();
    for style in story_pages_last(styles) {
        let _ = writeln!(html, "<link rel=\"stylesheet\" href=\"{style}\" />");
    }
    html.push_str("<link rel=\"stylesheet\" href=\"assets/site.css\" />\n");
    html.push_str(
        "<script src=\"assets/web/kit.js\" data-tokens=\"assets/web/tokens/\"></script>\n",
    );
    let scripts = SCRIPTS
        .iter()
        .map(|script| format!("assets/src/{script}"))
        .chain(page.scripts.iter().cloned())
        .collect::<BTreeSet<_>>();
    for script in story_pages_last(scripts) {
        let _ = writeln!(html, "<script src=\"{script}\" defer></script>");
    }
    let storybook = if native {
        format!(" data-docs-storybook=\"{STORYBOOK}\"")
    } else {
        String::new()
    };
    let _ = write!(
        html,
        "</head>\n<body>\n<div class=\"penna-docs-layout\"{storybook}>\n{}\n<div class=\"penna-docs-body\">\n\
         <aside class=\"penna-docs-side\" data-penna=\"sidebar\"><div class=\"penna-docs-rule\" data-penna=\"rule.sidebar\" data-edge=\"right\"></div>{}</aside>\n\
         <main class=\"penna-docs-main\"><article class=\"penna-docs-page\">\n{}</article></main>\n\
         </div>\n</div>\n</body>\n</html>\n",
        header(site, pages, page.group),
        nav(site, pages, current),
        page.body
    );
    html
}

fn segments<'a>(labels: impl Iterator<Item = &'a str>, name: &str) -> String {
    let spans = labels.fold(String::new(), |mut html, label_text| {
        let _ = write!(
            html,
            "<span class=\"penna-segment\" role=\"button\">{}</span>",
            label(label_text)
        );
        html
    });
    format!(
        "<div class=\"penna-segmented\" data-penna=\"field\" data-value=\"0\" role=\"radiogroup\" aria-label=\"{name}\">\
         <div class=\"penna-segments\" data-penna=\"button.quiet\" data-state=\"current\"><span class=\"penna-segment-pill\"></span>{spans}</div></div>"
    )
}

fn label(text: &str) -> String {
    format!(
        "<span class=\"penna-label\" data-penna-text>{}</span>",
        escape(text)
    )
}

fn current_attrs(current: bool) -> &'static str {
    if current {
        " data-state=\"current\" aria-current=\"page\""
    } else {
        ""
    }
}

fn header(site: &Site, pages: &[Page], group: usize) -> String {
    let links = site
        .groups
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let first = pages.iter().find(|page| page.group == index)?;
            Some(format!(
                "<a class=\"penna-button\" data-penna=\"button.quiet\" data-size=\"small\" data-docs-link href=\"{}\"{}>{}</a>",
                file_name(&first.slug),
                current_attrs(index == group),
                label(&entry.title)
            ))
        })
        .collect::<String>();
    format!(
        "<header class=\"penna-docs-header\" data-penna=\"bar\" role=\"toolbar\">\
         <div class=\"penna-docs-rule\" data-penna=\"rule.bar\"></div>\
         <button class=\"penna-button\" data-penna=\"button.quiet\" data-shape=\"icon\" data-size=\"small\" aria-label=\"Pages\" data-docs-menu><i data-icon=\"panel-left\"></i></button>\
         <div class=\"penna-docs-start\"><a class=\"penna-docs-brand\" href=\"index.html\"><i data-icon=\"app-window\"></i><span data-penna-text>{}</span></a>\
         <div class=\"penna-docs-links\">{links}</div></div><div class=\"penna-docs-spacer\"></div>\
         <a class=\"penna-button\" data-penna=\"button.quiet\" data-shape=\"icon\" data-size=\"small\" aria-label=\"Source\" data-tooltip=\"Source\" href=\"{}\"><i data-icon=\"git-branch\"></i></a>\
         <button class=\"penna-button\" data-penna=\"button.quiet\" data-shape=\"icon\" data-size=\"small\" aria-label=\"Light or dark\" data-tooltip=\"Light or dark\" data-docs-mode><i data-icon=\"palette\"></i></button>\
         </header>",
        escape(&site.title),
        escape(&site.source)
    )
}

fn nav(site: &Site, pages: &[Page], current: usize) -> String {
    let mut html = String::from("<nav class=\"penna-docs-nav\" aria-label=\"Documentation\">");
    for (index, entry) in site.groups.iter().enumerate() {
        let _ = write!(
            html,
            "<div class=\"penna-sidebar-heading\"><span data-penna-text>{}</span></div>",
            escape(&entry.title)
        );
        for (position, page) in pages
            .iter()
            .enumerate()
            .filter(|(_, page)| page.group == index)
        {
            let _ = write!(
                html,
                "<a class=\"penna-button penna-sidebar-row\" data-penna=\"sidebar.row\" href=\"{}\"{}>{}</a>",
                file_name(&page.slug),
                current_attrs(position == current),
                label(&page.title)
            );
        }
    }
    html.push_str("</nav>");
    html
}

fn first_heading(tree: &Node) -> Option<String> {
    tree.children()?.iter().find_map(|node| match node {
        Node::Heading(heading) if heading.depth == 1 => Some(plain(&heading.children)),
        _ => None,
    })
}

fn plain(nodes: &[Node]) -> String {
    nodes.iter().map(Node::to_string).collect()
}

fn meta(code: &Code) -> Vec<&str> {
    code.meta
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .collect()
}

fn is_example(node: &Node) -> bool {
    matches!(node, Node::Code(code) if meta(code).contains(&"example"))
}

fn story_ref(code: &Code) -> Option<&str> {
    meta(code)
        .into_iter()
        .find_map(|token| token.strip_prefix("story="))
}

fn flush(html: &mut String, prose: &mut String) {
    if !prose.is_empty() {
        let _ = writeln!(html, "<div class=\"penna-md\">{prose}</div>");
        prose.clear();
    }
}

impl Builder {
    fn page(&self, tree: &Node) -> io::Result<String> {
        let mut html = String::new();
        let mut prose = String::new();
        let children = tree.children().map_or(&[][..], Vec::as_slice);
        let mut index = 0;
        while index < children.len() {
            if let Node::Code(code) = &children[index]
                && code.lang.as_deref() == Some("figure")
            {
                flush(&mut html, &mut prose);
                html.push_str(&self.figure(code)?);
                index += 1;
                continue;
            }
            if is_example(&children[index]) {
                flush(&mut html, &mut prose);
                let start = index;
                while index < children.len() && is_example(&children[index]) {
                    index += 1;
                }
                html.push_str(&self.example(&children[start..index])?);
                continue;
            }
            block(&mut prose, &children[index], 0);
            index += 1;
        }
        flush(&mut html, &mut prose);
        Ok(html)
    }

    fn story_section(&self, reference: &str) -> io::Result<StorySection> {
        let (module, section) = reference
            .split_once('/')
            .ok_or_else(|| io::Error::other(format!("story={reference} needs story/section")))?;
        let folder = self.root.join("storybook/src/kit").join(module);
        let parity: Parity = serde_json::from_str(&fs::read_to_string(folder.join("parity.json"))?)
            .map_err(io::Error::other)?;
        let page = fs::read_to_string(folder.join(&parity.page))?;
        let open = format!("<section data-section=\"{section}\"");
        let at = page
            .find(&open)
            .ok_or_else(|| io::Error::other(format!("{module} has no section {section}")))?;
        let start = at + page[at..].find('>').map_or(open.len(), |end| end + 1);
        let end = start
            + page[start..]
                .find("</section>")
                .ok_or_else(|| io::Error::other(format!("{module}/{section} is not closed")))?;
        for (attribute, set) in [("href=\"", &self.styles), ("src=\"", &self.scripts)] {
            let mut rest = page.as_str();
            while let Some(at) = rest.find(attribute) {
                rest = &rest[at + attribute.len()..];
                let path = &rest[..rest.find('"').unwrap_or(0)];
                let asset = Path::new(path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("css")
                            || extension.eq_ignore_ascii_case("js")
                    });
                if !asset {
                    continue;
                }
                if let Some(kit) = path.strip_prefix(KIT) {
                    set.borrow_mut().insert(format!("assets/src/{kit}"));
                } else if !path.contains('/') {
                    set.borrow_mut()
                        .insert(format!("assets/kit/{module}/{path}"));
                }
            }
        }
        Ok(StorySection {
            story: parity.story,
            section: section.to_owned(),
            html: page[start..end].to_owned(),
        })
    }

    fn example(&self, fences: &[Node]) -> io::Result<String> {
        let codes: Vec<&Code> = fences
            .iter()
            .filter_map(|node| match node {
                Node::Code(code) => Some(code),
                _ => None,
            })
            .collect();
        let story = codes
            .iter()
            .find_map(|code| story_ref(code))
            .map(|reference| self.story_section(reference))
            .transpose()?;
        let mut tabs: Vec<(String, String)> = codes
            .iter()
            .map(|code| {
                (
                    language_name(code.lang.as_deref()),
                    code_block(code.lang.as_deref(), &code.value),
                )
            })
            .collect();
        let written_html = codes
            .iter()
            .find(|code| code.lang.as_deref() == Some("html"))
            .map(|code| code.value.clone());
        let preview = match (&story, written_html) {
            (_, Some(html)) => format!("<div class=\"penna-docs-preview\">{html}</div>"),
            (Some(story), None) => {
                tabs.push((
                    "HTML".into(),
                    code_block(Some("html"), &shown_markup(&story.html)),
                ));
                self.story_preview(story)
            }
            (None, None) => String::new(),
        };
        let targets = if story.is_some() {
            self.targets()
        } else {
            String::new()
        };
        let switch = if tabs.len() > 1 {
            format!(
                "<div class=\"penna-docs-tabs\">{}</div>",
                segments(tabs.iter().map(|(name, _)| name.as_str()), "Language")
            )
        } else {
            String::new()
        };
        let panes = tabs
            .iter()
            .enumerate()
            .fold(String::new(), |mut html, (index, (_, code))| {
                let _ = write!(
                    html,
                    "<div class=\"penna-md\" data-tab{}>{code}</div>",
                    if index == 0 { "" } else { " hidden" }
                );
                html
            });
        Ok(format!(
            "<div class=\"penna-docs-example\" role=\"figure\">\
             {targets}{preview}{switch}<div class=\"penna-docs-code\">{panes}</div></div>\n"
        ))
    }

    fn figure(&self, code: &Code) -> io::Result<String> {
        meta(code)
            .into_iter()
            .filter_map(|token| token.strip_prefix("uses="))
            .flat_map(|list| list.split(','))
            .for_each(|name| self.uses(name));
        for look in code.value.split("data-look=\"").skip(1) {
            let name = &look[..look.find('"').unwrap_or(0)];
            if LOOKS.iter().all(|(known, _)| *known != name) {
                return Err(io::Error::other(format!("no look named {name}")));
            }
            let mut styles = self.styles.borrow_mut();
            styles.insert(format!("assets/{RESET_SHEET}"));
            styles.insert(format!("assets/looks/{name}.css"));
        }
        let body = match story_ref(code) {
            Some(reference) => {
                let story = self.story_section(reference)?;
                format!(
                    "<section class=\"penna-docs-stage\" data-section=\"{}\">{}</section>",
                    escape(&story.section),
                    story.html
                )
            }
            None => format!("<div class=\"penna-docs-preview\">{}</div>", code.value),
        };
        Ok(format!(
            "<div class=\"penna-docs-example docs-figure\" role=\"figure\">{body}</div>\n"
        ))
    }

    fn uses(&self, name: &str) {
        for tier in ["components", "blocks"] {
            for (extension, set) in [("css", &self.styles), ("js", &self.scripts)] {
                let file = format!("{tier}/{name}/{name}.{extension}");
                if self.root.join("src").join(&file).exists() {
                    set.borrow_mut().insert(format!("assets/src/{file}"));
                }
            }
        }
    }

    fn targets(&self) -> String {
        if !self.native {
            return String::new();
        }
        format!(
            "<div class=\"penna-docs-targets\">{}</div>",
            segments(["HTML", "Native"].into_iter(), "Render")
        )
    }

    fn story_preview(&self, story: &StorySection) -> String {
        let mount = self.mounts.get();
        self.mounts.set(mount + 1);
        format!(
            "<section class=\"penna-docs-stage\" data-section=\"{0}\" data-target=\"html\">{1}</section>\
             <div class=\"penna-docs-stage\" data-target=\"native\" hidden>\
             <div class=\"penna-docs-mount\" id=\"docs-native-{mount}\" data-story=\"{2}\" data-section=\"{0}\"></div></div>",
            escape(&story.section),
            story.html,
            escape(&story.story)
        )
    }
}

fn shown_markup(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(" data-id=\"") {
        text.push_str(&rest[..at]);
        rest = &rest[at + " data-id=\"".len()..];
        rest = &rest[rest.find('"').map_or(rest.len(), |end| end + 1)..];
    }
    text.push_str(rest);
    let lines: Vec<&str> = text
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .collect();
    let lines = &lines[..lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(0, |last| last + 1)];
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(indent..).unwrap_or("").trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn language_name(lang: Option<&str>) -> String {
    match lang {
        Some("rust") => "Rust".into(),
        Some("html") => "HTML".into(),
        Some("toml") => "TOML".into(),
        Some("json") => "JSON".into(),
        Some(other) => other.into(),
        None => "Text".into(),
    }
}

fn block(html: &mut String, node: &Node, depth: usize) {
    match node {
        Node::Heading(heading) => {
            let text = plain(&heading.children);
            let _ = write!(
                html,
                "<h{0} id=\"{1}\">{2}</h{0}>",
                heading.depth,
                slug(&text),
                inline_all(&heading.children)
            );
        }
        Node::Paragraph(paragraph) => {
            let _ = write!(html, "<p>{}</p>", inline_all(&paragraph.children));
        }
        Node::Code(code) => html.push_str(&code_block(code.lang.as_deref(), &code.value)),
        Node::Blockquote(quote) => {
            html.push_str("<div class=\"md-quote\"><blockquote>");
            for child in &quote.children {
                block(html, child, depth);
            }
            html.push_str("</blockquote></div>");
        }
        Node::List(list) => {
            let tag = if list.ordered { "ol" } else { "ul" };
            let _ = write!(html, "<{tag}>");
            let start = list.start.unwrap_or(1);
            for (index, item) in list.children.iter().enumerate() {
                let Node::ListItem(item) = item else { continue };
                let marker = match item.checked {
                    Some(checked) => format!(
                        "<span class=\"md-box\"><span class=\"md-check\" role=\"checkbox\" aria-readonly=\"true\" aria-checked=\"{checked}\">{}</span></span>",
                        if checked {
                            "<i data-icon=\"check\"></i>"
                        } else {
                            ""
                        }
                    ),
                    None if list.ordered => format!(
                        "<span class=\"md-prefix\">{}. </span>",
                        start as usize + index
                    ),
                    None => format!(
                        "<span class=\"md-prefix\">{} </span>",
                        ["•", "◦", "▪"][depth.min(2)]
                    ),
                };
                let mut body = String::new();
                let mut nested = String::new();
                for child in &item.children {
                    if matches!(child, Node::List(_)) {
                        block(&mut nested, child, depth + 1);
                    } else {
                        block(&mut body, child, depth + 1);
                    }
                }
                let _ = write!(
                    html,
                    "<li><div class=\"md-item\">{marker}<div class=\"md-body\">{body}</div></div>{nested}</li>"
                );
            }
            let _ = write!(html, "</{tag}>");
        }
        Node::Table(table) => {
            html.push_str(
                "<div class=\"md-table\"><div class=\"md-figure\" data-penna=\"card.figure\">",
            );
            for row in &table.children {
                let Node::TableRow(row) = row else { continue };
                html.push_str("<div class=\"md-row\">");
                for (column, cell) in row.children.iter().enumerate() {
                    let Node::TableCell(cell) = cell else {
                        continue;
                    };
                    let align = match table.align.get(column) {
                        Some(AlignKind::Center) => "center",
                        Some(AlignKind::Right) => "right",
                        _ => "left",
                    };
                    let _ = write!(
                        html,
                        "<div class=\"md-cell\" data-align=\"{align}\">{}</div>",
                        inline_all(&cell.children)
                    );
                }
                html.push_str("</div>");
            }
            html.push_str("</div></div>");
        }
        Node::ThematicBreak(_) => html.push_str("<hr />"),
        Node::Html(raw) => html.push_str(&raw.value),
        other => html.push_str(&inline(other)),
    }
}

fn inline_all(nodes: &[Node]) -> String {
    nodes.iter().map(inline).collect()
}

fn inline(node: &Node) -> String {
    match node {
        Node::Text(text) => escape(&text.value),
        Node::Strong(strong) => format!("<strong>{}</strong>", inline_all(&strong.children)),
        Node::Emphasis(emphasis) => format!("<em>{}</em>", inline_all(&emphasis.children)),
        Node::Delete(delete) => format!("<s>{}</s>", inline_all(&delete.children)),
        Node::InlineCode(code) => format!(
            "<code data-penna=\"card.code\">{}</code>",
            escape(&code.value)
        ),
        Node::Break(_) => "<br />".into(),
        Node::Html(raw) => raw.value.clone(),
        Node::Link(link) => {
            let external = link.url.starts_with("http");
            let url = link
                .url
                .strip_suffix(".md")
                .map_or_else(|| link.url.clone(), file_name);
            format!(
                "<a href=\"{}\"{}>{}</a>",
                escape(&url),
                if external {
                    " target=\"_blank\" rel=\"noopener\""
                } else {
                    ""
                },
                inline_all(&link.children)
            )
        }
        other => other.children().map_or_else(
            || escape(&other.to_string()),
            |children| inline_all(children),
        ),
    }
}

fn code_block(lang: Option<&str>, source: &str) -> String {
    format!(
        "<div class=\"md-code\"><pre data-penna=\"card.code\" data-actions><code>{}</code>\
         <div class=\"md-actions\"><div><button class=\"penna-button\" data-penna=\"button.quiet\" data-shape=\"icon\" data-size=\"small\" aria-label=\"Copy\" data-tooltip=\"Copy\" data-copy><i data-icon=\"copy\"></i></button></div></div></pre></div>",
        highlight(lang.unwrap_or("text"), source)
    )
}

fn highlight(lang: &str, source: &str) -> String {
    let mut highlighter = SyntaxHighlighter::new(lang);
    highlighter.update(None, &Rope::from(source), None);
    let mut classes: Vec<Option<&str>> = vec![None; source.len()];
    for (range, name) in highlighter.highlights(0..source.len()) {
        let Some(class) = syntax_class(&name) else {
            continue;
        };
        for slot in &mut classes[range.start.min(source.len())..range.end.min(source.len())] {
            *slot = Some(class);
        }
    }
    let mut html = String::new();
    let mut run_start = 0;
    let mut boundaries: Vec<usize> = source.char_indices().map(|(index, _)| index).collect();
    boundaries.push(source.len());
    for window in boundaries.windows(2) {
        let (at, next) = (window[0], window[1]);
        if next == source.len() || classes[next] != classes[at] {
            push_run(&mut html, &source[run_start..next], classes[at]);
            run_start = next;
        }
    }
    html
}

fn push_run(html: &mut String, text: &str, class: Option<&str>) {
    match class {
        Some(class) => {
            let _ = write!(html, "<span class=\"tk-{class}\">{}</span>", escape(text));
        }
        None => html.push_str(&escape(text)),
    }
}

fn syntax_class(name: &str) -> Option<&'static str> {
    SYNTAX
        .iter()
        .find(|(_, capture)| *capture == name)
        .map(|(class, _)| *class)
}

fn slug(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c == ' ' || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}
