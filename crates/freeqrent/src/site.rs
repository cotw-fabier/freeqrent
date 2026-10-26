//! Palettes from a site's own text (SPEC §6.4): `<meta name="theme-color">`,
//! custom properties, and the colors in style sheets and `style`
//! attributes, ranked by how often and where they appear. The library
//! never fetches anything; callers pass the text in.
//!
//! The scanner is tolerant rather than complete: it reads what a palette
//! needs (selectors, declarations, at-rule blocks, `var()` chains) and
//! skips everything else, and it never fails on malformed input.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::color::Color;
use crate::extract::{ExtractError, PaletteReport, Roles, Swatch, assign};
use crate::palette::Palette;

/// Block nesting deeper than this is skipped.
const MAX_DEPTH: usize = 16;
/// `var()` chains longer than this are dropped.
const MAX_VAR_DEPTH: usize = 8;
/// Values and `var()` expansions one declaration may visit, so a chain
/// that fans out (`--a: var(--b) var(--b)`, …) stays linear.
const EXPANSIONS: usize = 256;
/// The longest a color function may grow while its `var()`s are
/// substituted.
const MAX_SUBSTITUTION: usize = 512;
/// Distinct colors kept; the rest of a huge sheet still adds weight to
/// colors already seen.
const MAX_COLORS: usize = 1024;
/// Swatches handed to the role assigner.
const MAX_SWATCHES: usize = 32;
/// Weight of `<meta name="theme-color">`.
const THEME_COLOR: f32 = 6.0;

impl Palette {
    /// Derives `bg`, `fg` and (when there is an accent) `finder.eye` from
    /// CSS text (SPEC §6.4). Colors are ranked by frequency and by where
    /// they appear: `:root` and `html` count most, then `body`, then links
    /// and buttons. Background, text and link colors, and custom
    /// properties named like them (`--bg`, `--text`, `--primary`, …), steer
    /// the roles. Rules for a dark color scheme are skipped. The result
    /// keeps every painted token at the default minimum contrast; colors
    /// short of it are darkened.
    pub fn from_css(css: &str) -> Result<(Palette, PaletteReport), ExtractError> {
        let mut site = Site::default();
        site.collect_vars(css);
        site.colors(css);
        site.finish()
    }

    /// [`from_css`](Palette::from_css) for an HTML document: its
    /// `<meta name="theme-color">` (a strong accent hint), its `<style>`
    /// elements and its `style` attributes. Scripts and comments are
    /// skipped.
    pub fn from_html(html: &str) -> Result<(Palette, PaletteReport), ExtractError> {
        let doc = Html::scan(html);
        let mut site = Site::default();
        for css in &doc.sheets {
            site.collect_vars(css);
        }
        for (_, decls) in &doc.inline {
            site.collect_var_decls(decls);
        }
        for theme in &doc.themes {
            site.theme_color(theme);
        }
        for css in &doc.sheets {
            site.colors(css);
        }
        for (tag, decls) in &doc.inline {
            let selector = Selector::of(tag);
            site.declarations(decls, selector);
        }
        site.finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Bg,
    Text,
    Accent,
    Other,
}

#[derive(Clone, Copy, Debug)]
struct Selector {
    weight: f32,
    /// Links and buttons: their colors are accents.
    accent: bool,
}

impl Selector {
    /// Weighs a selector list by its most important selector.
    fn of(list: &str) -> Selector {
        let mut out = Selector {
            weight: 1.0,
            accent: false,
        };
        for sel in list.split(',') {
            let sel = sel.trim().to_ascii_lowercase();
            let weight = match sel.as_str() {
                ":root" | "html" | ":host" => 4.0,
                "body" => 3.0,
                _ => 1.0,
            };
            out.weight = out.weight.max(weight);
            let accent = sel
                .split(|c: char| c.is_ascii_whitespace() || matches!(c, '>' | '+' | '~'))
                .any(|compound| {
                    let tag: String = compound
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect();
                    tag == "a"
                        || tag == "button"
                        || compound.contains("[type=submit]")
                        || compound.contains("[type=\"submit\"]")
                        || compound.split(['.', '#', ':']).skip(1).any(|part| {
                            ["btn", "button", "cta", "primary", "accent", "brand", "link"]
                                .iter()
                                .any(|k| part.contains(k))
                        })
                });
            if accent {
                out.accent = true;
                out.weight = out.weight.max(2.0);
            }
        }
        out
    }
}

/// What a custom property's name says its color is for.
fn var_role(name: &str) -> Role {
    let name = name.to_ascii_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| name.contains(k));
    if has(&[
        "primary",
        "accent",
        "brand",
        "link",
        "highlight",
        "cta",
        "action",
        "secondary",
    ]) {
        Role::Accent
    } else if has(&["bg", "background", "surface", "canvas", "paper", "base"]) {
        Role::Bg
    } else if has(&[
        "text",
        "fg",
        "foreground",
        "ink",
        "body",
        "heading",
        "content",
    ]) {
        Role::Text
    } else {
        Role::Other
    }
}

/// The role a property gives its colors, and how much they count.
fn property_role(name: &str, selector: Selector) -> Option<(Role, f32)> {
    let role = match name {
        "background" | "background-color" => Role::Bg,
        "color" => Role::Text,
        _ if name.starts_with("border")
            || name.starts_with("outline")
            || name.ends_with("-color")
            || name.ends_with("-shadow")
            || matches!(name, "fill" | "stroke" | "background-image") =>
        {
            return Some((
                if selector.accent {
                    Role::Accent
                } else {
                    Role::Other
                },
                0.5,
            ));
        }
        _ => return None,
    };
    Some((if selector.accent { Role::Accent } else { role }, 1.0))
}

#[derive(Default)]
struct Entry {
    color: Option<Color>,
    weight: f32,
    roles: Roles,
}

#[derive(Default)]
struct Site {
    vars: BTreeMap<String, String>,
    /// Keyed by the canonical form, so `#fff`, `white` and
    /// `rgb(255 255 255)` are one color.
    entries: BTreeMap<String, Entry>,
}

impl Site {
    fn collect_vars(&mut self, css: &str) {
        let css = strip_comments(css);
        rules(&css, 0, &mut |_, decls| {
            for (name, value) in declarations(decls) {
                if name.starts_with("--") {
                    self.vars.insert(name.to_string(), value.trim().to_string());
                }
            }
        });
    }

    fn collect_var_decls(&mut self, decls: &str) {
        for (name, value) in declarations(decls) {
            if name.starts_with("--") {
                self.vars.insert(name.to_string(), value.trim().to_string());
            }
        }
    }

    fn colors(&mut self, css: &str) {
        let css = strip_comments(css);
        let mut found = Vec::new();
        rules(&css, 0, &mut |selector, decls| {
            found.push((Selector::of(selector), String::from(decls)));
        });
        for (selector, decls) in found {
            self.declarations(&decls, selector);
        }
    }

    fn declarations(&mut self, decls: &str, selector: Selector) {
        for (name, value) in declarations(decls) {
            let lower = name.to_ascii_lowercase();
            let (role, weight) = if lower.starts_with("--") {
                // A definition counts when its name says what it's for;
                // a scale like `--color-blue-500` counts only where used.
                match var_role(&lower) {
                    Role::Other => continue,
                    role => (role, 1.0),
                }
            } else {
                match property_role(&lower, selector) {
                    Some(r) => r,
                    None => continue,
                }
            };
            let mut colors = Vec::new();
            let mut budget = EXPANSIONS;
            scan_value(value, &self.vars, 0, &mut budget, &mut colors);
            for color in colors {
                self.add(color, role, weight * selector.weight);
            }
        }
    }

    fn theme_color(&mut self, value: &str) {
        if let Ok(color) = Color::parse(value) {
            let [l, a, b] = color.to_oklab();
            let neutral_light = l >= 0.85 && a * a + b * b <= 0.12 * 0.12;
            let role = if neutral_light {
                Role::Bg
            } else {
                Role::Accent
            };
            self.add(color, role, THEME_COLOR);
        }
    }

    fn add(&mut self, color: Color, role: Role, weight: f32) {
        if color.alpha() < 0.5 {
            return;
        }
        let color = color.opaque();
        let key = color.to_string();
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_COLORS {
            return;
        }
        let entry = self.entries.entry(key).or_default();
        entry.color = Some(color);
        entry.weight += weight;
        match role {
            Role::Bg => entry.roles.bg += weight,
            Role::Text => entry.roles.text += weight,
            Role::Accent => entry.roles.accent += weight,
            Role::Other => {}
        }
    }

    fn finish(self) -> Result<(Palette, PaletteReport), ExtractError> {
        let total: f32 = self.entries.values().map(|e| e.weight).sum();
        if self.entries.is_empty() || total <= 0.0 {
            return Err(ExtractError::NoColors);
        }
        let mut entries: Vec<Entry> = self.entries.into_values().collect();
        // Heaviest first; the map's key order breaks ties, deterministically.
        entries.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        entries.truncate(MAX_SWATCHES);
        let swatches = entries
            .iter()
            .map(|e| Swatch {
                color: e.color.expect("set on insert"),
                share: e.weight / total,
            })
            .collect();
        let roles: Vec<Roles> = entries.iter().map(|e| e.roles).collect();
        Ok(assign(swatches, &roles, crate::extract::EXTRACT_CONTRAST))
    }
}

// ----------------------------------------------------------------- CSS

/// Replaces comments with a space, leaving strings alone.
fn strip_comments(css: &str) -> String {
    let b = css.as_bytes();
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    let mut start = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => i = skip_string(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => {
                out.push_str(&css[start..i]);
                out.push(' ');
                i = css[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push_str(&css[start.min(css.len())..]);
    out
}

/// The index just past the string starting at `i` (or the end).
fn skip_string(b: &[u8], i: usize) -> usize {
    let quote = b[i];
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == quote || c == b'\n' => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// The index of the `}` closing the block opened just before `i` (or the
/// end).
fn block_end(b: &[u8], mut i: usize) -> usize {
    let mut depth = 1usize;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = skip_string(b, i);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// Calls `rule(selector, declarations)` for every style rule, descending
/// into grouping at-rules and skipping dark-scheme and non-style blocks.
fn rules(css: &str, depth: usize, rule: &mut dyn FnMut(&str, &str)) {
    if depth > MAX_DEPTH {
        return;
    }
    let b = css.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // The prelude: up to `{`, `;` or a stray `}` outside strings and
        // parentheses.
        let start = i;
        let mut parens = 0usize;
        while i < b.len() {
            match b[i] {
                b'"' | b'\'' => {
                    i = skip_string(b, i);
                    continue;
                }
                b'(' => parens += 1,
                b')' => parens = parens.saturating_sub(1),
                b'{' | b';' | b'}' if parens == 0 => break,
                _ => {}
            }
            i += 1;
        }
        if i >= b.len() {
            return;
        }
        let prelude = css[start..i].trim();
        if b[i] != b'{' {
            i += 1;
            continue;
        }
        let end = block_end(b, i + 1);
        let body = &css[i + 1..end];
        i = end + 1;
        if let Some(at) = prelude.strip_prefix('@') {
            let name: String = at
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_ascii_lowercase();
            let condition = at.to_ascii_lowercase();
            let dark = condition.contains("prefers-color-scheme") && condition.contains("dark");
            let grouping = matches!(
                name.as_str(),
                "media" | "supports" | "layer" | "container" | "scope" | "document"
            );
            if grouping && !dark {
                rules(body, depth + 1, rule);
            }
        } else {
            // Nested rules inside the block are dropped by `declarations`.
            rule(prelude, body);
        }
    }
}

/// The `name: value` pairs of a declaration block, `!important` removed.
/// Nested rules are skipped.
fn declarations(block: &str) -> impl Iterator<Item = (&str, &str)> {
    let b = block.as_bytes();
    let mut parts = Vec::new();
    let (mut i, mut start, mut parens) = (0, 0, 0usize);
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = skip_string(b, i);
                continue;
            }
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b';' if parens == 0 => {
                parts.push(&block[start..i]);
                start = i + 1;
            }
            b'{' => {
                // A nested rule: drop its selector and body.
                i = block_end(b, i + 1) + 1;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if start < block.len() {
        parts.push(&block[start..]);
    }
    parts.into_iter().filter_map(|decl| {
        let (name, value) = decl.split_once(':')?;
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return None;
        }
        let value = value.trim();
        let value = match value.len().checked_sub(10) {
            Some(cut)
                if value.is_char_boundary(cut)
                    && value[cut..].eq_ignore_ascii_case("!important") =>
            {
                value[..cut].trim_end()
            }
            _ => value,
        };
        Some((name, value))
    })
}

/// The index of the `)` closing the parenthesis opened just before `i`.
fn paren_end(b: &[u8], mut i: usize) -> usize {
    let mut depth = 1usize;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => {
                i = skip_string(b, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

/// Collects the colors a property value paints with: literals, named
/// colors, colors inside gradients and other functions, and `var()`
/// references resolved through `vars`.
fn scan_value(
    value: &str,
    vars: &BTreeMap<String, String>,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<Color>,
) {
    if depth > MAX_VAR_DEPTH || *budget == 0 {
        return;
    }
    *budget -= 1;
    let b = value.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' || c == b'\'' {
            i = skip_string(b, i);
        } else if c == b'#' {
            let end = i + 1 + b[i + 1..].iter().take_while(|&&c| is_ident(c)).count();
            if let Ok(color) = Color::parse(&value[i..end]) {
                out.push(color);
            }
            i = end;
        } else if is_ident(c) && !c.is_ascii_digit() {
            let end = i + b[i..].iter().take_while(|&&c| is_ident(c)).count();
            let word = &value[i..end];
            if b.get(end) == Some(&b'(') {
                let close = paren_end(b, end + 1);
                let args = &value[end + 1..close.min(b.len())];
                let name = word.to_ascii_lowercase();
                match name.as_str() {
                    "rgb" | "rgba" | "hsl" | "hsla" | "oklch" | "oklab" => {
                        let whole = &value[i..(close + 1).min(b.len())];
                        let parsed = if args.contains("var(") {
                            // `hsl(var(--primary))` with `--primary: 220 90% 56%`.
                            Color::parse(&substitute(whole, vars, depth, budget))
                        } else {
                            Color::parse(whole)
                        };
                        if let Ok(color) = parsed {
                            out.push(color);
                        }
                    }
                    "var" => {
                        let (var, fallback) = match args.split_once(',') {
                            Some((v, f)) => (v.trim(), Some(f)),
                            None => (args.trim(), None),
                        };
                        match vars.get(var) {
                            Some(v) => scan_value(v, vars, depth + 1, budget, out),
                            None => {
                                if let Some(f) = fallback {
                                    scan_value(f, vars, depth + 1, budget, out);
                                }
                            }
                        }
                    }
                    // Not colors themselves, and their arguments aren't
                    // painted as given.
                    "url" | "color-mix" | "image-set" | "env" | "attr" | "calc" => {}
                    _ => scan_value(args, vars, depth + 1, budget, out),
                }
                i = close + 1;
            } else {
                if word.bytes().all(|c| c.is_ascii_alphabetic())
                    && let Ok(color) = Color::parse(word)
                    && color != Color::TRANSPARENT
                {
                    out.push(color);
                }
                i = end;
            }
        } else {
            i += 1;
        }
    }
}

/// `value` with every `var()` replaced by its definition (or fallback),
/// as text.
fn substitute(
    value: &str,
    vars: &BTreeMap<String, String>,
    depth: usize,
    budget: &mut usize,
) -> String {
    if depth > MAX_VAR_DEPTH || *budget == 0 {
        return String::new();
    }
    *budget -= 1;
    let b = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while let Some(at) = find_ci(b, i, b"var(") {
        out.push_str(&value[i..at]);
        let close = paren_end(b, at + 4);
        let args = &value[at + 4..close.min(b.len())];
        let (var, fallback) = match args.split_once(',') {
            Some((v, f)) => (v.trim(), Some(f.trim())),
            None => (args.trim(), None),
        };
        if let Some(v) = vars.get(var).map(String::as_str).or(fallback) {
            out.push_str(&substitute(v, vars, depth + 1, budget));
            if out.len() > MAX_SUBSTITUTION {
                return String::new();
            }
        }
        i = (close + 1).min(b.len());
    }
    out.push_str(&value[i..]);
    out
}

// ---------------------------------------------------------------- HTML

#[derive(Default)]
struct Html {
    sheets: Vec<String>,
    /// `(tag name, style attribute)`.
    inline: Vec<(String, String)>,
    themes: Vec<String>,
}

/// Finds `needle` (ASCII, lower case) in `hay` from `from`, ignoring case.
fn find_ci(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len())
        .find(|&i| hay[i..i + needle.len()].eq_ignore_ascii_case(needle))
}

impl Html {
    fn scan(html: &str) -> Html {
        let b = html.as_bytes();
        let mut doc = Html::default();
        let mut i = 0;
        while let Some(lt) = b[i..].iter().position(|&c| c == b'<').map(|p| p + i) {
            if b[lt..].starts_with(b"<!--") {
                i = find_ci(b, lt + 4, b"-->").map_or(b.len(), |e| e + 3);
                continue;
            }
            let name_end = lt
                + 1
                + b[lt + 1..]
                    .iter()
                    .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'-')
                    .count();
            let tag = html[lt + 1..name_end].to_ascii_lowercase();
            let (attrs, end) = attributes(html, name_end);
            i = end;
            if tag.is_empty() {
                continue;
            }
            let attr = |name: &str| {
                attrs
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.as_str())
            };
            if let Some(style) = attr("style") {
                doc.inline.push((tag.clone(), String::from(style)));
            }
            match tag.as_str() {
                "meta" => {
                    let dark =
                        attr("media").is_some_and(|m| m.to_ascii_lowercase().contains("dark"));
                    if attr("name").is_some_and(|n| n.eq_ignore_ascii_case("theme-color"))
                        && !dark
                        && let Some(content) = attr("content")
                    {
                        doc.themes.push(String::from(content));
                    }
                }
                "style" | "script" | "template" | "textarea" | "title" => {
                    let close = alloc::format!("</{tag}");
                    let stop = find_ci(b, i, close.as_bytes()).unwrap_or(b.len());
                    if tag == "style" {
                        let media = attr("media").map(str::to_ascii_lowercase);
                        let dark = media.is_some_and(|m| {
                            m.contains("prefers-color-scheme") && m.contains("dark")
                        });
                        if !dark {
                            doc.sheets.push(String::from(&html[i..stop]));
                        }
                    }
                    i = stop;
                }
                _ => {}
            }
        }
        doc
    }
}

/// Reads a tag's attributes from `i` (just past the name) to its `>`.
/// Names are lower case; values are unquoted, with the few entities a
/// color or declaration might contain decoded.
fn attributes(html: &str, mut i: usize) -> (Vec<(String, String)>, usize) {
    let b = html.as_bytes();
    let mut attrs = Vec::new();
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        if i >= b.len() {
            return (attrs, b.len());
        }
        if b[i] == b'>' {
            return (attrs, i + 1);
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let name = html[start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let quote = b[i];
                let end = b[i + 1..]
                    .iter()
                    .position(|&c| c == quote)
                    .map_or(b.len(), |p| i + 1 + p);
                value = decode_entities(&html[i + 1..end]);
                i = (end + 1).min(b.len());
            } else {
                let start = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                value = decode_entities(&html[start..i]);
            }
        }
        // An empty name means a stray `=`, which the value branch consumed.
        if !name.is_empty() {
            attrs.push((name, value));
        }
    }
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return String::from(s);
    }
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#35;", "#")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{Choice, Origin};
    use crate::palette::Token;

    fn choice(report: &PaletteReport, token: Token) -> Option<Choice> {
        report.choices.iter().copied().find(|c| c.token == token)
    }

    #[test]
    fn root_variables_name_the_roles() {
        let css = "
            :root { --brand-primary: #e4572e; --surface: #fbfaf7; --text-strong: oklch(24% .03 260); }
            body { background: var(--surface); color: var(--text-strong); }
            a, .btn-primary { color: var(--brand-primary); }
            .card { border: 1px solid #ddd; box-shadow: 0 1px 2px rgba(0,0,0,.2); }
            @media (prefers-color-scheme: dark) { body { background: #000; color: #fff } }
        ";
        let (palette, report) = Palette::from_css(css).unwrap();
        let bg = choice(&report, Token::Bg).unwrap();
        assert_eq!(bg.color, Color::parse("#fbfaf7").unwrap());
        assert!(matches!(bg.origin, Origin::Swatch(_)));
        let fg = choice(&report, Token::Fg).unwrap();
        assert_eq!(fg.color, Color::parse("oklch(24% .03 260)").unwrap());
        let eye = choice(&report, Token::FinderEye).unwrap();
        // The orange is darkened to reach the contrast, keeping its hue.
        assert!(matches!(eye.origin, Origin::Darkened { .. }));
        assert!(palette.contrast(Token::FinderEye).delta_l >= crate::extract::EXTRACT_CONTRAST);
        // The dark scheme and the translucent shadow never appear.
        let black = Color::parse("#000").unwrap();
        assert!(report.swatches.iter().all(|s| s.color != black));
    }

    #[test]
    fn equal_colors_merge_and_comments_are_skipped() {
        let css = "/* body { color: red } */ p { color: #fff } h1 { color: white } \
                   h2 { color: rgb(255 255 255) } .x { content: \"#123456\"; color: #222 }";
        let (_, report) = Palette::from_css(css).unwrap();
        let colors: Vec<_> = report
            .swatches
            .iter()
            .map(|s| s.color.to_string())
            .collect();
        assert_eq!(colors, ["#fff", "#222"], "{report:?}");
        assert!((report.swatches[0].share - 0.75).abs() < 1e-6);
    }

    #[test]
    fn html_reads_meta_style_and_attributes() {
        let html = r##"<!doctype html><html><head>
            <meta name="theme-color" content="#1a73e8">
            <meta name="theme-color" media="(prefers-color-scheme: dark)" content="#ff00ff">
            <!-- <style>body{color:lime}</style> -->
            <style>body { background-color: #ffffff; color: #202124 }</style>
            <script>const s = "<style>body{color:red}</style>";</script>
            </head><body style="border-top: 4px solid #1a73e8">
            <a style='color: #1a73e8'>x</a></body></html>"##;
        let (_, report) = Palette::from_html(html).unwrap();
        let color = |t| choice(&report, t).unwrap().color.to_string();
        assert_eq!(color(Token::Bg), "#fff");
        assert_eq!(color(Token::Fg), "#202124");
        // The brand blue is the accent, darkened: it's only ΔL 0.43 from
        // white.
        let blue = report
            .swatches
            .iter()
            .position(|s| s.color.to_string() == "#1a73e8")
            .unwrap();
        assert!(matches!(
            choice(&report, Token::FinderEye).unwrap().origin,
            Origin::Darkened { swatch, .. } if swatch == blue
        ));
        for skipped in ["#f0f", "#0f0", "#f00"] {
            assert!(
                report
                    .swatches
                    .iter()
                    .all(|s| s.color.to_string() != skipped)
            );
        }
    }

    #[test]
    fn component_variables_resolve_inside_color_functions() {
        // The shadcn/ui convention: bare HSL components in variables.
        let css = ":root { --background: 0 0% 100%; --foreground: 222.2 84% 4.9%; \
                   --primary: 262 83% 58%; --color-red-500: #ef4444; --color-lime-300: #bef264 } \
                   body { background: hsl(var(--background)); color: hsl(var(--foreground)) } \
                   .btn { background: hsl(var(--primary) / 1) }";
        let (_, report) = Palette::from_css(css).unwrap();
        let color = |t| choice(&report, t).unwrap().color.to_string();
        assert_eq!(color(Token::Bg), "#fff");
        assert_eq!(color(Token::Fg), "#020817");
        let eye = choice(&report, Token::FinderEye).unwrap();
        let violet = report
            .swatches
            .iter()
            .position(|s| s.color.to_string() == "#7c3bed");
        assert!(violet.is_some(), "{report:?}");
        assert!(
            matches!(eye.origin, Origin::Swatch(i) | Origin::Darkened { swatch: i, .. } if Some(i) == violet)
        );
        // Unused scale colors never become swatches.
        assert!(
            report
                .swatches
                .iter()
                .all(|s| s.color.to_string() != "#ef4444")
        );
    }

    #[test]
    fn hostile_input_is_harmless() {
        for input in [
            "",
            "{{{{",
            "}}}}",
            "a{color:",
            "@media{",
            "--x:var(--x)",
            ":root{--a:var(--b);--b:var(--a)} p{color:var(--a)}",
            "p{color:#",
            "\"unterminated",
            "/* open",
            "<style",
            "<meta name=theme-color content=",
            "<a style=\"color:red",
            "p{color:rgb(1 2 3}",
            "é{color:ré}",
            "p{color:#ab\u{e9}}",
            // Found by fuzzing: a stray `=` before a multibyte character.
            "<meta =\"x\"\u{10c}>",
        ] {
            let _ = Palette::from_css(input);
            let _ = Palette::from_html(input);
        }
        // Found by fuzzing: `var()` chains that fan out stay bounded, and
        // a shallow one still resolves.
        let fan = |levels: usize| {
            let mut css = String::from(":root{--v0:#123456}");
            for i in 1..=levels {
                let p = i - 1;
                css += &alloc::format!(":root{{--v{i}:var(--v{p}) var(--v{p}) var(--v{p})}}");
                css += &alloc::format!(":root{{--c{i}:rgb(var(--v{p}) var(--v{p}))}}");
            }
            css + &alloc::format!("body{{color:var(--v{levels});background:rgb(var(--c{levels}))}}")
        };
        assert_eq!(Palette::from_css(&fan(6)).unwrap().1.swatches.len(), 1);
        assert!(Palette::from_css(&fan(40)).is_err());
        let deep = "@media{".repeat(100) + "p{color:#123}" + &"}".repeat(100);
        assert!(Palette::from_css(&deep).is_err());
        assert_eq!(
            Palette::from_css("p { margin: 0 }"),
            Err(ExtractError::NoColors)
        );
    }
}
