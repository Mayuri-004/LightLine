//! Markdown to a flat list of display blocks, for the rendered preview.
//!
//! This module only understands Markdown; it knows nothing about windows,
//! fonts or pixels. The Windows preview (`windows_app::markdown_view`) lays
//! these blocks out and paints them, and a future extension could replace
//! either half on its own. Nothing here runs embedded HTML or scripts: HTML
//! comes through as literal text.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::path::{Component, Path, PathBuf};

/// True for the files the preview is offered for: `.md`/`.markdown` (and a
/// few rarer spellings) and extensionless READMEs.
pub fn is_markdown_path(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => matches!(
            ext.to_ascii_lowercase().as_str(),
            "md" | "markdown" | "mdown" | "mkd" | "mkdn"
        ),
        None => path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("readme")),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
}

/// A run of text with one style, and the link it belongs to, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Bullet,
    Number(u64),
    Task(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Heading(u8, Vec<Inline>),
    Paragraph(Vec<Inline>),
    ListItem(Marker, Vec<Inline>),
    Code {
        language: String,
        text: String,
    },
    Table {
        align: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Image {
        url: String,
        alt: String,
        /// Display width in pixels, from an HTML `<img width="...">`.
        width: Option<u32>,
    },
    Rule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// List nesting depth: a top-level list item is 1, and a paragraph that
    /// continues an item shares its depth so it lines up under the text.
    pub indent: usize,
    /// Block quote nesting depth.
    pub quote: usize,
    /// Zero-based source line the block starts on, for scroll sync.
    pub line: usize,
}

/// Parses `source` into display blocks. Any input is accepted: malformed
/// Markdown just comes out as plain paragraphs.
pub fn parse(source: &str) -> Vec<Block> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut builder = Builder::new(source);
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        builder.event(event, range.start);
    }
    builder.flush();
    builder.blocks
}

struct TableState {
    align: Vec<Align>,
    header: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
}

struct Builder<'a> {
    source: &'a str,
    newlines: Vec<usize>,
    blocks: Vec<Block>,
    inlines: Vec<Inline>,
    start: usize,
    bold: usize,
    italic: usize,
    strike: usize,
    links: Vec<String>,
    // Each open list's next number; None for a bulleted list.
    lists: Vec<Option<u64>>,
    marker: Option<Marker>,
    quote: usize,
    heading: Option<u8>,
    code: Option<(String, String)>,
    html: Option<String>,
    image: Option<(String, String)>,
    table: Option<TableState>,
}

impl<'a> Builder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            newlines: source.match_indices('\n').map(|(at, _)| at).collect(),
            blocks: Vec::new(),
            inlines: Vec::new(),
            start: 0,
            bold: 0,
            italic: 0,
            strike: 0,
            links: Vec::new(),
            lists: Vec::new(),
            marker: None,
            quote: 0,
            heading: None,
            code: None,
            html: None,
            image: None,
            table: None,
        }
    }

    fn line_of(&self, offset: usize) -> usize {
        self.newlines.partition_point(|&at| at < offset)
    }

    fn push_block(&mut self, kind: BlockKind) {
        self.blocks.push(Block {
            kind,
            indent: self.lists.len(),
            quote: self.quote,
            line: self.line_of(self.start.min(self.source.len())),
        });
    }

    // Emits the text gathered so far: as the pending list item's text if an
    // item is open and hasn't shown its text yet, otherwise as a paragraph.
    fn flush(&mut self) {
        let inlines = std::mem::take(&mut self.inlines);
        if inlines.iter().all(|inline| inline.text.trim().is_empty()) {
            return;
        }
        let kind = match self.marker.take() {
            Some(marker) => BlockKind::ListItem(marker, inlines),
            None => BlockKind::Paragraph(inlines),
        };
        self.push_block(kind);
    }

    // An HTML tag inside a paragraph: `<br>` breaks the line and `<img>`
    // becomes an image; every other tag is dropped (its text still shows).
    fn inline_html(&mut self, html: &str) {
        let Some(tag) = html
            .trim()
            .strip_prefix('<')
            .and_then(|tag| tag.strip_suffix('>'))
        else {
            self.text(html, false);
            return;
        };
        match tag_name(tag).as_str() {
            "br" => self.text("\n", false),
            "img" if !tag.starts_with('/') => {
                if let Some(image) = html_image(tag) {
                    self.flush();
                    self.push_block(image);
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        if let Some((_, alt)) = &mut self.image {
            alt.push_str(text);
            return;
        }
        let style = Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            code,
            strike: self.strike > 0,
        };
        let link = self.links.last().cloned();
        match self.inlines.last_mut() {
            Some(last) if last.style == style && last.link == link => last.text.push_str(text),
            _ => self.inlines.push(Inline {
                text: text.to_string(),
                style,
                link,
            }),
        }
    }

    fn event(&mut self, event: Event, offset: usize) {
        match event {
            Event::Start(tag) => self.start_tag(tag, offset),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => {
                if let Some((_, code)) = &mut self.code {
                    code.push_str(&text);
                } else if let Some(html) = &mut self.html {
                    html.push_str(&text);
                } else {
                    if self.inlines.is_empty() && self.table.is_none() {
                        self.start = offset;
                    }
                    self.text(&text, false);
                }
            }
            Event::Code(text) => self.text(&text, true),
            Event::Html(html) => match &mut self.html {
                Some(block) => block.push_str(&html),
                None => self.inline_html(&html),
            },
            Event::InlineHtml(html) => self.inline_html(&html),
            Event::SoftBreak => self.text(" ", false),
            Event::HardBreak => self.text("\n", false),
            Event::Rule => {
                self.flush();
                self.start = offset;
                self.push_block(BlockKind::Rule);
            }
            Event::TaskListMarker(checked) => {
                if self.marker.is_some() {
                    self.marker = Some(Marker::Task(checked));
                }
            }
            Event::FootnoteReference(name) => self.text(&format!("[{name}]"), false),
            _ => {}
        }
    }

    fn start_tag(&mut self, tag: Tag, offset: usize) {
        match tag {
            Tag::Paragraph => {
                self.flush();
                self.start = offset;
            }
            Tag::Heading { level, .. } => {
                self.flush();
                self.start = offset;
                self.heading = Some(level as u8);
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote += 1;
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                self.start = offset;
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((language, String::new()));
            }
            Tag::HtmlBlock => {
                self.flush();
                self.start = offset;
                self.html = Some(String::new());
            }
            Tag::List(first) => {
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush();
                self.start = offset;
                self.marker = Some(match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = Marker::Number(*number);
                        *number += 1;
                        marker
                    }
                    _ => Marker::Bullet,
                });
            }
            Tag::Table(align) => {
                self.flush();
                self.start = offset;
                self.table = Some(TableState {
                    align: align
                        .iter()
                        .map(|align| match align {
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                            _ => Align::Left,
                        })
                        .collect(),
                    header: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                });
            }
            Tag::TableRow | Tag::TableHead => {
                if let Some(table) = &mut self.table {
                    table.row.clear();
                }
            }
            Tag::TableCell => self.inlines.clear(),
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { dest_url, .. } => self.links.push(dest_url.to_string()),
            Tag::Image { dest_url, .. } => self.image = Some((dest_url.to_string(), String::new())),
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(_) => {
                let inlines = std::mem::take(&mut self.inlines);
                let level = self.heading.take().unwrap_or(1);
                self.push_block(BlockKind::Heading(level, inlines));
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some((language, mut text)) = self.code.take() {
                    if text.ends_with('\n') {
                        text.pop();
                    }
                    self.push_block(BlockKind::Code { language, text });
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(html) = self.html.take() {
                    for kind in html_blocks(&html) {
                        self.push_block(kind);
                    }
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                self.marker = None;
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inlines);
                if let Some(table) = &mut self.table {
                    table.row.push(cell);
                }
            }
            TagEnd::TableHead => {
                if let Some(table) = &mut self.table {
                    table.header = std::mem::take(&mut table.row);
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = &mut self.table {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.push_block(BlockKind::Table {
                        align: table.align,
                        header: table.header,
                        rows: table.rows,
                    });
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                let Some((url, alt)) = self.image.take() else {
                    return;
                };
                if self.table.is_some() {
                    // A table cell can't hold a block; show the description.
                    self.text(&alt, false);
                } else {
                    // Images get their own block, between the text before
                    // and after them.
                    self.flush();
                    self.push_block(BlockKind::Image {
                        url,
                        alt,
                        width: None,
                    });
                }
            }
            _ => {}
        }
    }
}

// The lowercase element name of a tag's inside (`/div` and `img src=..`
// give `div` and `img`).
fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .split(|ch: char| ch.is_whitespace() || ch == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

// Appends HTML text the way a browser shows it: any run of whitespace,
// newlines included, is one space.
fn push_collapsed(text: &mut String, raw: &str) {
    let mut last_space = text.is_empty() || text.ends_with([' ', '\n']);
    for ch in raw.chars() {
        if ch.is_whitespace() {
            if !last_space {
                text.push(' ');
                last_space = true;
            }
        } else {
            text.push(ch);
            last_space = false;
        }
    }
}

// The value of attribute `name` in a tag's inside, quoted or bare.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = lower[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value_start = tag.len() - rest.len() + 1;
        let value = tag[value_start..].trim_start();
        return Some(match value.chars().next() {
            Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or("").to_string(),
            _ => value
                .split(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
                .next()
                .unwrap_or("")
                .to_string(),
        });
    }
    None
}

fn html_image(tag: &str) -> Option<BlockKind> {
    let url = attribute(tag, "src").filter(|src| !src.trim().is_empty())?;
    Some(BlockKind::Image {
        url: decode_entities(&url),
        alt: decode_entities(&attribute(tag, "alt").unwrap_or_default()),
        width: attribute(tag, "width").and_then(|width| width.trim_end_matches("px").parse().ok()),
    })
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

// Shows the safe part of an HTML block, the way READMEs use it for their
// headers: `<img>` becomes an image, headings and text are kept, `<br>` and
// `<hr>` break the flow, and layout tags (`<div>`, `<p>`, `<a>`...) are
// dropped. Script, style and embedded-content elements are removed with
// everything inside them. Nothing is ever run.
fn html_blocks(html: &str) -> Vec<BlockKind> {
    const REMOVED: [&str; 8] = [
        "script", "style", "iframe", "object", "embed", "template", "noscript", "svg",
    ];
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut heading: Option<u8> = None;
    let mut skipping: Option<String> = None;
    let flush = |text: &mut String, heading: &mut Option<u8>, blocks: &mut Vec<BlockKind>| {
        let content = decode_entities(text.trim());
        text.clear();
        if content.is_empty() {
            return;
        }
        let inlines = vec![Inline {
            text: content,
            style: Style::default(),
            link: None,
        }];
        blocks.push(match heading.take() {
            Some(level) => BlockKind::Heading(level, inlines),
            None => BlockKind::Paragraph(inlines),
        });
    };
    let mut rest = html;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            if skipping.is_none() {
                push_collapsed(&mut text, rest);
            }
            break;
        };
        if skipping.is_none() {
            push_collapsed(&mut text, &rest[..open]);
        }
        let after = &rest[open..];
        if after.starts_with("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        let Some(close) = after.find('>') else {
            // An unfinished tag: keep it as text rather than lose it.
            if skipping.is_none() {
                text.push_str(after);
            }
            break;
        };
        let tag = &after[1..close];
        rest = &after[close + 1..];
        let name = tag_name(tag);
        let closing = tag.starts_with('/');
        if let Some(element) = &skipping {
            if closing && name == *element {
                skipping = None;
            }
            continue;
        }
        match name.as_str() {
            element if REMOVED.contains(&element) => {
                if !closing && !tag.ends_with('/') {
                    skipping = Some(name);
                }
            }
            "img" if !closing => {
                flush(&mut text, &mut heading, &mut blocks);
                blocks.extend(html_image(tag));
            }
            "br" => text.push('\n'),
            "hr" => {
                flush(&mut text, &mut heading, &mut blocks);
                blocks.push(BlockKind::Rule);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                flush(&mut text, &mut heading, &mut blocks);
                if !closing {
                    heading = name[1..].parse().ok();
                }
            }
            "p" | "div" | "center" | "li" | "ul" | "ol" | "table" | "tr" | "section"
            | "picture" => {
                flush(&mut text, &mut heading, &mut blocks);
            }
            _ => {}
        }
    }
    flush(&mut text, &mut heading, &mut blocks);
    blocks
}

/// The anchor GitHub gives a heading, so `[see](#install-steps)` finds it:
/// lowercase, punctuation dropped, spaces as hyphens.
pub fn heading_slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|ch| match ch {
            ' ' => Some('-'),
            '-' | '_' => Some(ch),
            _ if ch.is_alphanumeric() => Some(ch),
            _ => None,
        })
        .collect()
}

/// Where following a link leads.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkTarget {
    /// An `http`, `https` or `mailto` address, for the default browser.
    Web(String),
    /// A heading in the same document (`#anchor`).
    Anchor(String),
    /// A file inside the allowed folder.
    File(PathBuf),
    /// Anything else: other schemes (`javascript:`, `file:`), absolute paths,
    /// and relative paths that climb out of the allowed folder.
    Refused,
}

/// Resolves a link or image address from a Markdown file in `document_dir`.
/// Relative paths may only reach files inside `root` (the workspace, or the
/// document's own folder when there is none).
pub fn resolve_link(url: &str, document_dir: &Path, root: &Path) -> LinkTarget {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return LinkTarget::Web(url.to_string());
    }
    if let Some(anchor) = url.strip_prefix('#') {
        return LinkTarget::Anchor(percent_decode(anchor));
    }
    // A scheme (`javascript:`, `file:`), a drive letter or a rooted path.
    if url.is_empty() || url.contains(':') || url.starts_with(['/', '\\']) {
        return LinkTarget::Refused;
    }
    let relative = url.split(['#', '?']).next().unwrap_or("");
    let mut path = document_dir.to_path_buf();
    for part in percent_decode(relative).split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if !path.pop() {
                    return LinkTarget::Refused;
                }
            }
            part => path.push(part),
        }
    }
    let inside = |path: &Path| {
        let root_parts = root
            .components()
            .filter(|c| !matches!(c, Component::CurDir));
        let mut parts = path.components();
        root_parts.into_iter().all(|part| {
            parts
                .next()
                .is_some_and(|other| same_component(part, other))
        })
    };
    if !inside(&path) {
        return LinkTarget::Refused;
    }
    // A link or junction inside the folder could still point outside it.
    if let (Ok(real), Ok(real_root)) = (std::fs::canonicalize(&path), std::fs::canonicalize(root))
        && !real.starts_with(&real_root)
    {
        return LinkTarget::Refused;
    }
    LinkTarget::File(path)
}

// Windows compares path names without regard to case.
fn same_component(a: Component, b: Component) -> bool {
    a.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(value) = text
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(inlines: &[Inline]) -> String {
        inlines.iter().map(|inline| inline.text.as_str()).collect()
    }

    #[test]
    fn recognizes_markdown_files_and_extensionless_readmes() {
        assert!(is_markdown_path(Path::new("docs/Guide.MD")));
        assert!(is_markdown_path(Path::new("notes.markdown")));
        assert!(is_markdown_path(Path::new("README")));
        assert!(!is_markdown_path(Path::new("README.txt")));
        assert!(!is_markdown_path(Path::new("main.rs")));
    }

    #[test]
    fn headings_paragraphs_and_inline_styles() {
        let blocks =
            parse("# Title\n\nSome **bold** and *italic* `code` with [a link](https://x.y).\n");
        assert_eq!(blocks.len(), 2);
        let BlockKind::Heading(1, title) = &blocks[0].kind else {
            panic!("expected a heading: {:?}", blocks[0]);
        };
        assert_eq!(text_of(title), "Title");
        let BlockKind::Paragraph(inlines) = &blocks[1].kind else {
            panic!("expected a paragraph");
        };
        assert!(inlines.iter().any(|i| i.text == "bold" && i.style.bold));
        assert!(inlines.iter().any(|i| i.text == "italic" && i.style.italic));
        assert!(inlines.iter().any(|i| i.text == "code" && i.style.code));
        assert!(
            inlines
                .iter()
                .any(|i| i.text == "a link" && i.link.as_deref() == Some("https://x.y"))
        );
        assert_eq!(blocks[1].line, 2);
    }

    #[test]
    fn lists_number_nest_and_carry_task_state() {
        let blocks = parse("3. three\n4. four\n   - nested\n- [x] done\n- [ ] todo\n");
        let items: Vec<(Marker, String, usize)> = blocks
            .iter()
            .filter_map(|block| match &block.kind {
                BlockKind::ListItem(marker, inlines) => {
                    Some((*marker, text_of(inlines), block.indent))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            vec![
                (Marker::Number(3), "three".into(), 1),
                (Marker::Number(4), "four".into(), 1),
                (Marker::Bullet, "nested".into(), 2),
                (Marker::Task(true), "done".into(), 1),
                (Marker::Task(false), "todo".into(), 1),
            ]
        );
    }

    #[test]
    fn code_blocks_tables_quotes_rules_and_images() {
        let source = "```rust\nfn main() {}\n```\n\n| A | B |\n|:--|--:|\n| 1 | 2 |\n\n> quoted\n\n---\n\n![logo](img/logo.png)\n";
        let blocks = parse(source);
        assert_eq!(
            blocks[0].kind,
            BlockKind::Code {
                language: "rust".into(),
                text: "fn main() {}".into()
            }
        );
        let BlockKind::Table {
            align,
            header,
            rows,
        } = &blocks[1].kind
        else {
            panic!("expected a table: {:?}", blocks[1]);
        };
        assert_eq!(align, &vec![Align::Left, Align::Right]);
        assert_eq!(text_of(&header[1]), "B");
        assert_eq!(text_of(&rows[0][0]), "1");
        assert_eq!(blocks[2].quote, 1);
        assert_eq!(blocks[3].kind, BlockKind::Rule);
        assert_eq!(
            blocks[4].kind,
            BlockKind::Image {
                url: "img/logo.png".into(),
                alt: "logo".into(),
                width: None
            }
        );
    }

    #[test]
    fn html_shows_images_and_text_but_never_runs_anything() {
        let source = concat!(
            "<div align=\"center\">\n",
            "<img src=\"logo.png\" alt=\"Logo\" width=\"96\">\n",
            "<h1>Title</h1>\n<script>alert(1)</script>\n",
            "<p>Some\ntext &amp; more<br>next</p>\n</div>\n\n",
            "Text <b>inline</b><br>after\n",
        );
        let blocks = parse(source);
        let kinds: Vec<&BlockKind> = blocks.iter().map(|block| &block.kind).collect();
        assert_eq!(
            kinds[0],
            &BlockKind::Image {
                url: "logo.png".into(),
                alt: "Logo".into(),
                width: Some(96)
            }
        );
        let BlockKind::Heading(1, title) = kinds[1] else {
            panic!("expected a heading: {kinds:?}");
        };
        assert_eq!(text_of(title), "Title");
        // The script and everything in it is gone.
        let BlockKind::Paragraph(text) = kinds[2] else {
            panic!("expected a paragraph: {kinds:?}");
        };
        assert_eq!(text_of(text), "Some text & more\nnext");
        assert!(!format!("{kinds:?}").contains("alert"));
        // Inline tags are dropped and <br> breaks the line.
        let BlockKind::Paragraph(inline) = kinds[3] else {
            panic!("expected a paragraph: {kinds:?}");
        };
        assert_eq!(text_of(inline), "Text inline\nafter");
    }

    #[test]
    fn malformed_input_still_parses() {
        for source in [
            "",
            "**unclosed",
            "|a|\n|-",
            "```\nno end",
            "- \n-",
            "[x](",
            "\u{0}\u{feff}",
        ] {
            parse(source);
        }
    }

    #[test]
    fn heading_slugs_match_github_anchors() {
        assert_eq!(
            heading_slug("Download & Quick Start"),
            "download--quick-start"
        );
        assert_eq!(heading_slug("User Settings"), "user-settings");
    }

    #[test]
    fn links_resolve_only_inside_the_allowed_folder() {
        let root = Path::new(r"C:\work\repo");
        let docs = Path::new(r"C:\work\repo\docs");
        assert_eq!(
            resolve_link("https://github.com", docs, root),
            LinkTarget::Web("https://github.com".into())
        );
        assert_eq!(
            resolve_link("#user-settings", docs, root),
            LinkTarget::Anchor("user-settings".into())
        );
        assert_eq!(
            resolve_link("../README.md#top", docs, root),
            LinkTarget::File(PathBuf::from(r"C:\work\repo\README.md"))
        );
        assert_eq!(
            resolve_link("img/a%20b.png", docs, root),
            LinkTarget::File(PathBuf::from(r"C:\work\repo\docs\img\a b.png"))
        );
        for refused in [
            "../../secret.txt",
            "javascript:alert(1)",
            "file:///C:/Windows/win.ini",
            r"C:\Windows\win.ini",
            "/etc/passwd",
            r"\\server\share\x",
        ] {
            assert_eq!(
                resolve_link(refused, docs, root),
                LinkTarget::Refused,
                "{refused}"
            );
        }
    }
}
