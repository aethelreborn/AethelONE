pub const WINDOW_APP_ID: &str = "oneclient_app";
pub const WINDOW_TITLE: &str = "AethelONE";

pub const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFBMTQzMTk0RjRFNTQxNUMKUldSY1FlWDBsREVVcWpRSkVGTFZYalNKU05Jcktzd1NtNWdObjJ2SElEYlQ3bk50ZlhhdW53QTQK";
pub const UPDATER_ENDPOINT: &str =
    "https://github.com/aethelreborn/AethelONE/releases/latest/download/latest.json";
pub const RELEASES_URL: &str = "https://github.com/aethelreborn/AethelONE/releases/latest";
/// Release assets live under `/releases/download/oneclient-<version>/`; the
/// `.deb`/`.rpm` updater path builds its download URLs from this base.
pub const RELEASES_DOWNLOAD_BASE: &str =
    "https://github.com/aethelreborn/AethelONE/releases/download";

#[cfg(test)]
mod tests {
    use super::UPDATER_PUBKEY;
    use base64::Engine as _;

    #[test]
    fn updater_pubkey_matches_minisign_verify_contract() {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(UPDATER_PUBKEY)
            .expect("updater pubkey must be base64");
        let text = std::str::from_utf8(&decoded).expect("updater pubkey must be utf-8");
        minisign_verify::PublicKey::decode(text)
            .expect("pubkey must decode after the single base64 pass cargo-packager-updater applies");
    }
}
