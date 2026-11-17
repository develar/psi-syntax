//! The shape of a label as Bazel writes it (bazel.build/concepts/labels), told from a word without resolving it:
//! which repository or package a label names is the caller's (accio's `crates/bazel/src/label.rs`).

/// Does a string read as a label rather than as a word: `//…`, `@x…`, `:x…`? A bare `name` is a label only where an
/// attribute holds one, which the caller decides.
pub fn shaped(s: &str) -> bool {
    let ident = |c: Option<char>| {
        c.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@' | '/' | '+' | '$'))
    };
    s.starts_with("//")
        || (s.starts_with('@') && ident(s[1..].chars().next()))
        || (s.starts_with(':') && ident(s[1..].chars().next()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert!(shaped("//a") && shaped("@x//:y") && shaped(":x") && shaped("//"));
        assert!(!shaped("a") && !shaped("@%s") && !shaped(":") && !shaped("/a") && !shaped("--add-opens"));
    }
}
