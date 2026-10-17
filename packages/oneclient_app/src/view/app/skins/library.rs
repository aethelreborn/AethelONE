use std::path::{Path, PathBuf};

use base64::Engine as _;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

const MAX_SKIN_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinKind {
    File,
    Url,
    Name,
}

/// Which of the three local textures an entry is. CustomSkinLoader reads them
/// all from the LocalSkin folder, so the library manages them the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextureKind {
    #[default]
    Skin,
    Cape,
    Elytra,
}

impl TextureKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::Cape => "cape",
            Self::Elytra => "elytra",
        }
    }

    #[must_use]
    pub fn subdir(self) -> &'static str {
        match self {
            Self::Skin => "skins",
            Self::Cape => "capes",
            Self::Elytra => "elytras",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinEntry {
    pub id: String,
    pub name: String,
    pub origin: String,
    pub kind: SkinKind,
    pub slim: bool,
    /// Absent in libraries written before capes and elytras existed
    #[serde(default)]
    pub texture: TextureKind,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub skins: Vec<SkinEntry>,
    pub active: Option<String>,
    #[serde(default)]
    pub active_cape: Option<String>,
    #[serde(default)]
    pub active_elytra: Option<String>,
}

fn skins_dir() -> Result<PathBuf, String> {
    let dir = oneclient_common::paths::data_dir().map_err(|err| err.to_string())?;
    Ok(dir.join("skins"))
}

fn library_path() -> Result<PathBuf, String> {
    Ok(skins_dir()?.join("library.json"))
}

fn entry_path(id: &str) -> Result<PathBuf, String> {
    Ok(skins_dir()?.join(format!("{id}.png")))
}

impl Library {
    pub fn load() -> Self {
        let Ok(path) = library_path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = skins_dir()?;
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let raw = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        std::fs::write(dir.join("library.json"), raw).map_err(|err| err.to_string())
    }

    pub fn add(
        &mut self,
        name: String,
        origin: String,
        kind: SkinKind,
        texture: TextureKind,
        slim: bool,
        bytes: &[u8],
    ) -> Result<String, String> {
        let dir = skins_dir()?;
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;

        let id = uuid::Uuid::new_v4().simple().to_string();
        std::fs::write(dir.join(format!("{id}.png")), bytes).map_err(|err| err.to_string())?;

        self.skins.insert(
            0,
            SkinEntry {
                id: id.clone(),
                name,
                origin,
                kind,
                slim,
                texture,
            },
        );
        self.save()?;
        Ok(id)
    }

    pub fn remove(&mut self, id: &str) {
        let texture = self.entry(id).map(|entry| entry.texture);
        self.skins.retain(|entry| entry.id != id);
        if let Some(texture) = texture {
            let slot = self.active_slot_mut(texture);
            if slot.as_deref() == Some(id) {
                *slot = None;
            }
        }
        if let Ok(path) = entry_path(id) {
            let _ = std::fs::remove_file(path);
        }
        let _ = self.save();
    }

    /// Equips the entry in the slot that matches its texture kind.
    pub fn set_active(&mut self, id: Option<String>) {
        let texture = id
            .as_deref()
            .and_then(|id| self.entry(id))
            .map(|entry| entry.texture)
            .unwrap_or_default();
        *self.active_slot_mut(texture) = id;
        let _ = self.save();
    }

    fn active_slot_mut(&mut self, texture: TextureKind) -> &mut Option<String> {
        match texture {
            TextureKind::Skin => &mut self.active,
            TextureKind::Cape => &mut self.active_cape,
            TextureKind::Elytra => &mut self.active_elytra,
        }
    }

    fn active_slot(&self, texture: TextureKind) -> &Option<String> {
        match texture {
            TextureKind::Skin => &self.active,
            TextureKind::Cape => &self.active_cape,
            TextureKind::Elytra => &self.active_elytra,
        }
    }

    #[must_use]
    pub fn active_for(&self, texture: TextureKind) -> Option<&str> {
        self.active_slot(texture).as_deref()
    }

    /// Every entry of one texture kind, newest first, for the library grid
    #[must_use]
    pub fn entries_for(&self, texture: TextureKind) -> Vec<SkinEntry> {
        self.skins
            .iter()
            .filter(|entry| entry.texture == texture)
            .cloned()
            .collect()
    }

    /// Overrides the model guessed at import time when detection picked the
    /// wrong arm width. Returns whether anything changed.
    pub fn set_slim(&mut self, id: &str, slim: bool) -> bool {
        let Some(entry) = self.skins.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        if entry.slim == slim {
            return false;
        }
        entry.slim = slim;
        let _ = self.save();
        true
    }

    pub fn entry(&self, id: &str) -> Option<&SkinEntry> {
        self.skins.iter().find(|entry| entry.id == id)
    }

    pub fn entry_bytes(&self, id: &str) -> Option<(SkinEntry, Bytes)> {
        let entry = self.entry(id)?;
        let path = entry_path(id).ok()?;
        let bytes = std::fs::read(path).ok()?;
        Some((entry.clone(), Bytes::from(bytes)))
    }

    pub fn active_bytes(&self) -> Option<(SkinEntry, Bytes)> {
        self.active_bytes_for(TextureKind::Skin)
    }

    pub fn active_bytes_for(&self, texture: TextureKind) -> Option<(SkinEntry, Bytes)> {
        let id = self.active_slot(texture).as_deref()?;
        self.entry_bytes(id)
    }
}

/// Keeps only real 64x64/64x32 PNG skins and guesses the arm width when the
/// source carried no model metadata (the (43, 20) base-arm column and, on
/// 64x64 sheets, the (57, 20) sleeve column are empty on slim skins).
pub fn validate_skin(bytes: &[u8]) -> Result<bool, String> {
    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(PNG_MAGIC) {
        return Err("Only PNG skin files are supported".to_string());
    }

    let image = image::load_from_memory(bytes)
        .map_err(|_| "That file is not a readable PNG image".to_string())?;
    use image::GenericImageView as _;
    let (width, height) = image.dimensions();

    if width != 64 || (height != 32 && height != 64) {
        return Err(format!(
            "Skins must be 64x64 or 64x32 pixels, this image is {width}x{height}"
        ));
    }

    let rgba = image.to_rgba8();
    let slim = rgba.get_pixel(43, 20)[3] == 0 || (height == 64 && rgba.get_pixel(57, 20)[3] == 0);
    Ok(slim)
}

/// Capes are 64x32 on modern versions; 22x17 files from the very old days are
/// still worn by some accounts, so they are accepted too.
pub fn validate_cape(bytes: &[u8]) -> Result<(), String> {
    validate_dimensions(bytes, &[(64, 32), (22, 17)], "Capes")
}

/// Elytras share the cape layout and have no legacy size.
pub fn validate_elytra(bytes: &[u8]) -> Result<(), String> {
    validate_dimensions(bytes, &[(64, 32)], "Elytras")
}

/// The one validation the UI routes every import through; the bool is the
/// slim-arm guess and only means anything for skins.
pub fn validate_texture(bytes: &[u8], texture: TextureKind) -> Result<bool, String> {
    match texture {
        TextureKind::Skin => validate_skin(bytes),
        TextureKind::Cape => validate_cape(bytes).map(|()| false),
        TextureKind::Elytra => validate_elytra(bytes).map(|()| false),
    }
}

fn validate_dimensions(bytes: &[u8], allowed: &[(u32, u32)], what: &str) -> Result<(), String> {
    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(PNG_MAGIC) {
        return Err("Only PNG files are supported".to_string());
    }

    let image = image::load_from_memory(bytes)
        .map_err(|_| "That file is not a readable PNG image".to_string())?;
    use image::GenericImageView as _;
    let (width, height) = image.dimensions();

    if !allowed.contains(&(width, height)) {
        let sizes = allowed
            .iter()
            .map(|(w, h)| format!("{w}x{h}"))
            .collect::<Vec<_>>()
            .join(" or ");
        return Err(format!(
            "{what} must be {sizes} pixels, this image is {width}x{height}"
        ));
    }
    Ok(())
}

pub async fn fetch_url(raw: &str, texture: TextureKind) -> Result<(Bytes, bool), String> {
    let url = raw.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!(
            "Enter an http(s) link to a {} PNG",
            texture.label()
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("download failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("download failed: {err}"))?;

    if response
        .content_length()
        .is_some_and(|len| len > MAX_SKIN_BYTES as u64)
    {
        return Err("That file is too large to be a skin".to_string());
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("download failed: {err}"))?;
    if bytes.len() > MAX_SKIN_BYTES {
        return Err("That file is too large to be a skin".to_string());
    }

    let slim = validate_texture(&bytes, texture)?;
    Ok((bytes, slim))
}

/// Mojang names are 3-16 ASCII letters, digits or underscores; anything else
/// cannot resolve, so the lookup is skipped before any network call.
fn valid_mojang_name(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Resolves a Mojang player name to their current skin, exactly like the
/// official client would fetch it.
pub async fn fetch_by_name(raw: &str) -> Result<(Bytes, bool, String), String> {
    let name = raw.trim();
    if !valid_mojang_name(name) {
        return Err("Mojang names are 3-16 letters, numbers or underscores".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;

    let profile = client
        .get(format!(
            "https://api.mojang.com/users/profiles/minecraft/{name}"
        ))
        .send()
        .await
        .map_err(|err| format!("name lookup failed: {err}"))?;
    if profile.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("No Mojang account is named \"{name}\""));
    }
    let profile: serde_json::Value = profile
        .error_for_status()
        .map_err(|err| format!("name lookup failed: {err}"))?
        .json()
        .await
        .map_err(|err| format!("name lookup failed: {err}"))?;

    let canonical = profile
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or(name)
        .to_string();
    let uuid = profile
        .get("id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Mojang returned no profile id".to_string())?;

    let session: serde_json::Value = client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false"
        ))
        .send()
        .await
        .map_err(|err| format!("profile lookup failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("profile lookup failed: {err}"))?
        .json()
        .await
        .map_err(|err| format!("profile lookup failed: {err}"))?;

    let textures = session
        .get("properties")
        .and_then(|value| value.as_array())
        .and_then(|properties| {
            properties
                .iter()
                .find(|property| property.get("name").and_then(|n| n.as_str()) == Some("textures"))
        })
        .and_then(|property| property.get("value"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| "That profile has no skin".to_string())?;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(textures)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(textures))
        .map_err(|err| err.to_string())?;
    let textures: serde_json::Value =
        serde_json::from_slice(&decoded).map_err(|err| err.to_string())?;

    let skin = textures
        .get("textures")
        .and_then(|value| value.get("SKIN"))
        .ok_or_else(|| "That profile has no skin".to_string())?;
    let skin_url = skin
        .get("url")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "That profile has no skin".to_string())?;
    let model = skin
        .get("metadata")
        .and_then(|value| value.get("model"))
        .and_then(|value| value.as_str());

    let bytes = client
        .get(skin_url)
        .send()
        .await
        .map_err(|err| format!("skin download failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("skin download failed: {err}"))?
        .bytes()
        .await
        .map_err(|err| format!("skin download failed: {err}"))?;

    let detected = validate_skin(&bytes)?;
    let slim = match model {
        Some("slim") => true,
        Some(_) => false,
        None => detected,
    };
    Ok((bytes, slim, canonical))
}

pub fn file_display_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("Imported skin")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn png(width: u32, height: u32, transparent_left_arm_overlay: bool) -> Vec<u8> {
        let mut img = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        if transparent_left_arm_overlay {
            img.put_pixel(43, 20, image::Rgba([0, 0, 0, 0]));
        }
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("png encoding must work");
        buf.into_inner()
    }

    #[test]
    fn accepts_modern_skin_and_flags_classic_arms() {
        let skin = png(64, 64, false);
        assert_eq!(validate_skin(&skin), Ok(false));
    }

    #[test]
    fn detects_slim_arms_from_the_transparent_overlay() {
        let skin = png(64, 64, true);
        assert_eq!(validate_skin(&skin), Ok(true));
    }

    #[test]
    fn accepts_legacy_64_by_32_skin() {
        let skin = png(64, 32, false);
        assert_eq!(validate_skin(&skin), Ok(false));
    }

    #[test]
    fn rejects_files_that_are_not_png_skins() {
        assert!(validate_skin(b"not a png").is_err());

        let too_small = png(32, 32, false);
        assert!(validate_skin(&too_small).is_err());
    }

    #[test]
    fn accepts_only_plausible_mojang_names() {
        assert!(valid_mojang_name("Darkie_Krish"));
        assert!(valid_mojang_name("abc"));
        assert!(valid_mojang_name("sixteen_chars_ok"));

        assert!(!valid_mojang_name("ab"));
        assert!(!valid_mojang_name("seventeen_chars_x"));
        assert!(!valid_mojang_name("darkie krish"));
        assert!(!valid_mojang_name("darkié"));
        assert!(!valid_mojang_name(""));
    }

    #[test]
    fn derives_a_display_name_from_the_file_stem() {
        assert_eq!(file_display_name("my skin.png"), "my skin");
        assert_eq!(file_display_name("archive.tar.gz"), "archive.tar");
        assert_eq!(file_display_name("skin"), "skin");
        assert_eq!(file_display_name(""), "Imported skin");
    }

    #[test]
    fn libraries_written_before_capes_still_load() {
        let raw = r#"{
            "skins": [
                {"id":"a","name":"Old","origin":"old.png","kind":"file","slim":false}
            ],
            "active": "a"
        }"#;
        let library: Library = serde_json::from_str(raw).expect("legacy library must parse");
        assert_eq!(library.skins[0].texture, TextureKind::Skin);
        assert_eq!(library.active.as_deref(), Some("a"));
        assert_eq!(library.active_cape, None);
        assert_eq!(library.active_elytra, None);
    }

    #[test]
    fn equip_and_remove_route_through_the_entries_texture_kind() {
        let mut library = Library::default();
        library.skins.push(SkinEntry {
            id: "c1".into(),
            name: "Cape".into(),
            origin: "cape.png".into(),
            kind: SkinKind::File,
            slim: false,
            texture: TextureKind::Cape,
        });
        library.skins.push(SkinEntry {
            id: "s1".into(),
            name: "Skin".into(),
            origin: "skin.png".into(),
            kind: SkinKind::File,
            slim: true,
            texture: TextureKind::Skin,
        });

        library.set_active(Some("c1".into()));
        assert_eq!(library.active_cape.as_deref(), Some("c1"));
        assert_eq!(library.active, None, "a cape must not take the skin slot");

        library.set_active(Some("s1".into()));
        assert_eq!(library.active.as_deref(), Some("s1"));
        assert_eq!(library.active_cape.as_deref(), Some("c1"));

        library.remove("c1");
        assert_eq!(library.active_cape, None, "removing the cape un-equips it");
        assert_eq!(library.active.as_deref(), Some("s1"));
    }

    #[test]
    fn entries_are_grouped_by_texture_kind() {
        let mut library = Library::default();
        for (id, texture) in [
            ("s1", TextureKind::Skin),
            ("c1", TextureKind::Cape),
            ("e1", TextureKind::Elytra),
        ] {
            library.skins.push(SkinEntry {
                id: id.into(),
                name: id.into(),
                origin: id.into(),
                kind: SkinKind::File,
                slim: false,
                texture,
            });
        }

        let capes = library.entries_for(TextureKind::Cape);
        assert_eq!(capes.len(), 1);
        assert_eq!(capes[0].id, "c1");
        assert_eq!(library.entries_for(TextureKind::Skin).len(), 1);
    }

    #[test]
    fn cape_validation_accepts_modern_and_legacy_sizes() {
        assert_eq!(validate_cape(&png(64, 32, false)), Ok(()));
        let legacy = {
            let mut img = image::RgbaImage::from_pixel(22, 17, image::Rgba([0, 0, 255, 255]));
            let mut buf = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut buf, image::ImageFormat::Png)
                .expect("png encoding must work");
            buf.into_inner()
        };
        assert_eq!(validate_cape(&legacy), Ok(()));

        assert!(validate_cape(&png(64, 64, false)).is_err());
        assert!(validate_cape(b"not a png").is_err());
    }

    #[test]
    fn elytra_validation_only_accepts_the_cape_layout() {
        assert_eq!(validate_elytra(&png(64, 32, false)), Ok(()));
        assert!(validate_elytra(&png(64, 64, false)).is_err());
    }

    #[test]
    fn texture_validation_reports_slim_only_for_skins() {
        assert_eq!(
            validate_texture(&png(64, 64, true), TextureKind::Skin),
            Ok(true)
        );
        assert_eq!(
            validate_texture(&png(64, 32, false), TextureKind::Cape),
            Ok(false)
        );
        assert_eq!(
            validate_texture(&png(64, 32, false), TextureKind::Elytra),
            Ok(false)
        );
        assert!(validate_texture(&png(64, 64, false), TextureKind::Cape).is_err());
    }
}
