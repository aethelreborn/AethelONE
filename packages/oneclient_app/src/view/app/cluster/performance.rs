//! The mods that raise the frame rate the most, offered where the user goes
//! looking for mods
//!
//! A launcher cannot promise a frame rate on its own: the video settings and
//! the garbage collector only take the frame away from the game, the mods are
//! what hand it back

use freya::prelude::*;
use oneclient_content::packages::{GameLoader, ProviderId};

use crate::components::{Button, Icon, IconType};
use crate::hooks::{
    Actions, VERSIONS_PAGE_SIZE, use_dispatch, use_package_versions_when, version_list,
};
use crate::recommended::RecommendedPackage;
use crate::theme::colors;
use crate::ui::border_all_color;

/// The packages the card offers, in the order they help
const PERFORMANCE_IDS: [&str; 5] = [
    "AANobbMI", // Sodium
    "gvQqBUqZ", // Lithium
    "5ZwdcRci", // ImmediatelyFast
    "uXXizFIs", // FerriteCore
    "NNAgCjsB", // Entity Culling
];

#[derive(PartialEq)]
pub struct PerformanceSet {
    pub cluster_id: i64,
    pub picks: Vec<&'static RecommendedPackage>,
    /// The project ids the instance already carries
    pub installed: Vec<String>,
    pub mc_version: String,
    pub loader: GameLoader,
}

impl Component for PerformanceSet {
    fn render(&self) -> impl IntoElement {
        let cluster_id = self.cluster_id;
        let dispatch = use_dispatch();

        // The versions are asked for in the same order on every render so the
        // hooks keep their place
        let versions: [Option<String>; 5] = [
            latest_version(PERFORMANCE_IDS[0], &self.mc_version, self.loader),
            latest_version(PERFORMANCE_IDS[1], &self.mc_version, self.loader),
            latest_version(PERFORMANCE_IDS[2], &self.mc_version, self.loader),
            latest_version(PERFORMANCE_IDS[3], &self.mc_version, self.loader),
            latest_version(PERFORMANCE_IDS[4], &self.mc_version, self.loader),
        ];

        let mut missing = 0;
        let rows: Vec<Element> = self
            .picks
            .iter()
            .map(|pick| {
                let version = version_for(pick.project_id, &versions);
                let installed = self.installed.iter().any(|id| id == pick.project_id);
                if !installed && version.is_some() {
                    missing += 1;
                }

                pick_row(cluster_id, pick, installed, version, dispatch.clone())
            })
            .collect();

        let missing = missing;
        let install_all = {
            let dispatch = dispatch.clone();
            let versions = versions.clone();
            let picks: Vec<(&'static str, Option<String>)> = self
                .picks
                .iter()
                .map(|pick| (pick.project_id, version_for(pick.project_id, &versions)))
                .collect();
            let installed = self.installed.clone();

            move |_| {
                for (project_id, version) in picks {
                    if installed.iter().any(|id| id == project_id) {
                        continue;
                    }
                    let Some(version) = version else {
                        continue;
                    };
                    dispatch.install_package(
                        cluster_id,
                        ProviderId::Modrinth,
                        project_id.to_string(),
                        version,
                        None,
                    );
                }
            }
        };

        rect()
            .vertical()
            .width(Size::fill())
            .spacing(10.)
            .padding(Gaps::new_all(14.))
            .corner_radius(CornerRadius::new_all(12.))
            .background(colors::page_elevated())
            .border(border_all_color(1., colors::component_border()))
            .child(
                rect()
                    .vertical()
                    .width(Size::fill())
                    .spacing(2.)
                    .child(
                        label()
                            .text("Performance set")
                            .font_size(16.)
                            .font_weight(FontWeight::MEDIUM)
                            .color(colors::fg_primary()),
                    )
                    .child(
                        label()
                            .text(
                                "These are the mods that raise the frame rate the most. The \
                                 launcher writes the video settings and tunes the garbage \
                                 collector, the mods are what hand the frames back.",
                            )
                            .font_size(12.)
                            .color(colors::fg_secondary()),
                    ),
            )
            .children(rows)
            .child(
                Button::new()
                    .primary()
                    .enabled(missing > 0)
                    .on_press(install_all)
                    .child(Icon::new(IconType::Download01).size(14.))
                    .text(if missing > 0 {
                        format!("Install the {missing} missing")
                    } else {
                        "Everything is installed".to_string()
                    }),
            )
            .into_element()
    }
}

/// The newest version of a project that runs on this game and loader
fn latest_version(project_id: &str, mc_version: &str, loader: GameLoader) -> Option<String> {
    let query = use_package_versions_when(
        true,
        ProviderId::Modrinth,
        project_id.to_string(),
        Some(mc_version.to_string()),
        Some(loader),
        0,
        VERSIONS_PAGE_SIZE,
    );

    version_list(&query)
        .first()
        .map(|version| version.version_id.clone())
}

/// The version that belongs to a project out of the answers the queries gave
fn version_for(project_id: &str, versions: &[Option<String>; 5]) -> Option<String> {
    let index = PERFORMANCE_IDS.iter().position(|id| *id == project_id)?;

    versions[index].clone()
}

fn pick_row(
    cluster_id: i64,
    pick: &'static RecommendedPackage,
    installed: bool,
    version: Option<String>,
    dispatch: Actions,
) -> Element {
    let control = if installed {
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .child(
                Icon::new(IconType::CheckCircle)
                    .size(14.)
                    .color(colors::success()),
            )
            .child(
                label()
                    .text("Installed")
                    .font_size(12.)
                    .color(colors::fg_secondary()),
            )
            .into_element()
    } else {
        let project_id = pick.project_id;
        let provider = pick.provider;
        Button::new()
            .secondary()
            .small()
            .enabled(version.is_some())
            .on_press(move |_| {
                let Some(version) = version.clone() else {
                    return;
                };
                dispatch.install_package(
                    cluster_id,
                    provider,
                    project_id.to_string(),
                    version,
                    None,
                );
            })
            .child(Icon::new(IconType::Download01).size(13.))
            .text("Install")
            .into_element()
    };

    rect()
        .horizontal()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .spacing(12.)
        .child(
            rect()
                .vertical()
                .width(Size::flex(1.0))
                .spacing(1.)
                .child(
                    label()
                        .text(pick.name)
                        .font_size(13.)
                        .font_weight(FontWeight::MEDIUM)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text(pick.summary)
                        .font_size(11.)
                        .color(colors::fg_secondary()),
                ),
        )
        .child(control)
        .into_element()
}
