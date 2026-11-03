use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    env,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

use markdown::{
    ParseOptions,
    mdast::{AlignKind, Code, Node},
};
use penna::foundation::{
    highlighter::{Rope, SyntaxHighlighter},
    theme_css::{self, SYNTAX},
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
    for style in styles {
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
    for script in scripts {
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
         <a class=\"penna-docs-brand\" href=\"index.html\"><i data-icon=\"app-window\"></i><span data-penna-text>{}</span></a>\
         {links}<div class=\"penna-docs-spacer\"></div>\
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
            "<div class=\"penna-docs-example\" data-penna=\"card.figure\" role=\"figure\">\
             {targets}{preview}{switch}<div class=\"penna-docs-code\">{panes}</div></div>\n"
        ))
    }

    fn figure(&self, code: &Code) -> io::Result<String> {
        meta(code)
            .into_iter()
            .filter_map(|token| token.strip_prefix("uses="))
            .flat_map(|list| list.split(','))
            .for_each(|name| self.uses(name));
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
            "<div class=\"penna-docs-example docs-figure\" data-penna=\"card.figure\" role=\"figure\">{body}</div>\n"
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
