//! The logo SVG sanitizer (SPEC §7.3): an allowlist re-serializer. The
//! input is tokenized, and only allowlisted elements and attributes are
//! kept, as parsed values; the output is written fresh and escaped, so no
//! input text reaches the SVG unchecked.
//!
//! Dropped: scripts, event handlers, `foreignObject`, animation, links,
//! patterns, DOCTYPEs and custom entities, `class`, namespaced attributes,
//! and any reference that isn't to an ID inside the logo. `<style>`
//! elements are read for simple `.class { … }` rules, which are folded into
//! attributes; the `style` attribute is folded the same way.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::num::write_scaled;

/// A clean logo document: its viewBox and its content.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SvgDoc {
    /// `min-x min-y width height`, in thousandths.
    pub(crate) view_box: [i64; 4],
    nodes: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Open {
        tag: &'static str,
        attrs: Vec<(&'static str, Value)>,
    },
    /// Closes the most recent open element.
    Close,
    Text(String),
}

/// An attribute value: text, with references to logo IDs kept apart so
/// they can be prefixed on output.
#[derive(Clone, Debug, PartialEq)]
struct Value(Vec<Seg>);

#[derive(Clone, Debug, PartialEq)]
enum Seg {
    Text(String),
    Id(String),
}

/// Why a logo was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SvgProblem {
    /// The document has no `<svg>` root element.
    NotSvg,
    /// The root has neither a viewBox nor a width and height.
    NoSize,
}

/// Elements kept, as written in SVG (case matters).
const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "symbol",
    "use",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "mask",
    "text",
    "tspan",
    "image",
];

/// Presentation properties, accepted as attributes, in `style` and in
/// class rules.
const PROPERTIES: &[&str] = &[
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "stop-color",
    "stop-opacity",
    "clip-path",
    "clip-rule",
    "mask",
    "display",
    "visibility",
    "color",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "text-anchor",
    "dominant-baseline",
    "letter-spacing",
];

/// Other attributes, accepted only as attributes.
const ATTRIBUTES: &[&str] = &[
    "id",
    "x",
    "y",
    "width",
    "height",
    "rx",
    "ry",
    "cx",
    "cy",
    "r",
    "fx",
    "fy",
    "fr",
    "x1",
    "y1",
    "x2",
    "y2",
    "dx",
    "dy",
    "points",
    "d",
    "transform",
    "viewBox",
    "preserveAspectRatio",
    "offset",
    "gradientUnits",
    "gradientTransform",
    "spreadMethod",
    "clipPathUnits",
    "maskUnits",
    "maskContentUnits",
    "href",
];

/// Element names the text of which is kept.
const TEXT_ELEMENTS: &[&str] = &["text", "tspan"];

/// How many `use` elements a logo may have.
const MAX_USES: usize = 256;

fn known(list: &[&'static str], name: &str) -> Option<&'static str> {
    list.iter().copied().find(|&n| n == name)
}

/// A raw token from the input.
enum Token<'a> {
    Open {
        name: &'a str,
        attrs: Vec<(&'a str, String)>,
        self_closing: bool,
    },
    Close(&'a str),
    /// Decoded character data (text or CDATA).
    Text(String),
}

/// Tokenizes XML-ish input leniently: anything malformed ends the token
/// it's in, and the rest is skipped up to the next `>`.
fn tokenize(input: &str) -> Vec<Token<'_>> {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let find = |from: usize, pat: &str| input[from..].find(pat).map(|p| from + p);
    while i < b.len() {
        if b[i] != b'<' {
            let end = find(i, "<").unwrap_or(b.len());
            if let Some(text) = decode(&input[i..end]) {
                out.push(Token::Text(text));
            }
            i = end;
            continue;
        }
        let rest = &input[i..];
        if rest.starts_with("<!--") {
            i = find(i + 4, "-->").map_or(b.len(), |e| e + 3);
        } else if rest.starts_with("<![CDATA[") {
            let end = find(i + 9, "]]>").unwrap_or(b.len());
            let text = input[i + 9..end].chars().filter(|&c| xml_char(c)).collect();
            out.push(Token::Text(text));
            i = (end + 3).min(b.len());
        } else if rest.starts_with("<?") {
            i = find(i + 2, "?>").map_or(b.len(), |e| e + 2);
        } else if rest.starts_with("<!") {
            // DOCTYPE and friends, with any internal subset in brackets.
            let mut depth = 0i32;
            let mut j = i + 2;
            while j < b.len() {
                match b[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    b'>' if depth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
        } else if rest.starts_with("</") {
            let end = find(i, ">").unwrap_or(b.len());
            out.push(Token::Close(input[i + 2..end].trim()));
            i = end + 1;
        } else {
            i = open_tag(input, i, &mut out);
        }
    }
    out
}

/// Parses `<name attr="v" …>` at `i`; returns the index after it.
fn open_tag<'a>(input: &'a str, i: usize, out: &mut Vec<Token<'a>>) -> usize {
    let b = input.as_bytes();
    let name_char = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'-' | b'.');
    let skip_to_end = |from: usize| input[from..].find('>').map_or(b.len(), |p| from + p + 1);
    let mut j = i + 1;
    while j < b.len() && name_char(b[j]) {
        j += 1;
    }
    let name = &input[i + 1..j];
    if name.is_empty() {
        // A stray `<`: treat it as text and move on.
        return i + 1;
    }
    let mut attrs = Vec::new();
    loop {
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() {
            return b.len();
        }
        match b[j] {
            b'>' => {
                out.push(Token::Open {
                    name,
                    attrs,
                    self_closing: false,
                });
                return j + 1;
            }
            b'/' if b.get(j + 1) == Some(&b'>') => {
                out.push(Token::Open {
                    name,
                    attrs,
                    self_closing: true,
                });
                return j + 2;
            }
            c if name_char(c) => {
                let start = j;
                while j < b.len() && name_char(b[j]) {
                    j += 1;
                }
                let attr = &input[start..j];
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                if b.get(j) != Some(&b'=') {
                    return skip_to_end(j);
                }
                j += 1;
                while j < b.len() && b[j].is_ascii_whitespace() {
                    j += 1;
                }
                let Some(&quote) = b.get(j).filter(|&&q| q == b'"' || q == b'\'') else {
                    return skip_to_end(j);
                };
                let Some(len) = input[j + 1..].find(quote as char) else {
                    return b.len();
                };
                let raw = &input[j + 1..j + 1 + len];
                j += len + 2;
                // A value with an unknown entity is dropped.
                if let Some(value) = decode(raw) {
                    attrs.push((attr, value));
                }
            }
            _ => return skip_to_end(j),
        }
    }
}

/// Characters XML 1.0 allows.
fn xml_char(c: char) -> bool {
    (c as u32) >= 0x20 || matches!(c, '\t' | '\n' | '\r')
}

/// Decodes the predefined and numeric character references, dropping
/// characters XML can't carry. `None` when there's another entity, which
/// only a DOCTYPE could define.
fn decode(raw: &str) -> Option<String> {
    let decoded = decode_references(raw)?;
    Some(decoded.chars().filter(|&c| xml_char(c)).collect())
}

fn decode_references(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after.find(';')?;
        let entity = &after[..semi];
        let c = match entity {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = entity.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()?
                } else {
                    entity.strip_prefix('#')?.parse().ok()?
                };
                char::from_u32(code)?
            }
        };
        out.push(c);
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Simple class rules from `<style>` elements: `(class, declarations)` in
/// document order.
fn class_rules(tokens: &[Token<'_>]) -> Vec<(String, Vec<(&'static str, String)>)> {
    let mut css = String::new();
    let mut in_style = 0;
    for t in tokens {
        match t {
            Token::Open {
                name, self_closing, ..
            } if *name == "style" && !self_closing => in_style += 1,
            Token::Close(name) if *name == "style" && in_style > 0 => in_style -= 1,
            Token::Text(text) if in_style > 0 => css.push_str(text),
            _ => {}
        }
    }
    let css = strip_comments(&css);
    let mut rules = Vec::new();
    for block in css.split('}') {
        let Some((selectors, body)) = block.split_once('{') else {
            continue;
        };
        let decls = declarations(body);
        for selector in selectors.split(',') {
            let selector = selector.trim();
            if let Some(class) = selector.strip_prefix('.')
                && !class.is_empty()
                && class
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                rules.push((String::from(class), decls.clone()));
            }
        }
    }
    rules
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start + 2..]
            .find("*/")
            .map_or("", |e| &rest[start + 2 + e + 2..]);
    }
    out.push_str(rest);
    out
}

/// `prop: value; …`, keeping allowlisted properties.
fn declarations(body: &str) -> Vec<(&'static str, String)> {
    body.split(';')
        .filter_map(|decl| {
            let (prop, value) = decl.split_once(':')?;
            let prop = known(PROPERTIES, prop.trim())?;
            let value = value.trim().trim_end_matches("!important").trim();
            Some((prop, String::from(value)))
        })
        .collect()
}

/// Keeps ID characters that need no escaping anywhere.
fn clean_id(raw: &str) -> Option<String> {
    let id: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Parses a value, turning `url(#id)` into references. `None` drops the
/// attribute: a `url()` to anything else, a CSS escape, or a script URL.
fn parse_value(raw: &str) -> Option<Value> {
    let lower = raw.to_ascii_lowercase();
    if lower.contains('\\') || lower.contains("javascript:") || lower.contains("expression(") {
        return None;
    }
    let mut segs = Vec::new();
    let mut rest = raw;
    while let Some(at) = rest.to_ascii_lowercase().find("url(") {
        if at > 0 {
            segs.push(Seg::Text(String::from(&rest[..at])));
        }
        let inner_start = at + 4;
        let close = rest[inner_start..].find(')')? + inner_start;
        let inner = rest[inner_start..close]
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .trim();
        let id = clean_id(inner.strip_prefix('#')?)?;
        segs.push(Seg::Text(String::from("url(#")));
        segs.push(Seg::Id(id));
        segs.push(Seg::Text(String::from(")")));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        segs.push(Seg::Text(String::from(rest)));
    }
    Some(Value(segs))
}

/// `href`: `#id`, or on `image` a base64 raster data URI.
fn parse_href(tag: &str, raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if let Some(id) = raw.strip_prefix('#') {
        return Some(Value(alloc::vec![
            Seg::Text(String::from("#")),
            Seg::Id(clean_id(id)?)
        ]));
    }
    if tag != "image" {
        return None;
    }
    let data = ["png", "jpeg", "gif", "webp"].iter().find_map(|kind| {
        raw.strip_prefix("data:image/")?
            .strip_prefix(kind)?
            .strip_prefix(";base64,")
            .map(|d| (kind, d))
    });
    let (kind, data) = data?;
    let clean: String = data.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if clean.is_empty()
        || !clean
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
    {
        return None;
    }
    Some(Value(alloc::vec![Seg::Text(alloc::format!(
        "data:image/{kind};base64,{clean}"
    ))]))
}

/// The attributes kept on an element: its own, then class rules, then its
/// `style` attribute, later ones winning.
fn element_attrs(
    tag: &'static str,
    raw: &[(&str, String)],
    rules: &[(String, Vec<(&'static str, String)>)],
) -> Vec<(&'static str, Value)> {
    let mut merged: Vec<(&'static str, String)> = Vec::new();
    let mut set =
        |name: &'static str, value: &str| match merged.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = String::from(value),
            None => merged.push((name, String::from(value))),
        };
    let mut classes = "";
    let mut style = "";
    for (name, value) in raw {
        match *name {
            "class" => classes = value,
            "style" => style = value,
            // SVG 1.1 links: same meaning as `href`.
            "xlink:href" => set("href", value),
            n => {
                if let Some(n) = known(ATTRIBUTES, n).or_else(|| known(PROPERTIES, n)) {
                    set(n, value);
                }
            }
        }
    }
    for (class, decls) in rules {
        if classes.split_ascii_whitespace().any(|c| c == class) {
            for (prop, value) in decls {
                set(prop, value);
            }
        }
    }
    for (prop, value) in declarations(style) {
        set(prop, &value);
    }
    merged
        .into_iter()
        .filter_map(|(name, raw)| {
            let value = match name {
                "id" => Value(alloc::vec![Seg::Id(clean_id(&raw)?)]),
                "href" => parse_href(tag, &raw)?,
                _ => parse_value(&raw)?,
            };
            Some((name, value))
        })
        .collect()
}

/// A number with an optional `px` unit.
fn length(raw: &str) -> Option<f64> {
    let t = raw.trim();
    let t = t.strip_suffix("px").unwrap_or(t).trim();
    let v: f64 = t.parse().ok()?;
    (v.is_finite() && v > 0.0).then_some(v)
}

fn view_box(raw: &[(&str, String)]) -> Option<[i64; 4]> {
    let get = |n: &str| raw.iter().find(|(k, _)| *k == n).map(|(_, v)| v.as_str());
    let milli = |v: f64| crate::math::round(v * 1000.0) as i64;
    if let Some(vb) = get("viewBox") {
        let nums: Vec<f64> = vb
            .split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if let [x, y, w, h] = nums[..]
            && [x, y, w, h].iter().all(|v| v.is_finite() && v.abs() < 1e9)
            && w > 0.0
            && h > 0.0
            && milli(w) > 0
            && milli(h) > 0
        {
            return Some([milli(x), milli(y), milli(w), milli(h)]);
        }
    }
    let (w, h) = (length(get("width")?)?, length(get("height")?)?);
    (w < 1e9 && h < 1e9 && milli(w) > 0 && milli(h) > 0).then(|| [0, 0, milli(w), milli(h)])
}

/// Sanitizes a logo document.
pub(crate) fn sanitize(input: &str) -> Result<SvgDoc, SvgProblem> {
    let tokens = tokenize(input);
    let rules = class_rules(&tokens);
    let mut nodes = Vec::new();
    let mut root = None;
    // Open elements kept, and the depth inside a dropped one.
    let mut stack: Vec<&'static str> = Vec::new();
    let mut skip = 0usize;
    let mut done = false;
    for token in &tokens {
        if done {
            break;
        }
        match token {
            Token::Open {
                name,
                attrs,
                self_closing,
            } => {
                if skip > 0 {
                    skip += !self_closing as usize;
                    continue;
                }
                if root.is_none() {
                    if *name != "svg" {
                        return Err(SvgProblem::NotSvg);
                    }
                    root = Some(view_box(attrs).ok_or(SvgProblem::NoSize)?);
                    // The root's presentation attributes carry over on a
                    // group; its geometry became the viewBox.
                    let attrs = element_attrs("g", attrs, &rules)
                        .into_iter()
                        .filter(|(n, _)| PROPERTIES.contains(n))
                        .collect();
                    nodes.push(Node::Open { tag: "g", attrs });
                    stack.push("svg");
                    if *self_closing {
                        nodes.push(Node::Close);
                        done = true;
                    }
                    continue;
                }
                let Some(tag) = known(ELEMENTS, name) else {
                    skip = !self_closing as usize;
                    continue;
                };
                nodes.push(Node::Open {
                    tag,
                    attrs: element_attrs(tag, attrs, &rules),
                });
                if *self_closing {
                    nodes.push(Node::Close);
                } else {
                    stack.push(tag);
                }
            }
            Token::Close(name) => {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                if let Some(at) = stack.iter().rposition(|t| t == name) {
                    for _ in at..stack.len() {
                        nodes.push(Node::Close);
                    }
                    stack.truncate(at);
                    done = stack.is_empty();
                }
            }
            Token::Text(text) => {
                if skip == 0
                    && stack.last().is_some_and(|t| TEXT_ELEMENTS.contains(t))
                    && !text.is_empty()
                {
                    nodes.push(Node::Text(text.clone()));
                }
            }
        }
    }
    let view_box = root.ok_or(SvgProblem::NotSvg)?;
    for _ in 0..stack.len() {
        nodes.push(Node::Close);
    }
    // The root's group only matters when it carries paint; its Close is
    // the last node.
    if matches!(nodes.first(), Some(Node::Open { attrs, .. }) if attrs.is_empty()) {
        nodes.remove(0);
        nodes.pop();
    }
    limit_uses(&mut nodes);
    Ok(SvgDoc { view_box, nodes })
}

/// Drops `use` elements that point at something containing a `use` (which
/// could expand exponentially), and any beyond [`MAX_USES`].
fn limit_uses(nodes: &mut Vec<Node>) {
    // IDs of elements with a `use` inside them (or that are one).
    let mut open: Vec<(Option<String>, bool)> = Vec::new();
    let mut nesting = Vec::new();
    for node in nodes.iter() {
        match node {
            Node::Open { tag, attrs } => {
                let id = attrs.iter().find_map(|(n, v)| match (*n, &v.0[..]) {
                    ("id", [Seg::Id(id)]) => Some(id.clone()),
                    _ => None,
                });
                let is_use = *tag == "use";
                if is_use {
                    open.iter_mut().for_each(|(_, has)| *has = true);
                }
                open.push((id, is_use));
            }
            Node::Close => {
                if let Some((Some(id), true)) = open.pop() {
                    nesting.push(id);
                }
            }
            Node::Text(_) => {}
        }
    }
    let target = |attrs: &[(&str, Value)]| {
        attrs.iter().find_map(|(n, v)| match (*n, &v.0[..]) {
            ("href", [_, Seg::Id(id)]) => Some(id.clone()),
            _ => None,
        })
    };
    let mut kept = Vec::with_capacity(nodes.len());
    let mut uses = 0;
    let mut dropping = 0usize;
    for node in nodes.drain(..) {
        if dropping > 0 {
            match node {
                Node::Open { .. } => dropping += 1,
                Node::Close => dropping -= 1,
                Node::Text(_) => {}
            }
            continue;
        }
        if let Node::Open { tag: "use", attrs } = &node {
            let bad = target(attrs).is_none_or(|id| nesting.contains(&id));
            if bad || uses >= MAX_USES {
                dropping = 1;
                continue;
            }
            uses += 1;
        }
        kept.push(node);
    }
    *nodes = kept;
}

impl SvgDoc {
    /// Writes the content, with IDs and references prefixed by
    /// `{prefix}-logo-` (unchanged without a prefix).
    pub(crate) fn write_body<W: Write + ?Sized>(
        &self,
        w: &mut W,
        prefix: Option<&str>,
    ) -> fmt::Result {
        let mut tags: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < self.nodes.len() {
            match &self.nodes[i] {
                Node::Open { tag, attrs } => {
                    write!(w, "<{tag}")?;
                    for (name, value) in attrs {
                        write!(w, " {name}=\"")?;
                        for seg in &value.0 {
                            match seg {
                                Seg::Text(t) => escape(w, t)?,
                                Seg::Id(id) => match prefix {
                                    Some(p) => write!(w, "{p}-logo-{id}")?,
                                    None => w.write_str(id)?,
                                },
                            }
                        }
                        w.write_char('"')?;
                    }
                    // An element with nothing inside is written self-closed.
                    if matches!(self.nodes.get(i + 1), Some(Node::Close)) {
                        w.write_str("/>")?;
                        i += 2;
                        continue;
                    }
                    w.write_char('>')?;
                    tags.push(tag);
                }
                Node::Close => {
                    if let Some(tag) = tags.pop() {
                        write!(w, "</{tag}>")?;
                    }
                }
                Node::Text(text) => escape(w, text)?,
            }
            i += 1;
        }
        Ok(())
    }

    /// The solid colors the document paints with (`fill`, `stroke`,
    /// `stop-color`), in document order, each once.
    pub(crate) fn paints(&self) -> Vec<crate::color::Color> {
        let mut out = Vec::new();
        for node in &self.nodes {
            let Node::Open { attrs, .. } = node else {
                continue;
            };
            for (name, value) in attrs {
                if !matches!(*name, "fill" | "stroke" | "stop-color") {
                    continue;
                }
                let [Seg::Text(text)] = &value.0[..] else {
                    continue;
                };
                if let Ok(color) = crate::color::Color::parse(text)
                    && !out.contains(&color)
                {
                    out.push(color);
                }
            }
        }
        out
    }

    /// The viewBox attribute value.
    pub(crate) fn write_view_box<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        for (i, &v) in self.view_box.iter().enumerate() {
            if i > 0 {
                w.write_char(' ')?;
            }
            write_scaled(w, v, 3)?;
        }
        Ok(())
    }

    /// A standalone document of the clean logo.
    pub(crate) fn to_document(&self) -> String {
        let mut out = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"");
        let _ = self.write_view_box(&mut out);
        out.push_str("\">");
        let _ = self.write_body(&mut out, None);
        out.push_str("</svg>");
        out
    }
}

/// XML-escapes text for element content and double-quoted attributes,
/// dropping characters XML 1.0 forbids.
fn escape<W: Write + ?Sized>(w: &mut W, text: &str) -> fmt::Result {
    for c in text.chars() {
        match c {
            '&' => w.write_str("&amp;")?,
            '<' => w.write_str("&lt;")?,
            '>' => w.write_str("&gt;")?,
            '"' => w.write_str("&quot;")?,
            '\'' => w.write_str("&apos;")?,
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => w.write_char(c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(input: &str) -> String {
        sanitize(input).unwrap().to_document()
    }

    #[test]
    fn keeps_shapes_and_presentation() {
        let out = clean(
            r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="24px" height="24" fill="#123"><!-- hi --><path d="M0 0h24v24z" fill-rule="evenodd"/><circle cx="12" cy="12" r="4"></circle></svg>"##,
        );
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="#123"><path d="M0 0h24v24z" fill-rule="evenodd"/><circle cx="12" cy="12" r="4"/></g></svg>"##
        );
    }

    #[test]
    fn drops_scripts_handlers_and_foreign_content() {
        let out = clean(
            r#"<svg viewBox="0 0 10 10" onload="alert(1)"><script>alert(2)</script><foreignObject><div>x</div></foreignObject><rect width="1" height="1" onclick="x()" class="a"/><a href="https://evil"><rect width="2" height="2"/></a><animate attributeName="x"/><set to="1"/></svg>"#,
        );
        assert_eq!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="1" height="1"/></svg>"#
        );
    }

    #[test]
    fn references_stay_inside_the_logo() {
        let out = clean(
            r##"<svg viewBox="0 0 10 10"><defs><linearGradient id="g 1"><stop offset="0" stop-color="red"/></linearGradient></defs><rect fill="url( '#g 1' )" width="5" height="5"/><rect fill="url(https://x/y.svg#a)" width="1" height="1"/><use href="#g1"/><use xlink:href="https://evil/#a"/><image href="https://evil/x.png"/><image href="data:image/png;base64,iVBO Rw=="/></svg>"##,
        );
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><linearGradient id="g1"><stop offset="0" stop-color="red"/></linearGradient></defs><rect fill="url(#g1)" width="5" height="5"/><rect width="1" height="1"/><use href="#g1"/><image/><image href="data:image/png;base64,iVBORw=="/></svg>"##
        );
    }

    #[test]
    fn class_rules_and_style_attributes_become_attributes() {
        let out = clean(
            r##"<svg viewBox="0 0 4 4"><style><![CDATA[ /* c */ .a, .b { fill: #f00; stroke: blue } .a{fill:#0f0} svg .c{fill:red} .x{behavior:url(x.htc)} ]]></style><rect class="a" fill="#000" style="stroke:#00f;position:absolute" width="1" height="1"/><rect class="b c" width="1" height="1"/></svg>"##,
        );
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect fill="#0f0" width="1" height="1" stroke="#00f"/><rect width="1" height="1" fill="#f00" stroke="blue"/></svg>"##
        );
    }

    #[test]
    fn entities_doctypes_and_escapes() {
        let out = clean(
            r#"<!DOCTYPE svg [<!ENTITY e "boom">]><svg viewBox="0 0 4 4"><text x="1">a &lt;b&gt; &#x41;&#66; &e;</text><text font-family="A&amp;B">x</text><rect fill="u\72l(x)" width="1" height="1"/></svg>"#,
        );
        assert_eq!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><text x="1"/><text font-family="A&amp;B">x</text><rect width="1" height="1"/></svg>"#
        );
        let out = clean(r#"<svg viewBox="0 0 4 4"><text>a &lt;b&gt; &#x41;&#66;</text></svg>"#);
        assert!(out.contains("<text>a &lt;b&gt; AB</text>"), "{out}");
    }

    #[test]
    fn text_of_forbidden_characters_is_no_text() {
        // Found by fuzzing: an empty text node broke idempotence.
        let once = clean("<svg viewBox=\"0 0 1 1\"><text>\u{1}\0</text></svg>");
        assert!(once.contains("<text/>"), "{once}");
        assert_eq!(clean(&once), once);
    }

    #[test]
    fn recursive_uses_are_dropped() {
        let out = clean(
            r##"<svg viewBox="0 0 4 4"><defs><g id="a"><rect width="1" height="1"/></g><g id="b"><use href="#a"/><use href="#a"/></g></defs><use href="#b"/><use href="#a"/></svg>"##,
        );
        assert_eq!(out.matches("<use").count(), 3, "{out}");
        assert!(!out.contains("<use href=\"#b\""), "{out}");
    }

    #[test]
    fn rejects_documents_that_are_not_svgs() {
        assert_eq!(sanitize("<html></html>"), Err(SvgProblem::NotSvg));
        assert_eq!(sanitize("just text"), Err(SvgProblem::NotSvg));
        assert_eq!(sanitize("<svg><rect/></svg>"), Err(SvgProblem::NoSize));
        assert_eq!(
            sanitize(r#"<svg viewBox="0 0 0 4"/>"#),
            Err(SvgProblem::NoSize)
        );
    }

    #[test]
    fn sanitizing_is_idempotent() {
        let inputs = [
            r##"<svg viewBox="0 0 10 10"><defs><clipPath id="c"><circle r="3"/></clipPath></defs><g clip-path="url(#c)"><text>&amp;&lt;</text></g></svg>"##,
            r#"<svg width="3" height="5"><rect width="1" height="1" style="fill:red"/></svg>"#,
        ];
        for input in inputs {
            let once = clean(input);
            assert_eq!(clean(&once), once);
        }
    }
}
