//! Path-to-regex tokenizer and matcher (port of `PathRegex.scala`).
//!
//! Tokenizes a path template into `PathToken`s and synthesizes a host regex (via `fancy-regex`,
//! which supports the lookahead `(?=...)` and inline `(?i)` flags the JVM regex engine provides).
//! Path rendering percent-encodes values with `encode_uri_component`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use crate::errors::RegexError;

/// Options for `PathRegex` (port of `PathRegexOptions`, merging `ParseOptions`/`RegexOptions`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathRegexOptions {
    pub case_sensitive: bool,
    pub strict: bool,
    pub end: bool,
    pub delimiter: char,
    pub delimiters: BTreeSet<char>,
    pub ends_with: Vec<String>,
}

impl Default for PathRegexOptions {
    fn default() -> Self {
        PathRegexOptions {
            case_sensitive: false,
            strict: false,
            end: true,
            delimiter: '/',
            delimiters: ['.', '/'].into_iter().collect(),
            ends_with: Vec::new(),
        }
    }
}

impl PathRegexOptions {
    pub fn case_sensitive() -> Self {
        PathRegexOptions {
            case_sensitive: true,
            ..Default::default()
        }
    }

    pub fn strict() -> Self {
        PathRegexOptions {
            strict: true,
            ..Default::default()
        }
    }

    pub fn non_end() -> Self {
        PathRegexOptions {
            end: false,
            ..Default::default()
        }
    }
}

/// A single path token (port of the Scala `PathToken` case class).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathToken {
    pub name: Option<String>,
    pub key: i32,
    pub prefix: Option<char>,
    pub delimiter: Option<char>,
    pub optional: bool,
    pub repeat: bool,
    pub partial: bool,
    pub pattern: Option<String>,
    pub raw_path_part: Option<String>,
}

impl PathToken {
    /// `PathToken(substring)` — a raw path part.
    pub fn raw(substring: &str) -> PathToken {
        PathToken {
            name: None,
            key: -1,
            prefix: None,
            delimiter: None,
            optional: false,
            repeat: false,
            partial: false,
            pattern: None,
            raw_path_part: Some(substring.to_string()),
        }
    }

    pub fn is_raw_path_part(&self) -> bool {
        self.raw_path_part.is_some()
    }

    pub fn is_token(&self) -> bool {
        self.raw_path_part.is_none()
    }

    pub fn raw_part_char(&self) -> Option<char> {
        self.raw_path_part
            .as_ref()
            .filter(|p| !p.is_empty())
            .and_then(|p| p.chars().last())
    }

    fn matches_pattern(&self, value: &str) -> Result<bool, RegexError> {
        let pattern = self.pattern.as_deref().unwrap_or("");
        let re = fancy_regex::Regex::new(&format!("^(?:{pattern})$"))
            .map_err(|e| RegexError::Compile(e.to_string()))?;
        re.is_match(value)
            .map_err(|e| RegexError::Compile(e.to_string()))
    }

    /// Convert this token to a path segment (throws in Scala; returns `Result` here).
    pub fn format_segment(
        &self,
        args: &BTreeMap<String, Vec<String>>,
        encode: &dyn Fn(&str) -> String,
    ) -> Result<String, RegexError> {
        if let Some(raw) = &self.raw_path_part {
            return Ok(raw.clone());
        }

        let arg_name = self.name.clone().unwrap_or_else(|| self.key.to_string());

        let values = args.get(&arg_name);
        match values {
            None => self.format_optional(&arg_name),
            Some(v) if v.is_empty() => self.format_optional(&arg_name),
            Some(v) if v.len() == 1 => {
                let enc_value = encode(&v[0]);
                if self.matches_pattern(&enc_value)? {
                    let prefix = self.prefix.map(|c| c.to_string()).unwrap_or_default();
                    Ok(format!("{prefix}{enc_value}"))
                } else {
                    Err(RegexError::InvalidArgument(format!(
                        "Expected {arg_name} to match pattern {}, but got value {enc_value}",
                        self.pattern.as_deref().unwrap_or("")
                    )))
                }
            }
            Some(v) => {
                let mut out = String::new();
                for (idx, value) in v.iter().enumerate() {
                    let enc_value = encode(value);
                    if self.matches_pattern(&enc_value)? {
                        if idx == 0 {
                            out.push_str(&self.prefix.map(|c| c.to_string()).unwrap_or_default());
                        } else {
                            out.push_str(
                                &self.delimiter.map(|c| c.to_string()).unwrap_or_default(),
                            );
                        }
                        out.push_str(&enc_value);
                    } else {
                        return Err(RegexError::InvalidArgument(format!(
                            "Expected {arg_name}[{idx}] to match pattern {}, but got value {enc_value}",
                            self.pattern.as_deref().unwrap_or("")
                        )));
                    }
                }
                Ok(out)
            }
        }
    }

    fn format_optional(&self, arg_name: &str) -> Result<String, RegexError> {
        if self.optional {
            Ok(self
                .prefix
                .filter(|_| self.partial)
                .map(|c| c.to_string())
                .unwrap_or_default())
        } else {
            Err(RegexError::InvalidArgument(format!(
                "Expected value for token {arg_name}"
            )))
        }
    }
}

/// A compiled path template (port of the Scala `PathRegex` case class).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathRegex {
    pub tokens: Vec<PathToken>,
    pub options: PathRegexOptions,
}

impl PathRegex {
    pub fn with_options(&self, new_options: PathRegexOptions) -> PathRegex {
        PathRegex {
            tokens: self.tokens.clone(),
            options: new_options,
        }
    }

    pub fn keys(&self) -> Vec<&PathToken> {
        self.tokens.iter().filter(|t| t.is_token()).collect()
    }

    /// Render a path from arguments (default `encodeUriComponent`).
    pub fn to_path(&self, args: &BTreeMap<String, Vec<String>>) -> Result<String, RegexError> {
        self.to_path_with(args, &PathRegex::encode_uri_component)
    }

    pub fn to_path_with(
        &self,
        args: &BTreeMap<String, Vec<String>>,
        encode: &dyn Fn(&str) -> String,
    ) -> Result<String, RegexError> {
        let mut out = String::new();
        for token in &self.tokens {
            out.push_str(&token.format_segment(args, encode)?);
        }
        Ok(out)
    }

    /// The compiled path regex (port of the Scala `lazy val regex`).
    pub fn regex(&self) -> Result<fancy_regex::Regex, RegexError> {
        let ends_with = {
            let mut parts = vec!["$".to_string()];
            parts.extend(
                self.options
                    .ends_with
                    .iter()
                    .map(|s| PathRegex::escape_string(s)),
            );
            parts.join("|")
        };

        let route: Vec<String> = self
            .tokens
            .iter()
            .filter_map(|token| {
                if token.is_raw_path_part() {
                    token
                        .raw_path_part
                        .as_ref()
                        .map(|raw| PathRegex::escape_string(raw))
                } else {
                    token.pattern.as_ref().map(|token_pattern| {
                        let prefix = token
                            .prefix
                            .map(|p| PathRegex::escape_string(&p.to_string()))
                            .unwrap_or_default();
                        let capture = if token.repeat {
                            format!("(?:{token_pattern})(?:{prefix}(?:{token_pattern}))*")
                        } else {
                            token_pattern.clone()
                        };
                        if token.optional {
                            if token.partial {
                                format!("{prefix}({capture})?")
                            } else {
                                format!("(?:{prefix}({capture}))?")
                            }
                        } else {
                            format!("{prefix}({capture})")
                        }
                    })
                }
            })
            .collect();

        let route_finish: Vec<String> = if self.options.end {
            let strict_finish = if self.options.strict {
                Vec::new()
            } else {
                vec![format!("(?:{})?", self.options.delimiter)]
            };
            let mut finish = strict_finish;
            finish.push(if ends_with == "$" {
                "$".to_string()
            } else {
                format!("(?={ends_with})")
            });
            finish
        } else {
            let strict_finish = if self.options.strict {
                Vec::new()
            } else {
                vec![format!("(?:{}(?={ends_with}))?", self.options.delimiter)]
            };
            let is_end_delimited = self
                .tokens
                .last()
                .and_then(|t| t.raw_part_char())
                .is_some_and(|c| self.options.delimiters.contains(&c));
            let finish = if !is_end_delimited {
                vec![format!("(?={}|{ends_with})", self.options.delimiter)]
            } else {
                Vec::new()
            };
            let mut result = strict_finish;
            result.extend(finish);
            result
        };

        let pattern_flags = if self.options.case_sensitive {
            ""
        } else {
            "(?i)"
        };
        let body: String = route.iter().chain(route_finish.iter()).cloned().collect();
        let pattern = format!("{pattern_flags}^{body}");
        fancy_regex::Regex::new(&pattern).map_err(|e| RegexError::Compile(e.to_string()))
    }

    /// Escape a regex string (port of `PathRegex.escapeString`).
    pub fn escape_string(s: &str) -> String {
        let mut out = String::new();
        for c in s.chars() {
            if is_regex_special(c) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }

    /// Escape a capturing group (port of `PathRegex.escapeGroup`).
    fn escape_group(group: &str) -> String {
        let mut out = String::new();
        for c in group.chars() {
            if matches!(c, '=' | '!' | ':' | '$' | '/' | '(' | ')') {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }

    /// Percent-encode a URI component (port of `PathRegex.encodeUriComponent`).
    pub fn encode_uri_component(s: &str) -> String {
        if s.chars().all(uri_allowed) {
            s.to_string()
        } else {
            let mut out = String::new();
            for c in s.chars() {
                if uri_allowed(c) {
                    out.push(c);
                } else {
                    let mut buf = [0u8; 4];
                    for b in c.encode_utf8(&mut buf).as_bytes() {
                        out.push_str(&format!("%{b:02X}"));
                    }
                }
            }
            out
        }
    }

    /// Parse a path string into tokens (port of `PathRegex.parse`).
    pub fn parse(input: &str, options: &PathRegexOptions) -> Vec<PathToken> {
        let rx = rx_path_regex();
        let mut state = ParseState {
            sub_str: input.to_string(),
            raw_path_part: String::new(),
            tokens: Vec::new(),
            path_escaped: false,
        };

        loop {
            let caps = rx.captures(&state.sub_str).ok().flatten();
            let Some(caps) = caps else {
                let final_collected = format!("{}{}", state.raw_path_part, state.sub_str);
                if !final_collected.is_empty() {
                    state.tokens.insert(0, PathToken::raw(&final_collected));
                }
                break;
            };

            let Some(whole) = caps.get(0) else {
                break;
            };
            let start = whole.start();
            let end = whole.end();
            let sub_str_after = state.sub_str[end..].to_string();

            let mut raw_path_part = state.raw_path_part.clone();
            if start > 0 {
                raw_path_part.push_str(&state.sub_str[..start]);
            }

            if let Some(escape_match) = caps.get(1) {
                if let Some(ch) = escape_match.as_str().chars().nth(1) {
                    raw_path_part.push(ch);
                }
                let tokens = state.tokens.clone();
                state = ParseState {
                    sub_str: sub_str_after,
                    raw_path_part,
                    tokens,
                    path_escaped: true,
                };
            } else {
                let mut tokens = state.tokens.clone();

                let (prev, actual_path) = if !state.path_escaped && !raw_path_part.is_empty() {
                    match raw_path_part.chars().last() {
                        Some(last) if options.delimiters.contains(&last) => {
                            (Some(last), strip_last_char(&raw_path_part))
                        }
                        _ => (None, raw_path_part),
                    }
                } else {
                    (None, raw_path_part)
                };

                if !actual_path.is_empty() {
                    tokens.insert(0, PathToken::raw(&actual_path));
                }

                let next = sub_str_after.chars().next();

                let grp_capture = caps.get(3).map(|m| m.as_str().to_string());
                let grp_group = caps.get(4).map(|m| m.as_str().to_string());
                let grp_modifier = caps.get(5).map(|m| m.as_str().to_string());

                let delimiter = prev.unwrap_or(options.delimiter);
                let pattern_group = grp_capture.or(grp_group);

                let user_token = PathToken {
                    name: caps.get(2).map(|m| m.as_str().to_string()),
                    key: tokens.iter().filter(|t| t.is_token()).count() as i32,
                    prefix: prev,
                    delimiter: Some(delimiter),
                    optional: grp_modifier
                        .as_deref()
                        .is_some_and(|m| m == "?" || m == "*"),
                    repeat: grp_modifier
                        .as_deref()
                        .is_some_and(|m| m == "+" || m == "*"),
                    partial: prev.is_some() && next.is_some() && next != prev,
                    pattern: Some(pattern_group.map(|g| Self::escape_group(&g)).unwrap_or_else(
                        || {
                            format!(
                                "[^{}]+?",
                                Self::escape_string(&delimiter.to_string())
                            )
                        },
                    )),
                    raw_path_part: None,
                };

                tokens.insert(0, user_token);
                state = ParseState {
                    sub_str: sub_str_after,
                    raw_path_part: String::new(),
                    tokens,
                    path_escaped: false,
                };
            }
        }

        state.tokens.reverse();
        state.tokens
    }

    /// Compile a path string to a template (port of `PathRegex.apply`).
    pub fn apply(input: &str, options: PathRegexOptions) -> PathRegex {
        let tokens = Self::parse(input, &options);
        PathRegex { tokens, options }
    }
}

fn is_regex_special(c: char) -> bool {
    matches!(
        c,
        '.' | '+'
            | '*'
            | '?'
            | '='
            | '^'
            | '!'
            | ':'
            | '$'
            | '{'
            | '}'
            | '('
            | ')'
            | '['
            | ']'
            | '|'
            | '/'
            | '\\'
    )
}

fn uri_allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
}

fn strip_last_char(s: &str) -> String {
    match s.chars().last() {
        Some(last) => s[..s.len() - last.len_utf8()].to_string(),
        None => String::new(),
    }
}

struct ParseState {
    sub_str: String,
    raw_path_part: String,
    tokens: Vec<PathToken>,
    path_escaped: bool,
}

/// The tokenizer regex, with `\w` rendered as ASCII `[a-zA-Z0-9_]` to match the JVM's `\w`.
const RX_PATH_PATTERN: &str =
    r"(\\.)|(?:\:([a-zA-Z0-9_]+)(?:\(((?:\\.|[^\\()])+)\))?|\(((?:\\.|[^\\()])+)\))([+*?])?";

fn rx_path_regex() -> &'static fancy_regex::Regex {
    static RX: OnceLock<fancy_regex::Regex> = OnceLock::new();
    // The pattern is a compile-time constant (ported from the working Scala regex) and always
    // compiles; the `expect` is a static-validity assertion, not a runtime error path.
    RX.get_or_init(|| {
        fancy_regex::Regex::new(RX_PATH_PATTERN).expect("rxPath is a static constant and compiles")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn tok(
        name: Option<&str>,
        key: i32,
        prefix: Option<char>,
        delimiter: Option<char>,
        optional: bool,
        repeat: bool,
        partial: bool,
        pattern: &str,
    ) -> PathToken {
        PathToken {
            name: name.map(|s| s.to_string()),
            key,
            prefix,
            delimiter,
            optional,
            repeat,
            partial,
            pattern: Some(pattern.to_string()),
            raw_path_part: None,
        }
    }

    fn raw(s: &str) -> PathToken {
        PathToken::raw(s)
    }

    fn args(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn apply(path: &str, options: PathRegexOptions) -> PathRegex {
        PathRegex::apply(path, options)
    }

    #[test]
    fn empty_and_simple_paths() {
        let p = apply("/", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/");
        assert_eq!(p.to_path(&args(&[("id", &["123"])])).unwrap(), "/");

        let p = apply("/test", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/test")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test");

        let p = apply("/test/", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/test/")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test/");
    }

    #[test]
    fn case_sensitive_paths() {
        let p = apply("/test", PathRegexOptions::case_sensitive());
        assert_eq!(p.tokens, vec![raw("/test")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test");

        let p = apply("/TEST", PathRegexOptions::case_sensitive());
        assert_eq!(p.tokens, vec![raw("/TEST")]);
    }

    #[test]
    fn strict_mode() {
        let p = apply("/test", PathRegexOptions::strict());
        assert_eq!(p.tokens, vec![raw("/test")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test");

        let p = apply("/test/", PathRegexOptions::strict());
        assert_eq!(p.tokens, vec![raw("/test/")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test/");
    }

    #[test]
    fn single_named_parameter() {
        let p = apply("/:test", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), false, false, false, "[^\\/]+?")]
        );
        assert_eq!(p.to_path(&args(&[("test", &["route"])])).unwrap(), "/route");
        assert_eq!(
            p.to_path(&args(&[("test", &["something/else"])])).unwrap(),
            "/something%2Felse"
        );
        assert_eq!(
            p.to_path(&args(&[("test", &["something/else/more"])])).unwrap(),
            "/something%2Felse%2Fmore"
        );
    }

    #[test]
    fn named_parameter_strict_mode() {
        let p = apply("/:test", PathRegexOptions::strict());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), false, false, false, "[^\\/]+?")]
        );
        assert_eq!(p.to_path(&args(&[("test", &["route"])])).unwrap(), "/route");

        let p = apply("/:test/", PathRegexOptions::strict());
        assert_eq!(
            p.tokens,
            vec![
                tok(Some("test"), 0, Some('/'), Some('/'), false, false, false, "[^\\/]+?"),
                raw("/")
            ]
        );
        assert_eq!(p.to_path(&args(&[("test", &["route"])])).unwrap(), "/route/");
    }

    #[test]
    fn optional_named_parameter() {
        let p = apply("/:test?", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), true, false, false, "[^\\/]+?")]
        );

        let p = apply("/:test?", PathRegexOptions::strict());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), true, false, false, "[^\\/]+?")]
        );
        assert_eq!(p.to_path(&args(&[])).unwrap(), "");
        assert_eq!(p.to_path(&args(&[("test", &["foobar"])])).unwrap(), "/foobar");
    }

    #[test]
    fn optional_named_parameter_in_middle() {
        let p = apply("/:test?/bar", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![
                tok(Some("test"), 0, Some('/'), Some('/'), true, false, false, "[^\\/]+?"),
                raw("/bar")
            ]
        );
        assert_eq!(p.to_path(&args(&[("test", &["foo"])])).unwrap(), "/foo/bar");

        let p = apply("/:test?-bar", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![
                tok(Some("test"), 0, Some('/'), Some('/'), true, false, true, "[^\\/]+?"),
                raw("-bar")
            ]
        );
        assert_eq!(p.to_path(&args(&[("test", &["aaa"])])).unwrap(), "/aaa-bar");

        let p = apply("/:test*-bar", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![
                tok(Some("test"), 0, Some('/'), Some('/'), true, true, true, "[^\\/]+?"),
                raw("-bar")
            ]
        );
        assert_eq!(p.to_path(&args(&[("test", &["aaa"])])).unwrap(), "/aaa-bar");
        assert_eq!(
            p.to_path(&args(&[("test", &["aaa", "bbb"])])).unwrap(),
            "/aaa/bbb-bar"
        );
    }

    #[test]
    fn repeated_parameters() {
        let p = apply("/:test+", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), false, true, false, "[^\\/]+?")]
        );
        assert!(p.to_path(&args(&[])).is_err());
        assert_eq!(p.to_path(&args(&[("test", &["foobar"])])).unwrap(), "/foobar");
        assert_eq!(
            p.to_path(&args(&[("test", &["a", "b", "c"])])).unwrap(),
            "/a/b/c"
        );
    }

    #[test]
    fn repeated_inline_regex() {
        let p = apply("/:test(\\d+)+", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), false, true, false, "\\d+")]
        );
        assert!(p.to_path(&args(&[("test", &["abc"])])).is_err());
        assert_eq!(p.to_path(&args(&[("test", &["123"])])).unwrap(), "/123");
        assert_eq!(
            p.to_path(&args(&[("test", &["1", "2", "3"])])).unwrap(),
            "/1/2/3"
        );
    }

    #[test]
    fn custom_named_parameter_no_repeat() {
        let p = apply("/:test(\\d+)", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), false, false, false, "\\d+")]
        );
        assert!(p.to_path(&args(&[("test", &["abc"])])).is_err());
        assert_eq!(p.to_path(&args(&[("test", &["123"])])).unwrap(), "/123");
    }

    #[test]
    fn custom_named_wildcard() {
        let p = apply("/:test(.*)", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, Some('/'), Some('/'), false, false, false, ".*")]
        );
        assert_eq!(p.to_path(&args(&[("test", &[""])])).unwrap(), "/");
        assert_eq!(p.to_path(&args(&[("test", &["abc"])])).unwrap(), "/abc");
        assert_eq!(p.to_path(&args(&[("test", &["abc/123"])])).unwrap(), "/abc%2F123");
    }

    #[test]
    fn custom_named_charsets() {
        let p = apply("/:route([a-z]+)", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("route"), 0, Some('/'), Some('/'), false, false, false, "[a-z]+")]
        );
        assert!(p.to_path(&args(&[("route", &["123"])])).is_err());
        assert_eq!(p.to_path(&args(&[("route", &["abc"])])).unwrap(), "/abc");
    }

    #[test]
    fn custom_named_alternation() {
        let p = apply("/:route(this|that)", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("route"), 0, Some('/'), Some('/'), false, false, false, "this|that")]
        );
        assert_eq!(p.to_path(&args(&[("route", &["this"])])).unwrap(), "/this");
        assert_eq!(p.to_path(&args(&[("route", &["that"])])).unwrap(), "/that");
        assert!(p.to_path(&args(&[("route", &["abc"])])).is_err());
    }

    #[test]
    fn prefixed_slashes() {
        let p = apply("test", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("test")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "test");

        let p = apply(":test", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("test"), 0, None, Some('/'), false, false, false, "[^\\/]+?")]
        );
        assert!(p.to_path(&args(&[])).is_err());
        assert_eq!(p.to_path(&args(&[("test", &["route"])])).unwrap(), "route");
    }

    #[test]
    fn formats() {
        let p = apply("/test.json", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/test.json")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test.json");

        let p = apply("/:test.json", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![
                tok(Some("test"), 0, Some('/'), Some('/'), false, false, true, "[^\\/]+?"),
                raw(".json")
            ]
        );
        assert!(p.to_path(&args(&[])).is_err());
        assert_eq!(p.to_path(&args(&[("test", &["foo"])])).unwrap(), "/foo.json");

        let p = apply("/test.:format", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![
                raw("/test"),
                tok(Some("format"), 0, Some('.'), Some('.'), false, false, false, "[^\\.]+?")
            ]
        );
        assert_eq!(p.to_path(&args(&[("format", &["foo"])])).unwrap(), "/test.foo");
    }

    #[test]
    fn unnamed_params() {
        let p = apply("/(\\d+)", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(None, 0, Some('/'), Some('/'), false, false, false, "\\d+")]
        );
        assert!(p.to_path(&args(&[])).is_err());
        assert_eq!(p.to_path(&args(&[("0", &["123"])])).unwrap(), "/123");
    }

    #[test]
    fn escaped_characters() {
        let p = apply("/\\(testing\\)", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/(testing)")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/(testing)");

        let p = apply("/.+*?=^!:${}[]|", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/.+*?=^!:${}[]|")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/.+*?=^!:${}[]|");
    }

    #[test]
    fn unicode_characters() {
        let p = apply("/:foo", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![tok(Some("foo"), 0, Some('/'), Some('/'), false, false, false, "[^\\/]+?")]
        );
        assert_eq!(p.to_path(&args(&[("foo", &["café"])])).unwrap(), "/caf%C3%A9");

        let p = apply("/café", PathRegexOptions::default());
        assert_eq!(p.tokens, vec![raw("/café")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/café");
    }

    #[test]
    fn ends_with() {
        let p = apply(
            "/test",
            PathRegexOptions {
                ends_with: vec!["?".to_string()],
                ..Default::default()
            },
        );
        assert_eq!(p.tokens, vec![raw("/test")]);
        assert_eq!(p.to_path(&args(&[])).unwrap(), "/test");
    }

    #[test]
    fn custom_delimiters() {
        let p = apply(
            "$:foo$:bar?",
            PathRegexOptions {
                delimiters: ['$'].into_iter().collect(),
                ..Default::default()
            },
        );
        assert_eq!(
            p.tokens,
            vec![
                tok(Some("foo"), 0, Some('$'), Some('$'), false, false, false, "[^\\$]+?"),
                tok(Some("bar"), 1, Some('$'), Some('$'), true, false, false, "[^\\$]+?")
            ]
        );
        assert_eq!(p.to_path(&args(&[("foo", &["foo"])])).unwrap(), "$foo");
        assert_eq!(
            p.to_path(&args(&[("foo", &["foo"]), ("bar", &["bar"])])).unwrap(),
            "$foo$bar"
        );
    }

    #[test]
    fn unnamed_group_prefix() {
        let p = apply("/(apple-)?icon-:res(\\d+).png", PathRegexOptions::default());
        assert_eq!(
            p.tokens,
            vec![
                tok(None, 0, Some('/'), Some('/'), true, false, true, "apple-"),
                raw("icon-"),
                tok(Some("res"), 1, None, Some('/'), false, false, false, "\\d+"),
                raw(".png")
            ]
        );
    }

    #[test]
    fn simple_accept_matching() {
        // Single named parameter: matches with the named capture.
        let re = apply("/:test", PathRegexOptions::default()).regex().unwrap();
        let caps = re.captures("/route").unwrap().unwrap();
        assert_eq!(caps.get(0).unwrap().as_str(), "/route");
        assert_eq!(caps.get(1).unwrap().as_str(), "route");

        // Unnamed parameter.
        let re = apply("/(\\d+)", PathRegexOptions::default()).regex().unwrap();
        let caps = re.captures("/123").unwrap().unwrap();
        assert_eq!(caps.get(0).unwrap().as_str(), "/123");
        assert_eq!(caps.get(1).unwrap().as_str(), "123");
        assert!(re.captures("/abc").unwrap().is_none());

        // Simple literal path (case-insensitive by default).
        let re = apply("/test", PathRegexOptions::default()).regex().unwrap();
        assert!(re.is_match("/TEST").unwrap());
        assert!(!re.is_match("/route").unwrap());
    }
}
