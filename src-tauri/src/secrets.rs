//! API keys live in the OS credential store (Windows Credential Manager,
//! Secret Service on Linux), never in settings.json and never in the webview.

use crate::settings::ProviderKind;

const SERVICE: &str = "io.github.klysman08.kokorokaku";

fn entry(provider: ProviderKind) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("{}-api-key", provider.as_str()))
        .map_err(|e| format!("credential store unavailable: {e}"))
}

pub fn get_key(provider: ProviderKind) -> Result<Option<String>, String> {
    match entry(provider)?.get_password() {
        Ok(k) if !k.trim().is_empty() => Ok(Some(k)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("could not read API key: {e}")),
    }
}

pub fn set_key(provider: ProviderKind, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return delete_key(provider);
    }
    entry(provider)?
        .set_password(key)
        .map_err(|e| format!("could not save API key: {e}"))
}

pub fn delete_key(provider: ProviderKind) -> Result<(), String> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not delete API key: {e}")),
    }
}

/// Masked hint for the UI, e.g. "AIza…x9Q2". Never returns the full key.
pub fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn masks_keys() {
        assert_eq!(mask("AIzaSyA1234567890x9Q2"), "AIza…x9Q2");
        assert_eq!(mask("short"), "•••••");
    }
}
