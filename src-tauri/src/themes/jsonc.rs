//! VS Code theme files are often JSONC: JSON with `//` and `/* */` comments and trailing
//! commas. `to_json` rewrites that into strict JSON for `serde_json`, leaving strings untouched.

/// Removes comments and trailing commas outside of strings.
pub fn to_json(input: &str) -> String {
    trailing_commas(&comments(input))
}

/// Replaces comments with a space (so tokens never merge) and keeps strings verbatim.
fn comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        out.push(escaped);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Drops a `,` whose next non-whitespace character closes an object or array.
fn trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&escaped) = chars.get(i + 1) {
                    out.push(escaped);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|ch| !ch.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn parse(input: &str) -> Value {
        serde_json::from_str(&to_json(input)).expect("valid JSON after cleanup")
    }

    #[test]
    fn strips_line_and_block_comments() {
        let input = "{\n  // name\n  \"a\": 1, /* inline */ \"b\": 2\n  /* multi\n line */\n}";
        assert_eq!(parse(input), json!({ "a": 1, "b": 2 }));
    }

    #[test]
    fn removes_trailing_commas_in_objects_and_arrays() {
        assert_eq!(parse("{ \"a\": [1, 2, ], \"b\": { \"c\": 3, }, }"), json!({ "a": [1, 2], "b": { "c": 3 } }));
    }

    #[test]
    fn keeps_comment_markers_and_commas_inside_strings() {
        let input = r#"{ "url": "https://example.com/a,]", "q": "say \"/* hi */\"", }"#;
        assert_eq!(parse(input), json!({ "url": "https://example.com/a,]", "q": "say \"/* hi */\"" }));
    }

    #[test]
    fn strict_json_is_unchanged() {
        let input = r##"{"a":"#fff","b":[1,2]}"##;
        assert_eq!(to_json(input), input);
    }
}
