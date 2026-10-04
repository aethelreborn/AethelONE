pub const WINDOW_APP_ID: &str = "oneclient_app";
pub const WINDOW_TITLE: &str = "OneClient";

pub const UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDFGODk3MkMyMjg0MjFDMDUKUldRRkhFSW93bktKSHpkWjNEMXNzaDVINVpCTU8xSnhuK2RnV0dTZ2FkcFJWbG1zUkhGYTNjaUkK";
pub const UPDATER_ENDPOINT: &str =
    "https://github.com/aethelreborn/AethelONE/releases/latest/download/latest.json";
pub const RELEASES_URL: &str = "https://github.com/aethelreborn/AethelONE/releases/latest";
/// Release assets live under `/releases/download/oneclient-<version>/`; the
/// `.deb`/`.rpm` updater path builds its download URLs from this base.
pub const RELEASES_DOWNLOAD_BASE: &str =
    "https://github.com/aethelreborn/AethelONE/releases/download";
