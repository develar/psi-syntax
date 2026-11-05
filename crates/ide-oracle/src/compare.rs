//! Byte-for-byte comparison of an oracle's output with a port's: JSON lines (one value per input, in list order) and
//! texts (one dump per input). Equality is of bytes; the explanation of a difference names its first place, the field
//! of a JSON value or the line of a text.
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

/// The lines of a JSON line file, split at `\n` alone (a value may hold U+2028 or U+0085, which are not line ends here).
pub fn lines(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text.split_terminator('\n').map(str::to_owned).collect())
}

/// Two JSON line files of the same list.
pub struct JsonLines {
    pub left: Vec<String>,
    pub right: Vec<String>,
}

impl JsonLines {
    pub fn read(left: &Path, right: &Path) -> Result<Self> {
        Ok(JsonLines { left: lines(left)?, right: lines(right)? })
    }

    /// The indices of the lines that differ, byte for byte; `None` when the files differ in length (one side lost or
    /// added an input, so no line pairs with its input).
    pub fn differing(&self) -> Option<Vec<usize>> {
        (self.left.len() == self.right.len())
            .then(|| self.left.iter().zip(&self.right).enumerate().filter(|(_, (l, r))| l != r).map(|(i, _)| i).collect())
    }

    /// Line `i`'s difference in words: `PATH: /field/[2]: NAME_L <value> NAME_R <value>`, PATH the value's `path`
    /// field.
    pub fn explain(&self, i: usize, names: (&str, &str)) -> String {
        explain(&self.left[i], &self.right[i], names)
    }
}

/// The difference of two JSON lines in words (see [`JsonLines::explain`]).
pub fn explain(left: &str, right: &str, (l, r): (&str, &str)) -> String {
    let (Ok(jl), Ok(jr)) = (serde_json::from_str::<Value>(left), serde_json::from_str::<Value>(right)) else {
        return format!("unparsable: {}", clip_str(right));
    };
    let path = jl.get("path").and_then(Value::as_str).unwrap_or("?");
    match first_difference(&jl, &jr) {
        Some(d) => format!("{path}: {}: {l} {} {r} {}", d.at, clip(&d.left), clip(&d.right)),
        None => format!("{path}: the same values, written differently (escaping or key order)"),
    }
}

/// Where two JSON values first differ: `at` is a pointer (`/field/sub[2]`, `[3] (length)` for arrays of different
/// lengths); a key one side lacks is `<absent>` there.
#[derive(Debug, PartialEq)]
pub struct Difference {
    pub at: String,
    pub left: Value,
    pub right: Value,
}

/// The first difference of two values in document order: an object's keys in `left`'s order, then the keys only
/// `right` has.
pub fn first_difference(left: &Value, right: &Value) -> Option<Difference> {
    first_at(left, right, String::new())
}

fn first_at(a: &Value, b: &Value, at: String) -> Option<Difference> {
    let absent = || Value::String("<absent>".into());
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                let here = format!("{at}/{k}");
                match y.get(k) {
                    None => return Some(Difference { at: here, left: v.clone(), right: absent() }),
                    Some(w) => {
                        if let Some(d) = first_at(v, w, here) {
                            return Some(d);
                        }
                    }
                }
            }
            y.iter().find(|(k, _)| !x.contains_key(*k)).map(|(k, v)| Difference { at: format!("{at}/{k}"), left: absent(), right: v.clone() })
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_at(v, w, format!("{at}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| Difference { at: format!("{at} (length)"), left: Value::from(x.len()), right: Value::from(y.len()) })
        }
        _ => (a != b).then(|| Difference { at: if at.is_empty() { "/".into() } else { at }, left: a.clone(), right: b.clone() }),
    }
}

/// A value as JSON, cut at 200 characters.
pub fn clip(v: &Value) -> String {
    clip_str(&v.to_string())
}

fn clip_str(s: &str) -> String {
    if s.chars().count() > 200 { s.chars().take(200).collect::<String>() + "…" } else { s.to_owned() }
}

/// The 1-based number of the first line where two texts differ (a line that one side lacks counts), `None` when they
/// are equal byte for byte.
pub fn first_differing_line(left: &[u8], right: &[u8]) -> Option<usize> {
    if left == right {
        return None;
    }
    let common = left.iter().zip(right).take_while(|(a, b)| a == b).count();
    Some(1 + left[..common].iter().filter(|&&b| b == b'\n').count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_first_differing_field_is_named() {
        let d = first_difference(&json!({"a": 1, "b": [1, {"c": "x"}]}), &json!({"a": 1, "b": [1, {"c": "y"}]})).unwrap();
        assert_eq!(d.at, "/b[1]/c");
        assert_eq!(first_difference(&json!([1]), &json!([1, 2])).unwrap().at, " (length)");
        assert_eq!(first_difference(&json!({"a": 1}), &json!({"a": 1, "z": 2})).unwrap().at, "/z");
        assert_eq!(first_difference(&json!(1), &json!(2)).unwrap().at, "/");
        assert!(first_difference(&json!({"a": 1}), &json!({"a": 1})).is_none());
        assert_eq!(
            explain(r#"{"path":"p","x":[1]}"#, r#"{"path":"p","x":[2]}"#, ("oracle", "accio")),
            "p: /x[0]: oracle 1 accio 2"
        );
        assert_eq!(explain(r#"{"a":1,"b":2}"#, r#"{"b":2,"a":1}"#, ("o", "a")), "?: the same values, written differently (escaping or key order)");
    }

    #[test]
    fn the_first_differing_line_is_counted_from_one() {
        assert_eq!(first_differing_line(b"a\nb\n", b"a\nb\n"), None);
        assert_eq!(first_differing_line(b"a\nb\n", b"a\nc\n"), Some(2));
        assert_eq!(first_differing_line(b"a", b"b"), Some(1));
        assert_eq!(first_differing_line(b"a\n", b"a\nb"), Some(2));
    }

    #[test]
    fn lines_split_at_newlines_alone() {
        let dir = std::env::temp_dir().join(format!("ide-oracle-lines-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), "{\"s\":\"\u{2028}\u{85}\"}\n{}\n").unwrap();
        assert_eq!(lines(&dir.join("a")).unwrap().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
