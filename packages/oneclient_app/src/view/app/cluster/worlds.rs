use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use freya::prelude::*;
use freya::router::RouterContext;
use oneclient_core::{LauncherError, WorldInfo};

use crate::components::{
    Button, CARD_BG, CARD_NAME, CardLayout, ContextMenu, Icon, IconType, TextInput, kebab_button,
    meta_size, meta_text, on_secondary,
};
use crate::hooks::{
    backup_world, delete_world, duplicate_world, import_world_zip, query_is_loading, rename_world,
    spawn_world_task, try_cluster_worlds, try_world_size, use_cluster, use_cluster_worlds,
    use_datapack_world, use_dispatch, use_saves_folder_watch, use_view_state, use_world_size,
};
use crate::layout::cluster_content;
use crate::routes::Route;
use crate::theme::colors;
use crate::ui::{border_all_color, fmt_date};
use crate::utils::format_size;

use super::cluster_not_found;
use super::folder_list::{
    CardIcon, RowHeights, card_icon, confirm_dialog, content_box, dialog, folder_button,
    layout_toggle, matches_search, notify_in_use, search_input, supports_datapacks, toolbar_panel,
    use_game_folder_in_use,
};
use super::package_manager::{empty_hint, empty_shell, empty_title};

const LIST_ICON: f32 = 44.;
const GRID_ICON: f32 = 40.;
const LIST_PAD: f32 = 10.;
const GRID_PAD: f32 = 14.;

const SHARED_NOTICE: &str = "This version uses the shared game folder, so these worlds also appear in every other version that uses it.";

const WORLD_ROWS: RowHeights = RowHeights {
    list: LIST_ICON + 2. * LIST_PAD,
    grid: GRID_ICON + 2. * GRID_PAD,
};

#[derive(PartialEq)]
pub struct ClusterWorlds {
    pub cluster_id: i64,
}

#[derive(Clone)]
enum WorldOp {
    Rename { world: String },
    Duplicate { world: String },
}

fn world_safe(name: &str) -> bool {
    !name.is_empty() && !name.contains(&['/', '\\'][..])
}

fn open_datapacks(cluster_id: i64, world: String, mut remembered: State<HashMap<i64, String>>) {
    remembered.write().insert(cluster_id, world);
    let _ = RouterContext::get().push(Route::ClusterDataPacks { cluster_id });
}

impl Component for ClusterWorlds {
    fn render(&self) -> impl IntoElement {
        let cluster_id = self.cluster_id;
        let cluster = use_cluster(cluster_id);
        let saves = cluster
            .as_ref()
            .and_then(|c| c.game_dir().ok())
            .map(|d| d.join("saves"));

        let query = use_cluster_worlds(cluster_id);
        use_saves_folder_watch(saves.clone(), query);
        let dispatch = use_dispatch();
        let in_use = use_game_folder_in_use(cluster.as_ref());
        let remembered = use_datapack_world();
        let search = use_state(String::new);
        let layout = use_view_state("cluster.worlds").layout;
        let mut menu = use_state(|| None::<(f32, f32, WorldInfo)>);
        let mut pending_delete = use_state(|| None::<String>);
        let mut prompt = use_state(|| None::<WorldOp>);
        let mut name_text = use_state(String::new);

        let Some(cluster) = cluster else {
            return cluster_not_found();
        };
        let shared = !cluster.uses_dedicated_dir();
        let datapacks = supports_datapacks(&cluster.mc_version);

        let all = try_cluster_worlds(&query).unwrap_or_default();
        let needle = search.read().trim().to_lowercase();
        let worlds: Vec<WorldInfo> = all
            .iter()
            .filter(|w| matches_search(&needle, &[&w.folder_name]))
            .cloned()
            .collect();
        let card_layout = CardLayout::from(*layout.read());

        let row = {
            let worlds = worlds.clone();
            move |i: usize| {
                let info = worlds[i].clone();
                let press_world = info.folder_name.clone();
                let menu_info = info.clone();
                WorldCard {
                    cluster_id,
                    info,
                    layout: card_layout,
                    on_press: datapacks.then(|| {
                        (move |()| open_datapacks(cluster_id, press_world.clone(), remembered))
                            .into()
                    }),
                    on_context: (move |(x, y)| menu.set(Some((x, y, menu_info.clone())))).into(),
                }
                .into_element()
            }
        };

        let empty = if query_is_loading(&query) {
            None
        } else if all.is_empty() {
            Some(
                empty_shell(IconType::Globe01)
                    .child(empty_title("No worlds yet."))
                    .child(empty_hint(
                        "Worlds show up here once you create one in game.",
                    ))
                    .into_element(),
            )
        } else {
            Some(
                empty_shell(IconType::SearchMd)
                    .child(empty_title("No worlds match your search."))
                    .child(empty_hint("Try a different term."))
                    .into_element(),
            )
        };

        let mut controls = vec![search_input(search)];
        controls.extend(saves.map(folder_button));
        let all_names: Vec<String> = all.iter().map(|w| w.folder_name.clone()).collect();
        controls.push(
            Button::new()
                .secondary()
                .tooltip("Back up every world as its own .zip file")
                .disabled(all.is_empty())
                .on_press({
                    let dispatch = dispatch.clone();
                    let names = all_names.clone();
                    move |_| {
                        if in_use {
                            notify_in_use(&dispatch, "Worlds");
                            return;
                        }
                        let dispatch = dispatch.clone();
                        let names = names.clone();
                        spawn_forever(async move {
                            let Some(dir) = rfd::AsyncFileDialog::new()
                                .set_title("Back up all worlds")
                                .pick_folder()
                                .await
                            else {
                                return;
                            };
                            let dir = dir.path().to_path_buf();
                            let mut ok = 0usize;
                            let mut failures = 0usize;
                            for name in &names {
                                let dest = dir.join(format!("{name}.zip"));
                                match backup_world(cluster_id, name.clone(), dest).await {
                                    Ok(()) => ok += 1,
                                    Err(_) => failures += 1,
                                }
                            }
                            if failures == 0 && ok > 0 {
                                dispatch
                                    .notify("Worlds backed up")
                                    .body(format!("{ok} worlds saved as .zip files."))
                                    .info()
                                    .icon(IconType::FolderCheck)
                                    .toast_only()
                                    .send();
                            } else {
                                dispatch
                                    .notify("Couldn't back up all worlds")
                                    .body(format!("{failures} of {} worlds failed.", ok + failures))
                                    .error()
                                    .send();
                            }
                        });
                    }
                })
                .child(Icon::new(IconType::FolderDownload).size(14.))
                .text("Back up all")
                .into_element(),
        );
        controls.push(layout_toggle(layout));

        let menu_overlay = menu.read().clone().map(|(x, y, info)| {
            let open_world = info.folder_name.clone();
            let target_world = info.folder_name.clone();
            let rename_src = info.folder_name.clone();
            let duplicate_src = info.folder_name.clone();
            let path = info.path.clone();
            let mut context = ContextMenu::new(x, y).title(info.folder_name.clone());
            if datapacks {
                context = context.action(IconType::Database01, "Data packs", move |()| {
                    open_datapacks(cluster_id, open_world.clone(), remembered)
                });
            }
            context
                .action(IconType::Folder, "Open folder", move |()| {
                    crate::platform::open_path(&path.to_string_lossy())
                })
                .action(IconType::Pencil01, "Rename\u{2026}", move |()| {
                    prompt.set(Some(WorldOp::Rename {
                        world: rename_src.clone(),
                    }));
                    name_text.set(rename_src.clone());
                })
                .action(IconType::Copy01, "Duplicate\u{2026}", move |()| {
                    let src = duplicate_src.clone();
                    prompt.set(Some(WorldOp::Duplicate { world: src.clone() }));
                    name_text.set(format!("{src} (copy)"));
                })
                .action(IconType::Download01, "Backup as\u{2026}", {
                    let dispatch = dispatch.clone();
                    let backup_src = info.folder_name.clone();
                    move |()| {
                        if in_use {
                            notify_in_use(&dispatch, "Worlds");
                            return;
                        }
                        let backup_src = backup_src.clone();
                        let dispatch = dispatch.clone();
                        spawn_forever(async move {
                            let Some(file) = rfd::AsyncFileDialog::new()
                                .set_title(format!("Back up {backup_src}"))
                                .add_filter("World backup", &["zip"])
                                .set_file_name(format!("{backup_src}.zip"))
                                .save_file()
                                .await
                            else {
                                return;
                            };
                            let dest = file.path().to_path_buf();
                            match backup_world(cluster_id, backup_src.clone(), dest).await {
                                Ok(()) => {
                                    dispatch
                                        .notify("World backed up")
                                        .body(format!("{backup_src} was saved as a .zip."))
                                        .info()
                                        .icon(IconType::DownloadCloud02)
                                        .toast_only()
                                        .send();
                                }
                                Err(err) => {
                                    dispatch
                                        .notify("Couldn't back up world")
                                        .body(err.to_string())
                                        .error()
                                        .send();
                                }
                            }
                        });
                    }
                })
                .action(IconType::FilePlus02, "Restore from backup\u{2026}", {
                    let dispatch = dispatch.clone();
                    move |()| {
                        if in_use {
                            notify_in_use(&dispatch, "Worlds");
                            return;
                        }
                        let dispatch = dispatch.clone();
                        spawn_forever(async move {
                            let Some(file) = rfd::AsyncFileDialog::new()
                                .set_title("Restore a world backup")
                                .add_filter("World backup", &["zip"])
                                .pick_file()
                                .await
                            else {
                                return;
                            };
                            let zip = file.path().to_path_buf();
                            let name = zip
                                .file_stem()
                                .map(|stem| stem.to_string_lossy().to_string())
                                .filter(|stem| !stem.is_empty() && world_safe(stem))
                                .unwrap_or_else(|| "restored_world".to_string());
                            match import_world_zip(cluster_id, name.clone(), zip).await {
                                Ok(()) => {
                                    dispatch
                                        .notify("World restored")
                                        .body(format!("{name} was restored from the backup."))
                                        .info()
                                        .icon(IconType::FolderCheck)
                                        .toast_only()
                                        .send();
                                }
                                Err(err) => {
                                    dispatch
                                        .notify("Couldn't restore world")
                                        .body(err.to_string())
                                        .error()
                                        .send();
                                }
                            }
                        });
                    }
                })
                .separator()
                .danger_action(IconType::Trash01, "Delete", {
                    let dispatch = dispatch.clone();
                    move |()| {
                        if in_use {
                            notify_in_use(&dispatch, "Worlds");
                        } else {
                            pending_delete.set(Some(target_world.clone()));
                        }
                    }
                })
                .on_close(move |_| menu.set(None))
                .into_element()
        });

        let confirm_overlay = pending_delete.read().clone().map(|world| {
            let target_world = world.clone();
            let body = if shared {
                "This version uses the shared game folder, so the world is removed from every version that uses it. It can be restored from your system trash."
            } else {
                "It can be restored from your system trash."
            };
            confirm_dialog(
                format!("Move \"{world}\" to trash?"),
                body.to_string(),
                move || pending_delete.set(None),
                {
                    let dispatch = dispatch.clone();
                    move || {
                        pending_delete.set(None);
                        if in_use {
                            notify_in_use(&dispatch, "Worlds");
                            return;
                        }
                        spawn_world_task(
                            dispatch.clone(),
                            "Couldn't delete world",
                            delete_world(cluster_id, target_world.clone()),
                        );
                    }
                },
            )
        });

        let prompt_overlay = prompt.read().clone().map(|op| {
            let is_rename = matches!(op, WorldOp::Rename { .. });
            let original = match &op {
                WorldOp::Rename { world } | WorldOp::Duplicate { world } => world.clone(),
            };
            let current = name_text.read().clone();
            let trimmed = current.trim();
            let safe = !trimmed.is_empty() && !trimmed.contains(&['/', '\\'][..]);
            let enabled = safe && !(is_rename && trimmed == original);

            let title = if is_rename {
                "Rename world"
            } else {
                "Duplicate world"
            };
            let body = if is_rename {
                "The world folder is renamed on disk. World links inside the instance that point at this folder will need updating."
            } else {
                "A full copy of the world folder is created under the new name."
            };

            let op_for_task = op.clone();
            let close = move || prompt.set(None);
            let mut cancel = close;
            let mut confirm = {
                let dispatch = dispatch.clone();
                move || {
                    prompt.set(None);
                    if in_use {
                        notify_in_use(&dispatch, "Worlds");
                        return;
                    }
                    let target = match &op_for_task {
                        WorldOp::Rename { world } | WorldOp::Duplicate { world } => {
                            world.clone()
                        }
                    };
                    let name = name_text.read().clone().trim().to_string();
                    let task: Pin<Box<dyn Future<Output = Result<(), LauncherError>> + 'static>> =
                        match &op_for_task {
                            WorldOp::Rename { .. } => Box::pin(rename_world(
                                cluster_id,
                                target,
                                name,
                            )),
                            WorldOp::Duplicate { .. } => Box::pin(duplicate_world(
                                cluster_id,
                                target,
                                name,
                            )),
                        };
                    spawn_world_task(
                        dispatch.clone(),
                        if is_rename {
                            "Couldn't rename world"
                        } else {
                            "Couldn't duplicate world"
                        },
                        task,
                    );
                }
            };

            dialog(
                title.to_string(),
                body.to_string(),
                Some(
                    TextInput::new(name_text)
                        .placeholder(if is_rename { "World name" } else { "Copy name" })
                        .width(Size::fill())
                        .into_element(),
                ),
                close,
                [
                    Button::new()
                        .secondary()
                        .on_press(move |_| cancel())
                        .text("Cancel")
                        .into_element(),
                    Button::new()
                        .primary()
                        .on_press(move |_| confirm())
                        .disabled(!enabled)
                        .child(
                            Icon::new(if is_rename {
                                IconType::Pencil01
                            } else {
                                IconType::Copy01
                            })
                            .size(14.),
                        )
                        .text(if is_rename { "Rename" } else { "Duplicate" })
                        .into_element(),
                ],
            )
        });

        cluster_content()
            .child(toolbar_panel(None, controls))
            .child(content_box(
                worlds.len(),
                card_layout,
                WORLD_ROWS,
                row,
                empty,
                shared
                    .then(|| SHARED_NOTICE.to_string())
                    .into_iter()
                    .collect(),
            ))
            .maybe_child(menu_overlay)
            .maybe_child(confirm_overlay)
            .maybe_child(prompt_overlay)
            .into_element()
    }
}

#[derive(PartialEq)]
struct WorldCard {
    cluster_id: i64,
    info: WorldInfo,
    layout: CardLayout,
    on_press: Option<EventHandler<()>>,
    on_context: EventHandler<(f32, f32)>,
}

impl Component for WorldCard {
    fn render(&self) -> impl IntoElement {
        let mut hovered = use_state(|| false);
        let info = &self.info;
        let grid = self.layout == CardLayout::Grid;
        let size = try_world_size(&use_world_size(self.cluster_id, info.folder_name.clone()));

        let icon = card_icon(
            &info
                .icon
                .clone()
                .map_or(CardIcon::Symbol(IconType::Globe01), CardIcon::Image),
            if grid { GRID_ICON } else { LIST_ICON },
        );
        let last_played = format!("Last played {}", fmt_date(info.last_played));
        let on_press = self.on_press.clone();
        let pressable = on_press.is_some();

        let text = if grid {
            rect()
                .vertical()
                .width(Size::flex(1.0))
                .spacing(3.)
                .child(
                    label()
                        .text(info.folder_name.clone())
                        .font_size(14.)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .color(Color::WHITE),
                )
                .child(meta_text(
                    match size {
                        Some(size) => format!("{last_played} \u{b7} {}", format_size(size)),
                        None => last_played.clone(),
                    },
                    CARD_NAME.with_a(127),
                ))
        } else {
            rect()
                .vertical()
                .width(Size::flex(1.0))
                .spacing(3.)
                .child(
                    label()
                        .text(info.folder_name.clone())
                        .font_size(15.)
                        .font_weight(FontWeight::MEDIUM)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .color(CARD_NAME),
                )
                .child(
                    label()
                        .text(last_played)
                        .font_size(11.)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .width(Size::fill())
                        .color(colors::fg_secondary()),
                )
        };

        let hovering = *hovered.read();
        let card = rect()
            .horizontal()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .content(Content::Flex)
            .maybe(pressable, |el| el.cursor(CursorIcon::Pointer))
            .on_press(move |_| {
                if let Some(handler) = &on_press {
                    handler.call(());
                }
            })
            .on_secondary_down(on_secondary(Some(self.on_context.clone())));

        if grid {
            card.height(Size::px(WORLD_ROWS.grid))
                .spacing(11.)
                .padding(Gaps::new_all(GRID_PAD))
                .corner_radius(CornerRadius::new_all(6.))
                .background(if hovering {
                    colors::component_bg_hover()
                } else {
                    colors::component_bg()
                })
                .border(border_all_color(
                    1.,
                    if hovering {
                        colors::component_border_hover()
                    } else {
                        colors::component_border()
                    },
                ))
                .overflow(Overflow::Clip)
                .on_pointer_enter(move |_| hovered.set(true))
                .on_pointer_leave(move |_| hovered.set(false))
                .child(icon)
                .child(text)
                .child(kebab_button(self.on_context.clone()))
        } else {
            card.height(Size::px(WORLD_ROWS.list))
                .spacing(12.)
                .padding(Gaps::new_all(LIST_PAD))
                .corner_radius(CornerRadius::new_all(8.))
                .background(CARD_BG)
                .child(icon)
                .child(text)
                .child(meta_size(size.unwrap_or_default()))
                .child(kebab_button(self.on_context.clone()))
        }
    }
}
