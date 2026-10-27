//! Typed reads out of a parsed document, each error naming the JSON path
//! it happened at. Objects are strict: [`Obj::finish`] refuses keys nobody
//! read.

use freeqrent::Rgba8Image;

use crate::error::{DocError, image_error};
use crate::json::Value;

/// The path of `key` inside `path`.
pub(crate) fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn wrong(path: &str, expected: &str, v: &Value) -> DocError {
    DocError::invalid(path, format!("expected {expected}, got {}", v.kind()))
}

/// An object being read, key by key.
pub(crate) struct Obj<'a> {
    path: String,
    members: &'a [(String, Value)],
    used: Vec<bool>,
}

impl<'a> Obj<'a> {
    pub(crate) fn new(v: &'a Value, path: &str) -> Result<Obj<'a>, DocError> {
        match v {
            Value::Object(members) => Ok(Obj {
                path: path.to_string(),
                members,
                used: vec![false; members.len()],
            }),
            _ => Err(wrong(path, "an object", v)),
        }
    }

    /// The value at `key`, if present; `null` counts as absent.
    pub(crate) fn get(&mut self, key: &str) -> Option<(&'a Value, String)> {
        let i = self.members.iter().position(|(k, _)| k == key)?;
        self.used[i] = true;
        match &self.members[i].1 {
            Value::Null => None,
            v => Some((v, join(&self.path, key))),
        }
    }

    /// Whether `key` is present and not `null`, without reading it.
    pub(crate) fn has(&self, key: &str) -> bool {
        self.members
            .iter()
            .any(|(k, v)| k == key && *v != Value::Null)
    }

    /// The value at `key`, read with `f`.
    pub(crate) fn opt<T>(
        &mut self,
        key: &str,
        f: impl FnOnce(&'a Value, &str) -> Result<T, DocError>,
    ) -> Result<Option<T>, DocError> {
        match self.get(key) {
            None => Ok(None),
            Some((v, path)) => f(v, &path).map(Some),
        }
    }

    /// The value at `key`, which must be present.
    pub(crate) fn req<T>(
        &mut self,
        key: &str,
        f: impl FnOnce(&'a Value, &str) -> Result<T, DocError>,
    ) -> Result<T, DocError> {
        match self.get(key) {
            None => Err(DocError::invalid(&join(&self.path, key), "missing")),
            Some((v, path)) => f(v, &path),
        }
    }

    /// Refuses any key that wasn't read.
    pub(crate) fn finish(self) -> Result<(), DocError> {
        for ((key, _), used) in self.members.iter().zip(&self.used) {
            if !used {
                return Err(DocError::invalid(&join(&self.path, key), "unknown key"));
            }
        }
        Ok(())
    }

    /// The members, for objects keyed by data (palettes, corners).
    pub(crate) fn entries(mut self) -> impl Iterator<Item = (&'a str, &'a Value, String)> {
        self.used.iter_mut().for_each(|u| *u = true);
        let path = self.path;
        self.members
            .iter()
            .map(move |(k, v)| (k.as_str(), v, join(&path, k)))
    }
}

pub(crate) fn f32_of(v: &Value, path: &str) -> Result<f32, DocError> {
    match v {
        Value::Number(text) => {
            let x: f64 = text.parse().expect("JSON number syntax");
            let x = x as f32;
            if x.is_finite() {
                Ok(x)
            } else {
                Err(DocError::invalid(path, "number out of range"))
            }
        }
        _ => Err(wrong(path, "a number", v)),
    }
}

/// A whole number in `0..=max`. Written as an integer, or as a float with
/// no fraction (`4.0`, `1e3`) within 2^53.
pub(crate) fn uint_of(v: &Value, path: &str, max: u64) -> Result<u64, DocError> {
    let Value::Number(text) = v else {
        return Err(wrong(path, "a whole number", v));
    };
    let n = match text.parse::<u64>() {
        Ok(n) => Some(n),
        Err(_) => {
            let x: f64 = text.parse().expect("JSON number syntax");
            (x.fract() == 0.0 && (0.0..=9_007_199_254_740_992.0).contains(&x)).then_some(x as u64)
        }
    };
    match n {
        Some(n) if n <= max => Ok(n),
        _ => Err(DocError::invalid(
            path,
            format!("expected a whole number from 0 to {max}, got {text}"),
        )),
    }
}

pub(crate) fn u8_of(v: &Value, path: &str) -> Result<u8, DocError> {
    uint_of(v, path, u8::MAX as u64).map(|n| n as u8)
}

pub(crate) fn u32_of(v: &Value, path: &str) -> Result<u32, DocError> {
    uint_of(v, path, u32::MAX as u64).map(|n| n as u32)
}

pub(crate) fn u64_of(v: &Value, path: &str) -> Result<u64, DocError> {
    uint_of(v, path, u64::MAX)
}

pub(crate) fn bool_of(v: &Value, path: &str) -> Result<bool, DocError> {
    match v {
        Value::Bool(b) => Ok(*b),
        _ => Err(wrong(path, "a boolean", v)),
    }
}

pub(crate) fn str_of<'a>(v: &'a Value, path: &str) -> Result<&'a str, DocError> {
    match v {
        Value::String(s) => Ok(s),
        _ => Err(wrong(path, "a string", v)),
    }
}

pub(crate) fn array_of<'a>(v: &'a Value, path: &str) -> Result<&'a [Value], DocError> {
    match v {
        Value::Array(items) => Ok(items),
        _ => Err(wrong(path, "an array", v)),
    }
}

/// `[a, b]`.
pub(crate) fn pair_of(v: &Value, path: &str) -> Result<(f32, f32), DocError> {
    match array_of(v, path)? {
        [a, b] => Ok((
            f32_of(a, &format!("{path}[0]"))?,
            f32_of(b, &format!("{path}[1]"))?,
        )),
        _ => Err(DocError::invalid(path, "expected two numbers")),
    }
}

/// One of `names` (the value as written, which is one of them).
pub(crate) fn name_of<'a>(v: &Value, path: &str, names: &[&'a str]) -> Result<&'a str, DocError> {
    let s = str_of(v, path)?;
    names
        .iter()
        .find(|n| **n == s)
        .copied()
        .ok_or_else(|| unknown(path, s, names))
}

pub(crate) fn unknown(path: &str, got: &str, names: &[&str]) -> DocError {
    DocError::invalid(
        path,
        format!(
            "unknown value {got:?}; expected one of {}",
            names.join(", ")
        ),
    )
}

/// A tagged union: a bare name for a variant without data, or an object
/// whose `kind` is the name. Returns the name and the object (with `kind`
/// read) when there was one.
pub(crate) fn tagged<'a, 'n>(
    v: &'a Value,
    path: &str,
    names: &[&'n str],
) -> Result<(&'n str, Option<Obj<'a>>), DocError> {
    match v {
        Value::String(_) => Ok((name_of(v, path, names)?, None)),
        Value::Object(_) => {
            let mut o = Obj::new(v, path)?;
            let kind = o.req("kind", |v, p| name_of(v, p, names))?;
            Ok((kind, Some(o)))
        }
        _ => Err(wrong(path, "a string or an object", v)),
    }
}

/// Decodes standard base64 (RFC 4648), padding optional, no whitespace.
pub(crate) fn base64_of(v: &Value, path: &str) -> Result<Vec<u8>, DocError> {
    let s = str_of(v, path)?.as_bytes();
    let s = s.strip_suffix(b"==").or(s.strip_suffix(b"=")).unwrap_or(s);
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for (i, &c) in s.iter().enumerate() {
        let d = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => {
                return Err(DocError::invalid(
                    path,
                    format!("invalid base64 at character {i}"),
                ));
            }
        };
        acc = (acc << 6) | d as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if bits >= 6 || acc != 0 {
        return Err(DocError::invalid(path, "base64 ends mid-byte"));
    }
    Ok(out)
}

/// `{"width": w, "height": h, "rgba": base64}`.
pub(crate) fn image_of(v: &Value, path: &str) -> Result<Rgba8Image, DocError> {
    let mut o = Obj::new(v, path)?;
    let width = o.req("width", u32_of)?;
    let height = o.req("height", u32_of)?;
    let rgba = o.req("rgba", base64_of)?;
    o.finish()?;
    Rgba8Image::new(width, height, rgba).map_err(|e| image_error(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    fn b64(s: &str) -> Result<Vec<u8>, DocError> {
        base64_of(&Value::String(s.into()), "x")
    }

    #[test]
    fn base64() {
        assert_eq!(b64("").unwrap(), b"");
        assert_eq!(b64("Zg==").unwrap(), b"f");
        assert_eq!(b64("Zm8").unwrap(), b"fo");
        assert_eq!(b64("Zm9v").unwrap(), b"foo");
        assert_eq!(b64("+/8=").unwrap(), [0xfb, 0xff]);
        assert!(b64("Zm9v!").is_err());
        assert!(b64("Z").is_err());
        assert!(b64("Zh==").is_err());
    }

    #[test]
    fn numbers() {
        let n = |s: &str| parse(s).unwrap();
        assert_eq!(uint_of(&n("255"), "q", 255).unwrap(), 255);
        assert_eq!(uint_of(&n("4.0"), "q", 255).unwrap(), 4);
        assert!(uint_of(&n("256"), "q", 255).is_err());
        assert!(uint_of(&n("-1"), "q", 255).is_err());
        assert!(uint_of(&n("1.5"), "q", 255).is_err());
        assert_eq!(u64_of(&n("18446744073709551615"), "s").unwrap(), u64::MAX);
        assert!(f32_of(&n("1e300"), "x").is_err());
    }

    #[test]
    fn strict_objects() {
        let v = parse(r#"{"a":1,"b":null,"c":2}"#).unwrap();
        let mut o = Obj::new(&v, "root").unwrap();
        assert!(o.get("b").is_none());
        assert_eq!(o.req("a", u8_of).unwrap(), 1);
        let e = o.finish().unwrap_err();
        assert_eq!(e.message, "root.c: unknown key");
    }
}
