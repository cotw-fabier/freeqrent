//! A small JSON reader and writer (RFC 8259), enough for Style documents
//! and the results that go back. Objects keep their key order, and a
//! number keeps its text, so integers up to `u64::MAX` survive.

use std::fmt::Write;

/// The deepest nesting a document may have. Style documents need five
/// levels; the limit keeps hostile input from exhausting the stack.
pub const MAX_DEPTH: usize = 64;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as written.
    Number(String),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object, in document order. Duplicate keys are refused.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The value's JSON type, for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "a boolean",
            Value::Number(_) => "a number",
            Value::String(_) => "a string",
            Value::Array(_) => "an array",
            Value::Object(_) => "an object",
        }
    }
}

/// Why a text isn't JSON: what was wrong, at which byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    /// What was expected or found.
    pub reason: String,
    /// The byte offset into the text.
    pub offset: usize,
}

/// Parses one JSON value, with nothing but whitespace around it.
pub fn parse(text: &str) -> Result<Value, SyntaxError> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        text,
        pos: 0,
    };
    p.space();
    let value = p.value(0)?;
    p.space();
    if p.pos < p.bytes.len() {
        return Err(p.error("text after the JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, reason: &str) -> SyntaxError {
        SyntaxError {
            reason: reason.to_string(),
            offset: self.pos,
        }
    }

    fn space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, SyntaxError> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown literal"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, SyntaxError> {
        if depth >= MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        match self.bytes.get(self.pos) {
            None => Err(self.error("unexpected end of text")),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("expected a value")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, SyntaxError> {
        self.pos += 1;
        let mut members: Vec<(String, Value)> = Vec::new();
        self.space();
        if self.eat(b'}') {
            return Ok(Value::Object(members));
        }
        loop {
            self.space();
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(self.error("expected a key"));
            }
            let at = self.pos;
            let key = self.string()?;
            if members.iter().any(|(k, _)| *k == key) {
                return Err(SyntaxError {
                    reason: format!("duplicate key {key:?}"),
                    offset: at,
                });
            }
            self.space();
            if !self.eat(b':') {
                return Err(self.error("expected ':'"));
            }
            self.space();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.space();
            if self.eat(b'}') {
                return Ok(Value::Object(members));
            }
            if !self.eat(b',') {
                return Err(self.error("expected ',' or '}'"));
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, SyntaxError> {
        self.pos += 1;
        let mut items = Vec::new();
        self.space();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            self.space();
            items.push(self.value(depth + 1)?);
            self.space();
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            if !self.eat(b',') {
                return Err(self.error("expected ',' or ']'"));
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, SyntaxError> {
        let digits = self
            .text
            .get(self.pos..self.pos + 4)
            .filter(|d| d.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| self.error("expected four hex digits"))?;
        self.pos += 4;
        Ok(u32::from_str_radix(digits, 16).expect("hex digits"))
    }

    fn string(&mut self) -> Result<String, SyntaxError> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            // `start..pos` ends before an ASCII byte, so it's on a char
            // boundary of valid UTF-8.
            out.push_str(&self.text[start..self.pos]);
            match self.bytes.get(self.pos) {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let escaped = match self.bytes.get(self.pos) {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            self.pos += 1;
                            let at = self.pos;
                            let hi = self.hex4()?;
                            let code = if (0xd800..0xdc00).contains(&hi) {
                                if !self.text[self.pos..].starts_with("\\u") {
                                    return Err(self.error("unpaired surrogate"));
                                }
                                self.pos += 2;
                                let lo = self.hex4()?;
                                if !(0xdc00..0xe000).contains(&lo) {
                                    return Err(self.error("unpaired surrogate"));
                                }
                                0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                            } else {
                                hi
                            };
                            let c = char::from_u32(code).ok_or(SyntaxError {
                                reason: "unpaired surrogate".to_string(),
                                offset: at,
                            })?;
                            out.push(c);
                            continue;
                        }
                        _ => return Err(self.error("unknown escape")),
                    };
                    out.push(escaped);
                    self.pos += 1;
                }
                Some(_) => return Err(self.error("control character in a string")),
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while let Some(b'0'..=b'9') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn number(&mut self) -> Result<Value, SyntaxError> {
        let start = self.pos;
        self.eat(b'-');
        if self.eat(b'0') {
            if let Some(b'0'..=b'9') = self.bytes.get(self.pos) {
                return Err(self.error("leading zero"));
            }
        } else if self.digits() == 0 {
            return Err(self.error("expected a digit"));
        }
        if self.eat(b'.') && self.digits() == 0 {
            return Err(self.error("expected a digit after '.'"));
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if self.digits() == 0 {
                return Err(self.error("expected an exponent"));
            }
        }
        Ok(Value::Number(self.text[start..self.pos].to_string()))
    }
}

/// Writes `s` as a JSON string.
pub fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\u{:04x}", c as u32).expect("writing to a String");
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Writes a number in its shortest round-trip form, or `null` when it
/// isn't finite.
pub fn write_f32(out: &mut String, x: f32) {
    if x.is_finite() {
        write!(out, "{x}").expect("writing to a String");
    } else {
        out.push_str("null");
    }
}

/// A JSON object written member by member.
pub struct ObjectWriter<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> ObjectWriter<'a> {
    /// Opens an object.
    pub fn new(out: &'a mut String) -> ObjectWriter<'a> {
        out.push('{');
        ObjectWriter { out, first: true }
    }

    /// Starts a member and returns the buffer to write its value into.
    pub fn key(&mut self, key: &str) -> &mut String {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_str(self.out, key);
        self.out.push(':');
        self.out
    }

    /// A string member.
    pub fn str(&mut self, key: &str, value: &str) {
        write_str(self.key(key), value);
    }

    /// A number member.
    pub fn f32(&mut self, key: &str, value: f32) {
        write_f32(self.key(key), value);
    }

    /// An integer member.
    pub fn int(&mut self, key: &str, value: impl std::fmt::Display) {
        write!(self.key(key), "{value}").expect("writing to a String");
    }

    /// A boolean member.
    pub fn bool(&mut self, key: &str, value: bool) {
        self.key(key).push_str(if value { "true" } else { "false" });
    }

    /// A member written as raw JSON.
    pub fn raw(&mut self, key: &str, json: &str) {
        self.key(key).push_str(json);
    }

    /// Closes the object.
    pub fn end(self) {
        self.out.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Value {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e:?}"))
    }

    #[test]
    fn values() {
        assert_eq!(ok(" null "), Value::Null);
        assert_eq!(ok("true"), Value::Bool(true));
        assert_eq!(ok("-0.5e+3"), Value::Number("-0.5e+3".into()));
        assert_eq!(
            ok("18446744073709551615"),
            Value::Number("18446744073709551615".into())
        );
        assert_eq!(
            ok(r#"{"a":[1,{"b":"c"}],"d":{}}"#),
            Value::Object(vec![
                (
                    "a".into(),
                    Value::Array(vec![
                        Value::Number("1".into()),
                        Value::Object(vec![("b".into(), Value::String("c".into()))])
                    ])
                ),
                ("d".into(), Value::Object(vec![])),
            ])
        );
    }

    #[test]
    fn strings() {
        assert_eq!(
            ok(r#""a\"\\\/\b\f\n\r\t\u00e9\ud83d\ude00é""#),
            Value::String("a\"\\/\u{8}\u{c}\n\r\té😀é".into())
        );
        let mut out = String::new();
        write_str(&mut out, "a\"\\\n\u{1}é");
        assert_eq!(out, r#""a\"\\\n\u0001é""#);
        assert_eq!(ok(&out), Value::String("a\"\\\n\u{1}é".into()));
    }

    #[test]
    fn errors() {
        for (text, offset) in [
            ("", 0),
            ("{", 1),
            ("[1,]", 3),
            ("01", 1),
            ("1.", 2),
            ("\"\\ud800\"", 7),
            ("\"\\udc00\"", 3),
            ("\"a\nb\"", 2),
            ("{\"a\":1,\"a\":2}", 7),
            ("nul", 0),
            ("1 2", 2),
            ("\"\\x\"", 2),
        ] {
            let e = parse(text).expect_err(text);
            assert_eq!(e.offset, offset, "{text:?}: {e:?}");
        }
        let deep = "[".repeat(MAX_DEPTH + 1);
        assert_eq!(parse(&deep).unwrap_err().reason, "nested too deeply");
        assert!(
            parse(&format!(
                "{}{}",
                "[".repeat(MAX_DEPTH - 1),
                "]".repeat(MAX_DEPTH - 1)
            ))
            .is_ok()
        );
    }

    #[test]
    fn numbers_write_short() {
        let mut out = String::new();
        write_f32(&mut out, 0.63);
        out.push(' ');
        write_f32(&mut out, f32::NAN);
        out.push(' ');
        write_f32(&mut out, 2.0);
        assert_eq!(out, "0.63 null 2");
    }
}
