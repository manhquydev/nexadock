//! Diagnostic log sanitization to prevent leaking user paths, tokens, passwords, or session cookies.

use std::path::Path;

const SENSITIVE_KEY_VALUE_MARKERS: &[&str] = &[
    "token=",
    "bearer ",
    "cookie=",
    "zpw_sek=",
    "session=",
    "session_key=",
    "password=",
    "secret=",
    "authorization:",
];

const SENSITIVE_JSON_KEYS: &[&str] = &[
    "\"token\"",
    "\"zpw_sek\"",
    "\"secret\"",
    "\"cookie\"",
    "\"password\"",
    "\"session_key\"",
];

fn replace_ascii_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_owned();
    }
    let hay_lower = haystack.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    if hay_lower.len() != haystack.len() {
        return haystack.replace(needle, replacement);
    }
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0usize;
    for (idx, _) in hay_lower.match_indices(&needle_lower) {
        out.push_str(&haystack[last..idx]);
        out.push_str(replacement);
        last = idx + needle.len();
    }
    out.push_str(&haystack[last..]);
    out
}

/// Redacts OS user home prefixes and secret-looking key/value or JSON tokens from diagnostic strings.
#[must_use]
pub fn redact_sensitive_text(input: &str, user_home: Option<&Path>) -> String {
    let mut output = input.to_owned();

    if let Some(home) = user_home {
        let home_str = home.to_string_lossy();
        if !home_str.is_empty() {
            output = replace_ascii_case_insensitive(&output, home_str.as_ref(), "<USER_HOME>");
            let forward = home_str.replace('\\', "/");
            if forward != home_str.as_ref() {
                output = replace_ascii_case_insensitive(&output, &forward, "<USER_HOME>");
            }
            if let Some(username) = home.file_name().and_then(|s| s.to_str()) {
                if username.len() >= 3 && !username.eq_ignore_ascii_case("users") {
                    output = replace_ascii_case_insensitive(&output, username, "<USER>");
                }
            }
        }
    }

    let mut redacted_lines = Vec::new();
    for line in output.lines() {
        redacted_lines.push(redact_line_secrets(line));
    }
    if input.ends_with('\n') {
        let mut joined = redacted_lines.join("\n");
        joined.push('\n');
        joined
    } else {
        redacted_lines.join("\n")
    }
}

fn redact_line_secrets(line: &str) -> String {
    let mut current = redact_json_secrets_on_line(line);
    let lower = current.to_ascii_lowercase();

    for marker in SENSITIVE_KEY_VALUE_MARKERS {
        if let Some(idx) = lower.find(marker) {
            let prefix_end = idx + marker.len();
            let prefix = &current[..prefix_end];
            current = format!("{prefix}<REDACTED>");
            break;
        }
    }

    current
}

fn redact_json_secrets_on_line(line: &str) -> String {
    let mut result = line.to_owned();

    for key in SENSITIVE_JSON_KEYS {
        let mut search_from = 0usize;
        while search_from < result.len() {
            let lower = result.to_ascii_lowercase();
            let Some(rel_idx) = lower[search_from..].find(key) else {
                break;
            };
            let key_start = search_from + rel_idx;
            let key_end = key_start + key.len();
            let after_key = &result[key_end..];
            let trimmed_ws = after_key.trim_start();
            let Some(after_colon) = trimmed_ws.strip_prefix(':') else {
                search_from = key_end;
                continue;
            };

            let ws_before_colon = after_key.len() - trimmed_ws.len();
            let colon_end = key_end + ws_before_colon + 1;
            let after_colon_trimmed = after_colon.trim_start();
            let ws_after_colon = after_colon.len() - after_colon_trimmed.len();
            let value_start = colon_end + ws_after_colon;

            if after_colon_trimmed.starts_with('"') {
                let mut escaped = false;
                let mut value_end = result.len();
                for (offset, ch) in result[value_start + 1..].char_indices() {
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if ch == '\\' {
                        escaped = true;
                        continue;
                    }
                    if ch == '"' {
                        value_end = value_start + 1 + offset + 1;
                        break;
                    }
                }
                let replacement = "\"<REDACTED>\"";
                result.replace_range(value_start..value_end, replacement);
                search_from = value_start + replacement.len();
            } else {
                let value_end = result[value_start..]
                    .find([',', '}'])
                    .map(|off| value_start + off)
                    .unwrap_or(result.len());
                let replacement = "\"<REDACTED>\"";
                result.replace_range(value_start..value_end, replacement);
                search_from = value_start + replacement.len();
            }
        }
    }

    result
}
