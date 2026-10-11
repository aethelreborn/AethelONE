use bytes::Bytes;
use freya::prelude::*;
use freya::query::{QueriesStorage, Query, QueryCapability, UseQuery, use_query};
use oneclient_core::LauncherError;

use crate::AppAssets;
use crate::view::app::skins::library::{Library, SkinEntry};

pub fn use_player_skin(uuid: String) -> (Bytes, bool) {
    let profile = super::use_player_profile(uuid.clone(), None::<String>);
    let local = use_local_active_skin();

    let (skin_url, is_slim) = crate::hooks::settled_or_loading(&profile)
        .map_or((None, false), |profile| (profile.skin_url, profile.is_slim));

    let skin_query = super::use_cached_image(skin_url.clone(), 256);

    let steve = use_memo(|| AppAssets::get_bytes("steve.png").unwrap_or_default());
    let alex = use_memo(|| AppAssets::get_bytes("alex.png").unwrap_or_default());

    let default_slim = (java_string_hash(&uuid) & 1) == 1;

    match crate::hooks::loaded_image(skin_url.as_deref(), &skin_query) {
        Some((_, bytes)) => (bytes, is_slim),
        None => {
            // A profile still in flight may well carry a skin, so only fall
            // back once it has settled (offline and alt accounts settle as an
            // error or without a skin url)
            let equipped = if crate::hooks::query_is_busy(&profile) {
                None
            } else {
                crate::hooks::settled_or_loading(&local).flatten()
            };
            match equipped {
                Some((entry, bytes)) => (bytes, entry.slim),
                None if default_slim => (alex.read().clone(), true),
                None => (steve.read().clone(), false),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalSkinKeys;

/// The skin equipped on the Skins page, read from the local library so the
/// avatar can show it for accounts that have no Mojang skin of their own
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalSkinQuery;

impl QueryCapability for LocalSkinQuery {
    type Ok = Option<(SkinEntry, Bytes)>;
    type Err = LauncherError;
    type Keys = LocalSkinKeys;

    async fn run(&self, _keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let library = crate::launcher::off_ui_blocking(Library::load).await;
        Ok(library.active_bytes())
    }
}

pub fn use_local_active_skin() -> UseQuery<LocalSkinQuery> {
    use_query(Query::new(LocalSkinKeys, LocalSkinQuery))
}

/// Called after the library changes so every avatar re-reads the active skin
pub async fn invalidate_local_skin_queries() {
    QueriesStorage::<LocalSkinQuery>::invalidate_all().await;
}

fn java_string_hash(s: &str) -> i32 {
    let mut h: i32 = 0;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(c as i32);
    }
    h
}
