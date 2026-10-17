use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use freya::prelude::*;
use oneclient_content::packages::ContentType;
use oneclient_content::packages::health::HealthIssue;

use crate::components::{Button, OverlayPopup};
use crate::hooks::{
    bundle_overrides_map, bundles_with_status_items, cluster_content_items, package_updates,
    stale_hashes, use_bundle_overrides, use_bundles_with_status, use_cluster_content,
    use_mod_health, use_mods_folder_sync, use_package_updates, use_shadowed_mods,
};
use crate::layout::cluster_content;
use crate::theme::colors;
use crate::ui::border_all_color;

use super::cluster_not_found;

use super::package_manager::{
    PackageManager, bundle_categories, bundle_packages, stale_page_updates, use_content_meta,
};
use crate::hooks::use_cluster;

#[derive(PartialEq)]
pub struct ClusterMods {
    pub cluster_id: i64,
}

impl Component for ClusterMods {
    fn render(&self) -> impl IntoElement {
        let content = use_cluster_content(self.cluster_id, ContentType::Mod);
        let bundles = use_bundles_with_status(self.cluster_id);
        let overrides = use_bundle_overrides(self.cluster_id);
        let updates = use_package_updates(self.cluster_id);
        let shadowed = use_shadowed_mods(self.cluster_id);
        let health = use_mod_health(self.cluster_id);
        let bundle_items = bundles_with_status_items(&bundles);
        let content_items = cluster_content_items(&content);
        let meta = use_content_meta(&content_items, &bundle_items, ContentType::Mod);

        let Some(cluster) = use_cluster(self.cluster_id) else {
            return cluster_not_found();
        };

        let all_categories = bundle_categories(&bundle_items);
        let mut items = bundle_packages(
            content_items,
            &bundle_items,
            &bundle_overrides_map(&overrides),
            &meta,
            &stale_hashes(&updates),
            ContentType::Mod,
        );
        for item in &mut items {
            let Some(hash) = &item.hash else {
                continue;
            };
            item.shadowed = shadowed.shadowed.contains(hash);
            item.outranked = shadowed.outranked.contains(hash);
        }
        let update_all = stale_page_updates(&items, &package_updates(&updates));
        cluster_content()
            .child(ModsFolderSync {
                cluster_id: self.cluster_id,
                folder: oneclient_common::paths::cluster_mods_dir(&cluster.folder_name).ok(),
            })
            .child(ModHealthBanner { issues: health })
            .child(
                PackageManager::new(
                    "Mods",
                    "mods",
                    "mod",
                    ContentType::Mod,
                    self.cluster_id,
                    items,
                    all_categories,
                    oneclient_common::paths::cluster_mods_dir(&cluster.folder_name).ok(),
                    update_all,
                )
                .into_element(),
            )
            .into_element()
    }
}

#[derive(PartialEq)]
struct ModsFolderSync {
    cluster_id: i64,
    folder: Option<PathBuf>,
}

impl Component for ModsFolderSync {
    fn render(&self) -> impl IntoElement {
        use_mods_folder_sync(self.cluster_id, self.folder.clone());
        rect()
    }

    fn render_key(&self) -> DiffKey {
        let mut hasher = DefaultHasher::new();
        (self.cluster_id, &self.folder).hash(&mut hasher);
        DiffKey::U64(hasher.finish())
    }
}

/// Warns about problems that are provable from the jars in the mods folder
/// alone — duplicate mod ids, wrong-loader jars, missing required
/// dependencies — before the game gets a chance to crash on them.
#[derive(PartialEq)]
struct ModHealthBanner {
    issues: Arc<Vec<HealthIssue>>,
}

impl Component for ModHealthBanner {
    fn render(&self) -> impl IntoElement {
        let mut open = use_state(|| false);
        let count = self.issues.len();
        let issues = self.issues.clone();
        let mut close_open = open;

        rect()
            .vertical()
            .width(Size::fill())
            .maybe_child((count > 0).then(|| {
                rect()
                    .horizontal()
                    .width(Size::fill())
                    .spacing(10.)
                    .cross_align(Alignment::Center)
                    .padding(Gaps::new_symmetric(10., 14.))
                    .corner_radius(CornerRadius::new_all(12.))
                    .background(colors::danger().with_a(20))
                    .border(border_all_color(1., colors::danger().with_a(120)))
                    .child(
                        Icon::new(IconType::AlertTriangle)
                            .size(16.)
                            .color(colors::danger()),
                    )
                    .child(
                        rect()
                            .vertical()
                            .width(Size::flex(1.))
                            .child(
                                label()
                                    .text(format!(
                                        "{count} mod health {} found in this instance",
                                        if count == 1 { "issue" } else { "issues" }
                                    ))
                                    .font_size(13.)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(colors::fg_primary()),
                            )
                            .child(
                                label()
                                    .text("Provable from the mods folder alone; the game may crash or silently ignore jars.")
                                    .font_size(11.)
                                    .color(colors::fg_secondary()),
                            ),
                    )
                    .child(
                        Button::new()
                            .danger()
                            .small()
                            .on_press(move |_| open.set(true))
                            .text("Review"),
                    )
                    .into_element()
            }))
            .maybe_child((*open.read()).then(|| {
                ModHealthModal {
                    issues,
                    on_close: EventHandler::new(move |()| close_open.set(false)),
                }
                .into_element()
            }))
    }
}

#[derive(PartialEq)]
struct ModHealthModal {
    issues: Arc<Vec<HealthIssue>>,
    on_close: EventHandler<()>,
}

impl Component for ModHealthModal {
    fn render(&self) -> impl IntoElement {
        let close_scrim = self.on_close.clone();
        let close_button = self.on_close.clone();

        OverlayPopup::new()
            .on_close(move |()| close_scrim.call(()))
            .child(
                rect()
                    .width(Size::window_percent(100.))
                    .height(Size::window_percent(100.))
                    .center()
                    .child(
                        rect()
                            .vertical()
                            .width(Size::px(560.))
                            .max_width(Size::window_percent(92.))
                            .max_height(Size::window_percent(84.))
                            .spacing(14.)
                            .padding(Gaps::new_all(20.))
                            .corner_radius(CornerRadius::new_all(16.))
                            .background(colors::page_elevated())
                            .border(border_all_color(1., colors::component_border()))
                            .child(
                                label()
                                    .text("Mod health")
                                    .font_size(18.)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(colors::fg_primary()),
                            )
                            .child(
                                label()
                                    .text("These problems are readable from the jars in the mods folder alone, without launching the game.")
                                    .font_size(12.)
                                    .color(colors::fg_secondary()),
                            )
                            .child(
                                ScrollView::new()
                                    .width(Size::fill())
                                    .height(Size::Inner)
                                    .max_height(Size::px(420.))
                                    .child(
                                        rect().vertical().width(Size::fill()).spacing(8.).children(
                                            self.issues
                                                .iter()
                                                .map(|issue| issue_row(issue).into_element()),
                                        ),
                                    ),
                            )
                            .child(
                                rect()
                                    .horizontal()
                                    .width(Size::fill())
                                    .main_align(Alignment::End)
                                    .child(
                                        Button::new()
                                            .secondary()
                                            .on_press(move |_| close_button.call(()))
                                            .text("Close"),
                                    ),
                            ),
                    ),
            )
    }
}

fn issue_row(issue: &HealthIssue) -> impl IntoElement {
    rect()
        .vertical()
        .width(Size::fill())
        .spacing(4.)
        .padding(Gaps::new_all(10.))
        .corner_radius(CornerRadius::new_all(10.))
        .background(colors::component_bg())
        .border(border_all_color(1., colors::component_border()))
        .child(
            label()
                .text(issue.title())
                .font_size(13.)
                .font_weight(FontWeight::MEDIUM)
                .color(colors::fg_primary()),
        )
        .child(
            label()
                .text(issue.detail())
                .font_size(11.)
                .color(colors::fg_secondary()),
        )
}
