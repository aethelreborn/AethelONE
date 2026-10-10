pub const DUMMY_REPLACE_NEWLINE: &str = "\n";

pub const MICROSOFT_CLIENT_ID: &str = "9419b7ee-1448-4d1b-b52a-550d8f36ab56";
pub const MINECRAFT_SCOPES: &str = "XboxLive.SignIn XboxLive.offline_access";
pub const CURSEFORGE_API_KEY: &str = "$2a$10$6utA1UNSmFPrE/Lh7b7ndeeGmiOkjKNY8kpFB0fsmE/d42ZAfFgCe";
pub const DISCORD_CLIENT_ID: &str = "1426999264633946334";

pub const MODRINTH_API_URL: &str = "https://api.modrinth.com";
pub const MODRINTH_CDN_PREFIX: &str = "https://cdn.modrinth.com/data/";
pub const CURSEFORGE_API_URL: &str = "https://api.curseforge.com/v1";
pub const CURSEFORGE_GAME_ID: u32 = 432;
pub const METADATA_API_URL: &str = "https://meta.polyfrost.org";
pub const MCLOGS_API_URL: &str = "https://api.mclo.gs/1";
pub const SKYCLIENT_BASE_URL: &str =
    "https://raw.githubusercontent.com/SkyblockClient/SkyblockClient-REPO/refs/heads/main/v1";
pub const META_URL_BASE: &str = "https://data-v2.polyfrost.org";
pub const TOS_URL: &str = "https://polyfrost.org/legal/terms";
pub const PRIVACY_URL: &str = "https://polyfrost.org/legal/privacy";
pub const PLUS_BACKEND_URL: &str = "https://plus.polyfrost.org";

/// Modrinth project id of PolyPlus (shown as "PolyPlus+").
pub const POLYPLUS_MODRINTH_PROJECT_ID: &str = "Iw9mZi4a";
/// Rolling manifest of AethelOnePLUS-built PolyPlus jars (cracked-uuid badge
/// patch) published by the `build.yml` release job. Downloads and bundle
/// update checks consult it and fall back to the official Modrinth file
/// whenever it is unreachable or has no entry for the requested mc version.
pub const POLYPLUS_MIRROR_MANIFEST_URL: &str =
    "https://github.com/aethelreborn/AethelOnePLUS/releases/latest/download/polyplus-mirror.json";

/// Loopback endpoint of the built-in Poly+ cosmetics proxy
/// (`oneclient_app::cosmetics_proxy`); the port is what gets bound and what
/// `-Dpolyplus.apiUrl` points the game at.
pub const COSMETICS_PROXY_PORT: u16 = 8777;
pub const COSMETICS_PROXY_URL: &str = "http://127.0.0.1:8777";
/// Probed before injection so a foreign process squatting on the port is never
/// mistaken for the proxy.
pub const COSMETICS_PROXY_PROBE: &str = "/__oneclient_proxy";
/// Header (and value) the proxy stamps on locally-answered responses.
pub const COSMETICS_PROXY_MARKER: &str = "x-oneclient-cosmetics";
pub const COSMETICS_PROXY_MARKER_VALUE: &str = "aethelone";

pub const SENTRY_DSN: &str = match option_env!("ONECLIENT_SENTRY_DSN") {
    Some(dsn) => dsn,
    None => {
        "https://e7dff7e07427e1a28b9212cfcc8ddc1e@o4511714343124992.ingest.us.sentry.io/4511714354135040"
    }
};

pub const TARGET_OS: &str = cfg_select! {
    target_os = "windows" => "windows",
    target_os = "macos" => "osx",
    target_os = "linux" => "linux"
};

pub const NATIVE_ARCH: &str = cfg_select! {
    target_arch = "x86" => "32",
    target_arch = "x86_64" => "64",
    _ => "64"
};

pub const JAVA_BIN: &str = if cfg!(windows) { "javaw.exe" } else { "java" };

pub const ARCH_WIDTH: &str = if cfg!(target_pointer_width = "64") {
    "64"
} else {
    "32"
};

pub const LINE_ENDING: &str = if cfg!(windows) { "\r\n" } else { "\n" };
pub const CLASSPATH_SEPARATOR: &str = if cfg!(windows) { ";" } else { ":" };
