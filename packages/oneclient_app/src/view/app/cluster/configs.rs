use freya::prelude::*;
use oneclient_core::ConfigFileInfo;

use crate::components::{Button, Icon, IconType, ScrollArea, TextInput};
use crate::hooks::{
    invalidate_config_files, query_error, query_is_loading, try_config_content, try_config_files,
    use_cluster, use_config_content, use_config_files, use_dispatch, write_config_file,
};
use crate::layout::cluster_content;
use crate::theme::{self, colors};
use crate::ui::border_all_color;
use crate::utils::format_size;

use super::cluster_not_found;
use super::folder_list::folder_button;
use super::package_manager::{empty_hint, empty_shell, empty_title};

#[derive(PartialEq)]
pub struct ClusterConfigs {
    pub cluster_id: i64,
}

impl Component for ClusterConfigs {
    fn render(&self) -> impl IntoElement {
        let cluster_id = self.cluster_id;
        let cluster = use_cluster(cluster_id);
        let files_query = use_config_files(cluster_id);
        let files = try_config_files(&files_query);
        let loading = query_is_loading(&files_query);
        let list_error = query_error(&files_query);

        let selected = use_state(|| None::<String>);
        let pending_switch = use_state(|| None::<String>);
        let mut text = use_state(String::new);
        let mut baseline = use_state(|| (String::new(), String::new()));
        let dispatch = use_dispatch();

        let Some(cluster) = cluster else {
            return cluster_not_found();
        };
        let config_dir = cluster.game_dir().ok().map(|dir| dir.join("config"));

        let current = selected
            .read()
            .clone()
            .filter(|path| {
                files
                    .as_ref()
                    .is_none_or(|files| files.iter().any(|file| &file.path == path))
            })
            .or_else(|| {
                files
                    .as_ref()
                    .and_then(|files| files.first().map(|file| file.path.clone()))
            });

        let content_query = use_config_content(cluster_id, current.clone().unwrap_or_default());
        let server = try_config_content(&content_query);
        let content_error = query_error(&content_query);

        if let Some(path) = current.as_ref()
            && let Some(server) = server.as_ref()
            && baseline.peek().0 != *path
        {
            baseline.set((path.clone(), server.clone()));
            text.set(server.clone());
        }

        let dirty = *text.read() != baseline.read().1;
        let can_edit = current.is_some() && content_error.is_none();

        let leading = config_dir.clone().map(|dir| {
            rect()
                .horizontal()
                .spacing(8.)
                .child(folder_button(dir))
                .into_element()
        });

        let body = if let Some(files) = files.as_ref() {
            if files.is_empty() {
                empty_shell(IconType::File02)
                    .child(empty_title("No config files yet."))
                    .child(empty_hint(
                        "Launch the game or install a mod that writes into config/ and its files will show up here.",
                    ))
                    .into_element()
            } else {
                config_body(
                    cluster_id,
                    files,
                    current,
                    content_error,
                    can_edit,
                    dirty,
                    text,
                    baseline,
                    selected,
                    pending_switch,
                    dispatch,
                )
            }
        } else if let Some(error) = list_error {
            empty_shell(IconType::FileX02)
                .child(empty_title("Couldn't read the config folder."))
                .child(empty_hint(error))
                .into_element()
        } else if loading {
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(
                    label()
                        .text("Loading configs...")
                        .font_size(13.)
                        .color(colors::fg_secondary()),
                )
                .into_element()
        } else {
            rect().into_element()
        };

        let pending = pending_switch.read().clone();
        let confirm = pending.map(|path| {
            let mut pending_switch = pending_switch;
            let mut selected = selected;
            super::dialog(
                "Discard unsaved changes?".to_string(),
                format!("Your edits to {} will be lost.", short_name(&path)),
                None,
                move || pending_switch.set(None),
                [
                    Button::new()
                        .secondary()
                        .on_press(move |_| pending_switch.set(None))
                        .text("Keep editing")
                        .into_element(),
                    Button::new()
                        .danger()
                        .on_press(move |_| {
                            selected.set(Some(path.clone()));
                            pending_switch.set(None);
                        })
                        .text("Discard")
                        .into_element(),
                ],
            )
        });

        cluster_content()
            .child(
                rect()
                    .horizontal()
                    .width(Size::fill())
                    .cross_align(Alignment::Center)
                    .spacing(8.)
                    .padding(Gaps::new(12., 40., 12., 40.))
                    .maybe_child(leading)
                    .child(
                        label()
                            .text("Edit the text files mods read at startup.")
                            .font_size(12.)
                            .color(colors::fg_secondary()),
                    ),
            )
            .child(
                rect()
                    .vertical()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .padding(Gaps::new(0., 40., 16., 40.))
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::flex(1.0))
                            .padding(Gaps::new_all(12.))
                            .corner_radius(CornerRadius::new_all(12.))
                            .background(colors::page_elevated())
                            .overflow(Overflow::Clip)
                            .child(body),
                    ),
            )
            .maybe_child(confirm)
            .into_element()
    }
}

#[allow(clippy::too_many_arguments)]
fn config_body(
    cluster_id: i64,
    files: &[ConfigFileInfo],
    current: Option<String>,
    content_error: Option<String>,
    can_edit: bool,
    dirty: bool,
    text: State<String>,
    baseline: State<(String, String)>,
    selected: State<Option<String>>,
    pending_switch: State<Option<String>>,
    dispatch: crate::Actions,
) -> Element {
    let list = ScrollArea::new()
        .width(Size::px(300.))
        .height(Size::fill())
        .spacing(2.)
        .child(
            rect()
                .vertical()
                .width(Size::fill())
                .spacing(2.)
                .children(files.iter().map(|file| {
                    let path = file.path.clone();
                    let active = current.as_deref() == Some(path.as_str());
                    let mut selected = selected;
                    let mut pending_switch = pending_switch;
                    let switch = file.path.clone();
                    file_row(file, active, move |_: Event<PressEventData>| {
                        if active {
                            return;
                        }
                        if dirty {
                            pending_switch.set(Some(switch.clone()));
                        } else {
                            selected.set(Some(switch.clone()));
                        }
                    })
                })),
        );

    let (header, editor) = match current.as_deref() {
        Some(path) => {
            let path = path.to_string();
            let explanation = content_error.clone();
            let editor = if let Some(error) = explanation {
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .center()
                    .child(label().text(error).font_size(12.).color(colors::danger()))
                    .into_element()
            } else {
                let submit_path = path.clone();
                let submit_dispatch = dispatch.clone();
                TextInput::new(text)
                    .multiline(true)
                    .font_family(theme::MONO_FONT)
                    .font_size(13.)
                    .width(Size::fill())
                    .height(Size::fill())
                    .on_submit(move |_| {
                        if dirty {
                            save_config(
                                cluster_id,
                                submit_path.clone(),
                                text.read().clone(),
                                baseline.clone(),
                                submit_dispatch.clone(),
                            );
                        }
                    })
                    .into_element()
            };

            let mut revert = text;
            let revert_baseline = baseline.peek().1.clone();
            let save_path = path.clone();
            let save_dispatch = dispatch.clone();
            let header = rect()
                .horizontal()
                .width(Size::fill())
                .cross_align(Alignment::Center)
                .spacing(8.)
                .child(
                    label()
                        .text(path.clone())
                        .font_family(theme::MONO_FONT)
                        .font_size(12.)
                        .color(colors::fg_secondary()),
                )
                .child(rect().width(Size::flex(1.0)))
                .child(
                    Button::new()
                        .secondary()
                        .enabled(dirty)
                        .on_press(move |_| revert.set(revert_baseline.clone()))
                        .text("Revert"),
                )
                .child(
                    Button::new()
                        .primary()
                        .enabled(can_edit && dirty)
                        .child(Icon::new(IconType::Check).size(14.))
                        .on_press(move |_| {
                            let content = text.read().clone();
                            save_config(
                                cluster_id,
                                save_path.clone(),
                                content,
                                baseline,
                                save_dispatch.clone(),
                            );
                        })
                        .text("Save"),
                );

            (header.into_element(), editor)
        }
        None => (
            rect().into_element(),
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .center()
                .child(
                    label()
                        .text("Pick a file to edit.")
                        .font_size(13.)
                        .color(colors::fg_secondary()),
                )
                .into_element(),
        ),
    };

    rect()
        .horizontal()
        .width(Size::fill())
        .height(Size::fill())
        .spacing(12.)
        .child(list)
        .child(
            rect()
                .vertical()
                .width(Size::fill())
                .height(Size::fill())
                .spacing(8.)
                .child(header)
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::flex(1.0))
                        .padding(Gaps::new_all(8.))
                        .corner_radius(CornerRadius::new_all(10.))
                        .background(colors::component_bg())
                        .border(border_all_color(1., colors::component_border()))
                        .overflow(Overflow::Clip)
                        .child(editor),
                ),
        )
        .into_element()
}

fn save_config(
    cluster_id: i64,
    path: String,
    content: String,
    mut baseline: State<(String, String)>,
    dispatch: crate::Actions,
) {
    spawn(async move {
        match write_config_file(cluster_id, path.clone(), content.clone()).await {
            Ok(()) => {
                baseline.set((path, content));
                invalidate_config_files().await;
                dispatch
                    .notify("Config saved")
                    .body("The game reads it on the next launch.")
                    .info()
                    .icon(IconType::Check)
                    .toast_only()
                    .send();
            }
            Err(err) => {
                dispatch
                    .notify("Couldn't save config")
                    .body(err.to_string())
                    .error()
                    .send();
            }
        }
    });
}

fn file_row(
    file: &ConfigFileInfo,
    active: bool,
    on_press: impl Into<EventHandler<Event<PressEventData>>>,
) -> Element {
    rect()
        .horizontal()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .padding(Gaps::new_symmetric(8., 10.))
        .corner_radius(CornerRadius::new_all(8.))
        .background(if active {
            colors::component_bg()
        } else {
            Color::TRANSPARENT
        })
        .on_press(on_press)
        .child(
            rect()
                .vertical()
                .width(Size::fill())
                .spacing(2.)
                .child(
                    label()
                        .text(short_name(&file.path))
                        .font_size(12.)
                        .font_family(theme::MONO_FONT)
                        .color(if active {
                            colors::fg_primary()
                        } else {
                            colors::fg_secondary()
                        }),
                )
                .child(
                    label()
                        .text(format!(
                            "{} \u{b7} {}",
                            file.path,
                            format_size(file.size_bytes)
                        ))
                        .font_size(10.)
                        .color(colors::fg_secondary()),
                ),
        )
        .into_element()
}

fn short_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}
