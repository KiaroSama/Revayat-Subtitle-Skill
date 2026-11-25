pub use super::logging::{LogLevel, OperationalLog};
use regex::Regex;
use std::{borrow::Cow, sync::LazyLock};

static CREDENTIAL: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?i)(\b(?:authorization|proxy-authorization|cookie|set-cookie)\s*:\s*)[^\r\n]+|(\b(?:api[_-]?key|password|passwd|token|secret|credential|session(?:[_-]?id)?|access[_-]?key)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,;]+)"#).ok()
});
static URL: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)(https?://)[^\s/@]+(?::[^\s/@]*)?@").ok());
static BEARER: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)\b(bearer|basic)\s+[a-z0-9._~+/=-]+").ok());

pub fn redact(text: &str) -> String {
    let Some(credentials) = CREDENTIAL.as_ref() else {
        return "[REDACTED]".into();
    };
    let Some(url) = URL.as_ref() else {
        return "[REDACTED]".into();
    };
    let Some(bearer) = BEARER.as_ref() else {
        return "[REDACTED]".into();
    };
    let redacted = credentials.replace_all(text, "${1}${2}[REDACTED]");
    let redacted = url.replace_all(&redacted, "${1}[REDACTED]@");
    let redacted = bearer.replace_all(&redacted, "${1} [REDACTED]");
    redacted
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn label(input: &str) -> Cow<'_, str> {
    if !input.is_empty()
        && input.len() <= 64
        && input
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        Cow::Borrowed(input)
    } else {
        Cow::Borrowed("unknown")
    }
}
pub fn line(component: &str, level: &str, message: &str) -> String {
    let stamp = jiff::Timestamp::now();
    format!(
        "[{} UTC] [{}] [{}] {}",
        stamp.strftime("%Y-%m-%d %H:%M:%S"),
        label(level),
        label(component),
        redact(message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_credentials_and_flattens_forged_lines() {
        let text = line(
            "test",
            "WARNING",
            "password=demo-secret https://user:private@example.invalid/path\nAuthorization: Bearer abc.xyz\nDone token='value with spaces'",
        );
        assert!(
            text.contains("[WARNING] [test]")
                && text.contains("example.invalid")
                && text.contains("Done")
        );
        for secret in [
            "demo-secret",
            "user:private",
            "abc.xyz",
            "value with spaces",
            "\n",
        ] {
            assert!(!text.contains(secret));
        }
    }
}
