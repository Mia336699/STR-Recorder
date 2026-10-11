//! Platform-qualified identities preserve existing Stripchat settings and filesystem names.
use crate::core::error::{AppError, Result};

pub const CB_PREFIX: &str = "chaturbate~";

pub fn normalize(input: &str) -> Result<String> {
    normalize_with_mirror(input, None)
}

pub fn normalize_with_mirror(input: &str, mirror: Option<&str>) -> Result<String> {
    let input = input.trim().to_lowercase();
    let (is_cb, name) = if input.contains("://") {
        let url =
            reqwest::Url::parse(&input).map_err(|_| AppError::Other("Invalid room URL".into()))?;
        let host = url.host_str().unwrap_or("");
        let is_cb = host == "chaturbate.com" || host.ends_with(".chaturbate.com");
        let mirror_host = mirror.and_then(|value| {
            reqwest::Url::parse(&format!("https://{}/", value.trim_end_matches('/')))
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
        });
        let is_sc = host == "stripchat.com"
            || host.ends_with(".stripchat.com")
            || mirror_host.as_deref() == Some(host);
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || !(is_cb || is_sc)
        {
            return Err(AppError::Other(
                "Use an HTTPS Stripchat or Chaturbate room URL".into(),
            ));
        }
        (is_cb, url.path().trim_matches('/').to_owned())
    } else if let Some(name) = input.strip_prefix(CB_PREFIX) {
        (true, name.to_owned())
    } else {
        (false, input)
    };
    let valid_name = if is_cb {
        name.as_str()
    } else {
        name.strip_suffix("@xh").unwrap_or(&name)
    };
    if valid_name.is_empty()
        || !valid_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || (!is_cb && b == b'-'))
    {
        return Err(AppError::Other(
            "Room username must contain only letters, digits or underscores".into(),
        ));
    }
    Ok(if is_cb {
        format!("{CB_PREFIX}{name}")
    } else {
        name
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn localized_urls_and_distinct_identities() {
        assert_eq!(
            normalize("https://zh-hans.chaturbate.com/Alice/?x=1").unwrap(),
            "chaturbate~alice"
        );
        assert_eq!(normalize("Alice").unwrap(), "alice");
        assert_eq!(normalize("alice@xh").unwrap(), "alice@xh");
        assert_eq!(
            normalize_with_mirror("https://mirror.test/alice/", Some("mirror.test")).unwrap(),
            "alice"
        );
        assert_eq!(
            normalize("https://zh.stripchat.com/Alice/").unwrap(),
            "alice"
        );
        assert_eq!(normalize("chaturbate~Alice").unwrap(), "chaturbate~alice");
    }
    #[test]
    fn rejects_paths_and_untrusted_hosts() {
        for input in [
            "../alice",
            "chaturbate~",
            "https://chaturbate.com.evil.test/alice/",
            "https://evil.test/alice",
            "https://chaturbate.com/alice/other",
            "https://a@chaturbate.com/alice",
            "http://chaturbate.com/alice",
        ] {
            assert!(normalize(input).is_err(), "{input}");
        }
    }
}
