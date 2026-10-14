use freya::prelude::*;

use super::settings_page;
use crate::components::{IconType, TextInput};
use crate::hooks::{use_dispatch, use_settings_snapshot};
use crate::view::app::settings::{resettable, section_header, settings_row, settings_row_focus};

fn normalize(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Empty or non-numeric input both mean "unlimited" so a half-typed value can
/// never leave downloads stalled at a nonsense cap
fn parse_speed_limit(value: &str) -> u64 {
    value.trim().parse().unwrap_or(0)
}

#[derive(PartialEq)]
pub struct SettingsApis;

impl Component for SettingsApis {
    fn render(&self) -> impl IntoElement {
        let settings = use_settings_snapshot().settings;
        let dispatch = use_dispatch();

        let modrinth_key = use_state({
            let v = settings.modrinth_api_key.clone().unwrap_or_default();
            move || v
        });
        let curseforge_key = use_state({
            let v = settings.curseforge_api_key.clone().unwrap_or_default();
            move || v
        });
        let custom_api_endpoint = use_state({
            let v = settings.custom_api_endpoint.clone().unwrap_or_default();
            move || v
        });
        let custom_meta_url_base = use_state({
            let v = settings.custom_meta_url_base.clone().unwrap_or_default();
            move || v
        });
        let download_speed_limit = use_state({
            let v = if settings.download_speed_limit_kbps == 0 {
                String::new()
            } else {
                settings.download_speed_limit_kbps.to_string()
            };
            move || v
        });

        let mut first = use_state(|| true);
        use_side_effect(move || {
            let modrinth = modrinth_key.read().clone();
            let curseforge = curseforge_key.read().clone();
            let endpoint = custom_api_endpoint.read().clone();
            let meta_url_base = custom_meta_url_base.read().clone();
            let speed_limit = download_speed_limit.read().clone();
            if *first.peek() {
                first.set(false);
                return;
            }
            dispatch.edit_settings(|settings| {
                settings.modrinth_api_key = normalize(&modrinth);
                settings.curseforge_api_key = normalize(&curseforge);
                settings.custom_api_endpoint = normalize(&endpoint);
                settings.custom_meta_url_base = normalize(&meta_url_base);
                settings.download_speed_limit_kbps = parse_speed_limit(&speed_limit);
            });
        });

        settings_page()
            .child(section_header("PROVIDERS"))
            .child(settings_row_focus(
                "apis.modrinth_key",
                IconType::Key01,
                "Modrinth",
                "Personal access token used for Modrinth requests.",
                resettable(
                    TextInput::new(modrinth_key)
                        .placeholder("Optional")
                        .width(Size::px(220.)),
                    modrinth_key,
                    String::new(),
                ),
            ))
            .child(settings_row_focus(
                "apis.curseforge_key",
                IconType::Key01,
                "CurseForge",
                "API key used for CurseForge requests.",
                resettable(
                    TextInput::new(curseforge_key)
                        .placeholder("Default")
                        .width(Size::px(220.)),
                    curseforge_key,
                    String::new(),
                ),
            ))
            .child(section_header("ADVANCED"))
            .child(settings_row_focus(
                "apis.custom_endpoint",
                IconType::Globe01,
                "Custom API Endpoint",
                "Override the default AethelONE backend endpoint.",
                resettable(
                    TextInput::new(custom_api_endpoint)
                        .placeholder("Default")
                        .width(Size::px(220.)),
                    custom_api_endpoint,
                    String::new(),
                ),
            ))
            .child(settings_row_focus(
                "apis.custom_meta_url_base",
                IconType::Globe01,
                "Custom Meta URL Base",
                "Override the default bundles and versions data host.",
                resettable(
                    TextInput::new(custom_meta_url_base)
                        .placeholder("Default")
                        .width(Size::px(220.)),
                    custom_meta_url_base,
                    String::new(),
                ),
            ))
            .child(section_header("DOWNLOADS"))
            .child(settings_row_focus(
                "apis.download_speed_limit",
                IconType::Download01,
                "Download Speed Limit",
                "Cap download bandwidth in kilobytes per second; leave empty for unlimited.",
                resettable(
                    TextInput::new(download_speed_limit)
                        .placeholder("Unlimited")
                        .width(Size::px(220.)),
                    download_speed_limit,
                    String::new(),
                ),
            ))
            .into_element()
    }
}
