pub const WINDOW_APP_ID: &str = "oneclient_app";
pub const WINDOW_TITLE: &str = "AethelONE";

pub const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFBMTQzMTk0RjRFNTQxNUMKUldSY1FlWDBsREVVcWpRSkVGTDZYalNKU05Jcktzd1NtNWdObjJ2SElEYlQ3bk50ZlhhdW53QTQK";
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
        minisign_verify::PublicKey::decode(text).expect(
            "pubkey must decode after the single base64 pass cargo-packager-updater applies",
        );
    }

    /// The pubkey is a hand-managed constant. In 2.8.3 a one-byte transcription
    /// error when unwrapping an older double-encoded value produced a key that
    /// decoded fine but failed every release signature with "The signature
    /// verification failed" (issue #3). Decode-only checks cannot see that, so
    /// pin the actual key material: key_id + 32-byte public key.
    #[test]
    fn updater_pubkey_key_material_is_the_release_signing_key() {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(UPDATER_PUBKEY)
            .expect("updater pubkey must be base64");
        let text = std::str::from_utf8(&decoded).expect("updater pubkey must be utf-8");
        let bin = base64::engine::general_purpose::STANDARD
            .decode(text.lines().nth(1).expect("pubkey text needs a key line"))
            .expect("key line must be base64");
        assert_eq!(
            &hex_to_bytes("5c41e5f4943114aa")[..],
            &bin[2..10],
            "key_id drifted"
        );
        assert_eq!(
            &hex_to_bytes("34091052fa5e348948d22b2acc129b980d9f6bc72036d3ee736d7d76ae9f0038")[..],
            &bin[10..42],
            "public key drifted: release signatures will fail to verify"
        );
    }

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
            .collect()
    }
}
