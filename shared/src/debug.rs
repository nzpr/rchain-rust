//! Debug helpers (port of the pure `suffixAfterLast` half of `shared/Debug.scala`).
//!
//! The `sourcecode`-based `print`/`printUnsafe`/`string` are deferred — they capture the expression
//! source and source location via Scala macros, which map to a Rust `macro_rules!`.

/// The substring after the last occurrence of `pattern`, or the whole string when absent (port of
/// `Debug.suffixAfterLast`).
pub fn suffix_after_last<'a>(pattern: &str, string: &'a str) -> &'a str {
    string
        .rsplit_once(pattern)
        .map(|(_, after)| after)
        .unwrap_or(string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_after_last_extracts_tail() {
        assert_eq!(suffix_after_last(".", "a.b.c"), "c");
        assert_eq!(suffix_after_last("/", "path/to/file"), "file");
    }

    #[test]
    fn suffix_after_last_returns_whole_when_absent() {
        assert_eq!(suffix_after_last(".", "no_dots"), "no_dots");
    }
}
