use std::path::PathBuf;
use std::sync::Arc;

use freya::prelude::*;
use oneclient_cluster::naming::{MAX_NAME_CHARS, validate_name};
use oneclient_common::domain::GameLoader;
use oneclient_core::clusters::{Cluster, ClusterKind};

use super::cards::{VERSION_ROW_H, VERSION_ROW_SPACING, version_row};
use super::details::{DetailsState, details_body};
use super::model::VersionList;
use super::rail::{Rail, facts_card, rail, version_art};
use super::shell::{Shell, shell};
use crate::components::{Icon, IconType, ScrollArea, TextInput};
use crate::hooks::{
    ClusterAction, game_versions, settled_or_loading, use_cluster_mutation, use_game_versions,
    use_loader_versions,
};
use crate::theme::colors;
use crate::ui::centered_note;

#[derive(Clone, PartialEq)]
pub struct InstanceFacts {
    pub cluster_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub cover: Option<PathBuf>,
    pub mc_version: String,
    pub mc_loader: GameLoader,
    pub kind: ClusterKind,
    pub modpack: bool,
}

impl InstanceFacts {
    pub fn from_cluster(cluster: &Cluster) -> Self {
        Self {
            cluster_id: cluster.id,
            name: cluster.name.clone(),
            description: cluster.description.clone(),
            tags: cluster.tags.clone(),
            cover: cluster.cover_file(),
            mc_version: cluster.mc_version.clone(),
            mc_loader: cluster.mc_loader,
            kind: cluster.kind,
            modpack: cluster.linked_modpack_hash.is_some(),
        }
    }
}

#[derive(PartialEq)]
pub struct EditInstanceModal {
    facts: InstanceFacts,
    on_close: EventHandler<()>,
}

impl EditInstanceModal {
    pub fn new(facts: InstanceFacts, on_close: impl Into<EventHandler<()>>) -> Self {
        Self {
            facts,
            on_close: on_close.into(),
        }
    }
}

fn name_limit(modpack: bool) -> Option<usize> {
    (!modpack).then_some(MAX_NAME_CHARS)
}

fn kind_label(kind: ClusterKind) -> &'static str {
    match kind {
        ClusterKind::OneClient => "AethelONE",
        ClusterKind::Vanilla => "Vanilla",
        ClusterKind::Modded => "Modded",
    }
}

impl Component for EditInstanceModal {
    fn render(&self) -> impl IntoElement {
        let mutation = use_cluster_mutation();
        let cluster_id = self.facts.cluster_id;

        let state = DetailsState::seeded(
            &self.facts.name,
            self.facts.description.as_deref(),
            &self.facts.tags,
        );

        let existing = self.facts.cover.clone();
        let preview = state.preview_cover(existing.clone());
        let tags = state.tags.read().clone();
        let typed = state.typed_name();
        let name_problem = if typed == self.facts.name {
            None
        } else {
            validate_name(&typed, name_limit(self.facts.modpack)).err()
        };
        let ready = !typed.is_empty() && name_problem.is_none();

        let description = state.description.read().trim().to_string();
        let subtitle = if description.is_empty() {
            format!(
                "{} · {}",
                kind_label(self.facts.kind),
                self.facts.mc_version
            )
        } else {
            description
        };

        let title = if typed.is_empty() {
            self.facts.name.clone()
        } else {
            typed.clone()
        };

        let art = version_art(Some(&self.facts.mc_version), Some(self.facts.mc_loader));
        let art = match &preview {
            Some((path, true)) => art.picked_cover(Some(path.clone())),
            Some((path, false)) => art.cover(Some(path.clone())),
            None => art,
        };

        let loader = self.facts.mc_loader.to_string();

        let close_x = self.on_close.clone();
        let close_cancel = self.on_close.clone();
        let close_save = self.on_close.clone();

        shell(Shell {
            rail: rail(Rail {
                art,
                title,
                subtitle,
                card: facts_card(
                    vec![
                        ("Type", kind_label(self.facts.kind).to_string()),
                        ("Version", self.facts.mc_version.clone()),
                        ("Loader", loader),
                    ],
                ),
                tags,
            }),
            eyebrow: Some("Instance details".to_string()),
            title: "Edit the instance".to_string(),
            subtitle: "Only how it is presented changes here. The version, loader and installed content stay as they are."
                .to_string(),
            body: details_body(
                state,
                self.facts.name.clone(),
                existing,
                version_art(Some(&self.facts.mc_version), Some(self.facts.mc_loader)),
                name_problem,
                name_limit(self.facts.modpack),
            ),
            scrolls_itself: false,
            note: "Changes apply straight away.".to_string(),
            secondary_label: "Cancel".to_string(),
            primary_label: "Save changes".to_string(),
            primary_enabled: ready,
            on_close: (move |()| close_x.call(())).into(),
            on_secondary: (move |()| close_cancel.call(())).into(),
            on_primary: (move |()| {
                mutation.mutate(ClusterAction::UpdateInstance {
                    cluster_id,
                    name: state.typed_name(),
                    description: state.description_value(),
                    tags: state.tags.read().clone(),
                    cover_source: state.cover.read().clone(),
                    clear_cover: *state.cover_cleared.read(),
                });
                close_save.call(());
            })
            .into(),
        })
    }
}

#[derive(PartialEq)]
pub struct ChangeVersionModal {
    facts: InstanceFacts,
    on_close: EventHandler<()>,
}

impl ChangeVersionModal {
    pub fn new(facts: InstanceFacts, on_close: impl Into<EventHandler<()>>) -> Self {
        Self {
            facts,
            on_close: on_close.into(),
        }
    }
}

impl Component for ChangeVersionModal {
    fn render(&self) -> impl IntoElement {
        let mutation = use_cluster_mutation();
        let cluster_id = self.facts.cluster_id;
        let current = self.facts.mc_version.clone();
        let loader = self.facts.mc_loader;

        let mut picked = use_state(|| None::<String>);
        let mut query = use_state(String::new);

        // Empty search shows the release line; anything else filters every build
        let all = game_versions(&use_game_versions());
        let needle = query.read().trim().to_lowercase();
        let visible: Arc<[u32]> = all
            .as_ref()
            .map(|all| {
                all.iter()
                    .enumerate()
                    .filter(|(_, entry)| {
                        if needle.is_empty() {
                            entry.kind.is_release()
                        } else {
                            entry.id.to_lowercase().contains(&needle)
                        }
                    })
                    .map(|(index, _)| index as u32)
                    .collect::<Vec<u32>>()
                    .into()
            })
            .unwrap_or_else(|| Arc::from([]));
        let list = all.clone().map(|all| VersionList::Catalogue {
            all,
            visible: visible.clone(),
        });

        let picked_version = picked.read().clone();
        let probe = picked_version.clone().unwrap_or_default();
        let loader_support = if loader == GameLoader::Vanilla || picked_version.is_none() {
            Some(true)
        } else {
            settled_or_loading(&use_loader_versions(probe, loader))
                .map(|versions| !versions.is_empty())
        };

        let changed = picked_version
            .as_deref()
            .is_some_and(|version| version != current);
        let ready = changed && loader_support == Some(true);

        let shown = picked_version.clone().unwrap_or_else(|| current.clone());
        let mods_warning = changed.then(|| {
            format!("Mods built for {current} may not load on {shown} — check them before playing.")
        });

        let body = rect()
            .vertical()
            .width(Size::fill())
            .height(Size::fill())
            .spacing(12.)
            .child(
                TextInput::new(query)
                    .placeholder("Search versions")
                    .width(Size::fill())
                    .leading(
                        Icon::new(IconType::SearchMd)
                            .size(14.)
                            .color(colors::fg_secondary())
                            .into_element(),
                    ),
            )
            .child(match list {
                None => centered_note("Loading versions..."),
                Some(list) if list.is_empty() => centered_note("No versions match that search."),
                Some(list) => {
                    let count = list.len();
                    let selected = picked_version.clone();
                    ScrollArea::new()
                        .width(Size::fill())
                        .height(Size::fill())
                        .scrollbar_gutter(true)
                        .lazy(count, VERSION_ROW_H, VERSION_ROW_SPACING, move |index| {
                            let Some(row) = list.row(index) else {
                                return rect().into_element();
                            };
                            let id = row.id.clone();
                            let is_selected = selected.as_deref() == Some(row.id.as_str());
                            version_row(
                                &row,
                                is_selected,
                                (move |()| picked.set(Some(id.clone()))).into(),
                            )
                        })
                        .into_element()
                }
            })
            .into_element();

        let art = version_art(Some(&shown), Some(loader));

        let close_x = self.on_close.clone();
        let close_cancel = self.on_close.clone();
        let close_save = self.on_close.clone();

        shell(Shell {
            rail: rail(Rail {
                art,
                title: self.facts.name.clone(),
                subtitle: if changed {
                    format!("{current} → {shown}")
                } else {
                    format!("Currently on {current}")
                },
                card: facts_card(vec![
                    ("Type", kind_label(self.facts.kind).to_string()),
                    ("Version", current.clone()),
                    ("Loader", loader.to_string()),
                ]),
                tags: Vec::new(),
            }),
            eyebrow: Some("Change version".to_string()),
            title: "Move this instance to another version".to_string(),
            subtitle: mods_warning.unwrap_or_else(|| {
                "Everything in the instance stays — worlds, mods and settings included.".to_string()
            }),
            body,
            scrolls_itself: false,
            note: if loader_support == Some(false) {
                format!("{loader} has no builds for {shown} yet — pick another version.")
            } else {
                "The new version's files download automatically the next time you play.".to_string()
            },
            secondary_label: "Cancel".to_string(),
            primary_label: "Change version".to_string(),
            primary_enabled: ready,
            on_close: (move |()| close_x.call(())).into(),
            on_secondary: (move |()| close_cancel.call(())).into(),
            on_primary: (move |()| {
                if let Some(version) = picked_version.clone() {
                    mutation.mutate(ClusterAction::ChangeVersion {
                        cluster_id,
                        mc_version: version,
                    });
                }
                close_save.call(());
            })
            .into(),
        })
    }
}

#[derive(PartialEq)]
pub struct DuplicateInstanceModal {
    facts: InstanceFacts,
    on_close: EventHandler<()>,
}

impl DuplicateInstanceModal {
    pub fn new(facts: InstanceFacts, on_close: impl Into<EventHandler<()>>) -> Self {
        Self {
            facts,
            on_close: on_close.into(),
        }
    }
}

impl Component for DuplicateInstanceModal {
    fn render(&self) -> impl IntoElement {
        let mutation = use_cluster_mutation();
        let cluster_id = self.facts.cluster_id;

        let state = DetailsState::seeded(
            &format!("{} (copy)", self.facts.name),
            self.facts.description.as_deref(),
            &self.facts.tags,
        );

        let existing = self.facts.cover.clone();
        let preview = state.preview_cover(existing.clone());
        let tags = state.tags.read().clone();
        let typed = state.typed_name();
        let name_problem = validate_name(&typed, name_limit(self.facts.modpack)).err();
        let ready = !typed.is_empty() && name_problem.is_none();

        let description = state.description.read().trim().to_string();
        let subtitle = if description.is_empty() {
            format!(
                "Copy of {} · {}",
                kind_label(self.facts.kind),
                self.facts.mc_version
            )
        } else {
            description
        };

        let title = if typed.is_empty() {
            format!("{} (copy)", self.facts.name)
        } else {
            typed.clone()
        };

        let art = version_art(Some(&self.facts.mc_version), Some(self.facts.mc_loader));
        let art = match &preview {
            Some((path, true)) => art.picked_cover(Some(path.clone())),
            Some((path, false)) => art.cover(Some(path.clone())),
            None => art,
        };

        let loader = self.facts.mc_loader.to_string();

        let close_x = self.on_close.clone();
        let close_cancel = self.on_close.clone();
        let close_save = self.on_close.clone();

        shell(Shell {
            rail: rail(Rail {
                art,
                title,
                subtitle,
                card: facts_card(
                    vec![
                        ("Type", kind_label(self.facts.kind).to_string()),
                        ("Version", self.facts.mc_version.clone()),
                        ("Loader", loader),
                    ],
                ),
                tags,
            }),
            eyebrow: Some("Duplicate instance".to_string()),
            title: "Copy this instance".to_string(),
            subtitle: "Every file is copied into a brand new instance — instanced worlds, \
                       mods, shaders and textures included."
                .to_string(),
            body: details_body(
                state,
                format!("{} (copy)", self.facts.name),
                existing,
                version_art(Some(&self.facts.mc_version), Some(self.facts.mc_loader)),
                name_problem,
                name_limit(self.facts.modpack),
            ),
            scrolls_itself: false,
            note: "The copy starts exactly like the original and shares nothing with it from there on."
                .to_string(),
            secondary_label: "Cancel".to_string(),
            primary_label: "Create copy".to_string(),
            primary_enabled: ready,
            on_close: (move |()| close_x.call(())).into(),
            on_secondary: (move |()| close_cancel.call(())).into(),
            on_primary: (move |()| {
                mutation.mutate(ClusterAction::DuplicateInstance {
                    cluster_id,
                    name: state.typed_name(),
                    description: state.description_value(),
                    tags: state.tags.read().clone(),
                });
                close_save.call(());
            })
            .into(),
        })
    }
}
