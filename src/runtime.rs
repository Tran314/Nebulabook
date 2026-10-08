//! Branding-compatible diagnostic options; no persistent settings are changed.
use std::ffi::OsString;

/// Prefer the current name while keeping explicitly configured Nebula 4.0
/// diagnostics working during upgrades. An explicit current value always wins.
pub fn option(suffix: &str) -> Option<OsString> {
    from_lookup(suffix, |name| std::env::var_os(name))
}

fn from_lookup(suffix: &str, mut lookup: impl FnMut(&str) -> Option<OsString>) -> Option<OsString> {
    lookup(&format!("NEBULABOOK_{suffix}")).or_else(|| lookup(&format!("NEBULA_{suffix}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_name_wins_over_legacy_even_when_disabled() {
        let actual = from_lookup("NO_ERROR_DIALOG", |name| match name {
            "NEBULABOOK_NO_ERROR_DIALOG" => Some("0".into()),
            "NEBULA_NO_ERROR_DIALOG" => Some("1".into()),
            _ => None,
        });
        assert_eq!(actual, Some("0".into()));
    }

    #[test]
    fn legacy_explicit_values_remain_supported() {
        let actual = from_lookup("STARTUP_LOG", |name| {
            (name == "NEBULA_STARTUP_LOG").then(|| OsString::from("old-diagnostic.log"))
        });
        assert_eq!(actual, Some("old-diagnostic.log".into()));
        assert!(from_lookup("STARTUP_LOG", |_| None).is_none());
    }
}
