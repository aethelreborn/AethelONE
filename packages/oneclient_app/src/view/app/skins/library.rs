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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinEntry {
    pub id: String,
    pub name: String,
    pub origin: String,
    pub kind: SkinKind,
    pub slim: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub skins: Vec<SkinEntry>,
    pub active: Option<String>,
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
            },
        );
        self.save()?;
        Ok(id)
    }

    pub fn remove(&mut self, id: &str) {
        self.skins.retain(|entry| entry.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
        if let Ok(path) = entry_path(id) {
            let _ = std::fs::remove_file(path);
        }
        let _ = self.save();
    }

    pub fn set_active(&mut self, id: Option<String>) {
        self.active = id;
        let _ = self.save();
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
        let id = self.active.as_deref()?;
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

pub async fn fetch_url(raw: &str) -> Result<(Bytes, bool), String> {
    let url = raw.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Enter an http(s) link to a skin PNG".to_string());
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

    let slim = validate_skin(&bytes)?;
    Ok((bytes, slim))
}

/// Resolves a Mojang player name to their current skin, exactly like the
/// official client would fetch it.
pub async fn fetch_by_name(raw: &str) -> Result<(Bytes, bool, String), String> {
    let name = raw.trim();
    let valid = (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !valid {
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
        let mut img =
            image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
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
}
