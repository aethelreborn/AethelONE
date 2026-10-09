use bytes::Bytes;
use freya::prelude::*;

use crate::components::{
    Button, ButtonVariant, Icon, IconType, PlayerModel, ScrollArea, TextInput,
};
use crate::hooks::{try_default_account, use_current_account};
use crate::theme::colors;
use crate::ui::border_all_color;

mod library;
pub mod inject;

use library::{
    Library, SkinEntry, SkinKind, fetch_by_name, fetch_url, file_display_name, validate_skin,
};

const CELL_WIDTH: f32 = 84.;
const CELL_HEIGHT: f32 = 100.;
const CELL_COLUMNS: usize = 4;

#[derive(PartialEq)]
pub struct AccountSkins;

impl Component for AccountSkins {
    fn render(&self) -> impl IntoElement {
        let account = try_default_account(&use_current_account());
        let account_uuid = account.as_ref().map(|a| a.id.to_string());
        let account_name = account.as_ref().map(|a| a.username.clone());

        let library = use_state(Library::load);
        let selected = use_state(|| None::<String>);
        let url_input = use_state(String::new);
        let name_input = use_state(String::new);
        let status = use_state(|| None::<(String, bool)>);
        let busy = use_state(|| false);
        let preview = use_state(|| {
            Library::load()
                .active_bytes()
                .map(|(entry, bytes)| (bytes, entry.slim))
        });

        let selected_id = selected.read().clone();
        let selected_entry = selected_id
            .as_deref()
            .and_then(|id| library.read().entry(id).cloned());
        let active_id = library.read().active.clone();
        let busy_now = *busy.peek();
        let already_active = selected_id.is_some() && selected_id == active_id;

        rect()
            .horizontal()
            .width(Size::fill())
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .padding(40.)
            .spacing(24.)
            .child(preview_panel(
                account_uuid,
                account_name,
                selected_entry.as_ref(),
                preview.read().clone(),
                library,
                selected,
                status,
                selected_entry.is_some() && !busy_now,
                already_active,
            ))
            .child(side_panel(
                library,
                selected,
                selected_id,
                selected_entry,
                active_id,
                url_input,
                name_input,
                status,
                busy,
                preview,
            ))
    }
}

#[allow(clippy::too_many_arguments)]
fn preview_panel(
    account_uuid: Option<String>,
    account_name: Option<String>,
    selected: Option<&SkinEntry>,
    preview: Option<(Bytes, bool)>,
    library: State<Library>,
    selected_state: State<Option<String>>,
    status: State<Option<(String, bool)>>,
    equip_enabled: bool,
    already_active: bool,
) -> impl IntoElement {
    let mut model = PlayerModel::new(account_uuid.unwrap_or_default());
    if let Some((bytes, slim)) = preview {
        model = model.skin(bytes, slim);
    }

    let caption = match (selected, account_name) {
        (Some(entry), _) => format!("{} · preview", entry.name),
        (None, Some(name)) => name,
        (None, None) => "No account signed in".to_string(),
    };

    rect()
        .vertical()
        .width(Size::px(320.))
        .height(Size::fill())
        .spacing(16.)
        .child(
            rect()
                .center()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .corner_radius(CornerRadius::new_all(16.))
                .background(colors::page_elevated())
                .border(border_all_color(1., colors::component_border()))
                .child(model.into_element()),
        )
        .child(
            rect()
                .center()
                .width(Size::fill())
                .child(
                    label()
                        .text(caption)
                        .font_size(14.)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(colors::fg_primary()),
                ),
        )
        .child(
            Button::new()
                .variant(ButtonVariant::Primary)
                .width(Size::fill())
                .disabled(!equip_enabled || already_active)
                .text(if already_active { "Equipped" } else { "Equip skin" })
                .on_press(move |_| {
                    equip_selected(library, selected_state, status);
                }),
        )
        .into_element()
}

/// Points the library at the selected skin and reports the result. Shared by
/// the preview panel's Equip button and the library's "Use this skin" button.
fn equip_selected(
    mut library: State<Library>,
    selected: State<Option<String>>,
    mut status: State<Option<(String, bool)>>,
) {
    let Some(id) = selected.read().clone() else {
        status.set(Some(("Select a skin first".to_string(), true)));
        return;
    };
    library.write().set_active(Some(id));
    status.set(Some((
        "Active — offline accounts will wear it in game".to_string(),
        false,
    )));
}

#[allow(clippy::too_many_arguments)]
fn side_panel(
    mut library: State<Library>,
    mut selected: State<Option<String>>,
    selected_id: Option<String>,
    selected_entry: Option<SkinEntry>,
    active_id: Option<String>,
    url_input: State<String>,
    name_input: State<String>,
    mut status: State<Option<(String, bool)>>,
    busy: State<bool>,
    mut preview: State<Option<(Bytes, bool)>>,
) -> impl IntoElement {
    let entry_count = library.read().skins.len();
    let is_busy = *busy.peek();

    let rows: Vec<Vec<SkinEntry>> = library
        .read()
        .skins
        .clone()
        .chunks(CELL_COLUMNS)
        .map(<[SkinEntry]>::to_vec)
        .collect();

    let grid = if rows.is_empty() {
        rect()
            .center()
            .width(Size::fill())
            .height(Size::px(120.))
            .child(
                label()
                    .text("Import a skin PNG, paste an image URL, or add a Mojang player name.")
                    .font_size(13.)
                    .color(colors::fg_secondary()),
            )
            .into_element()
    } else {
        let rows = rows.into_iter().map(|row| {
            let cells = row.into_iter().map(|entry| {
                let id = entry.id.clone();
                let is_active = active_id.as_deref() == Some(entry.id.as_str());
                let is_selected = selected_id.as_deref() == Some(entry.id.as_str());

                skin_cell(entry.name, is_active, is_selected, move |_| {
                    select_skin(id.clone(), selected, preview, library);
                })
                .into_element()
            });

            rect().horizontal().spacing(12.).children(cells)
        });

        ScrollArea::new()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .scrollbar_gutter(true)
            .child(rect().vertical().spacing(12.).children(rows))
            .into_element()
    };

    let remove_enabled = selected_entry.is_some() && !is_busy;
    let use_enabled = selected_entry.is_some() && !is_busy;

    rect()
        .vertical()
        .width(Size::flex(1.0))
        .height(Size::fill())
        .overflow(Overflow::Clip)
        .spacing(16.)
        .child(
            label()
                .text("Skins")
                .font_size(32.)
                .font_weight(FontWeight::BOLD)
                .color(colors::fg_primary()),
        )
        .child(
            rect().horizontal().spacing(12.).child(import_button(
                library, selected, status, preview, busy,
            )),
        )
        .child(section_label("ADD BY URL"))
        .child(
            rect()
                .horizontal()
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .spacing(12.)
                .child(
                    TextInput::new(url_input)
                        .placeholder("https://…/skin.png")
                        .width(Size::flex(1.0))
                        .enabled(!is_busy),
                )
                .child({
                    let add_url_status = status;
                    Button::new()
                        .variant(ButtonVariant::Primary)
                        .disabled(is_busy)
                        .text("SET")
                        .on_press(move |_| {
                            let url = url_input.read().clone();
                            add_from_url(
                                url,
                                library,
                                selected,
                                add_url_status,
                                preview,
                                busy,
                            );
                        })
                }),
        )
        .child(section_label("ADD BY PLAYER NAME"))
        .child(
            rect()
                .horizontal()
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .spacing(12.)
                .child(
                    TextInput::new(name_input)
                        .placeholder("e.g. Notch")
                        .width(Size::flex(1.0))
                        .enabled(!is_busy),
                )
                .child({
                    let add_name_status = status;
                    Button::new()
                        .variant(ButtonVariant::Primary)
                        .disabled(is_busy)
                        .text("SET")
                        .on_press(move |_| {
                            let name = name_input.read().clone();
                            add_from_name(
                                name,
                                library,
                                selected,
                                add_name_status,
                                preview,
                                busy,
                            );
                        })
                }),
        )
        .maybe_child(status.read().clone().map(|(text, is_error)| {
            label()
                .text(text)
                .font_size(12.)
                .color(if is_error {
                    colors::danger()
                } else {
                    colors::fg_secondary()
                })
                .into_element()
        }))
        .child(section_label(format!("SKIN LIBRARY · {entry_count}")))
        .child(grid)
        .child(
            rect()
                .horizontal()
                .spacing(12.)
                .child(
                    Button::new()
                        .variant(ButtonVariant::Primary)
                        .disabled(!use_enabled)
                        .text("Use this skin")
                        .on_press(move |_| equip_selected(library, selected, status)),
                )
                .child(
                    Button::new()
                        .variant(ButtonVariant::Secondary)
                        .disabled(!remove_enabled)
                        .text("Remove")
                        .on_press(move |_| {
                            let Some(id) = selected.read().clone() else {
                                return;
                            };
                            library.write().remove(&id);
                            selected.set(None);
                            preview.set(
                                library
                                    .read()
                                    .active_bytes()
                                    .map(|(entry, bytes)| (bytes, entry.slim)),
                            );
                            status.set(Some(("Skin removed".to_string(), false)));
                        }),
                ),
        )
        .child(
            label()
                .text("The active skin installs CustomSkinLoader into your versions automatically when you launch. Custom skins apply to offline accounts on 1.8 – 26.3.")
                .font_size(12.)
                .color(colors::fg_secondary()),
        )
        .into_element()
}

fn import_button(
    mut library: State<Library>,
    mut selected: State<Option<String>>,
    mut status: State<Option<(String, bool)>>,
    mut preview: State<Option<(Bytes, bool)>>,
    mut busy: State<bool>,
) -> impl IntoElement {
    Button::new()
        .variant(ButtonVariant::Primary)
        .disabled(*busy.peek())
        .child(
            Icon::new(IconType::Plus)
                .size(16.)
                .color(colors::fg_primary()),
        )
        .text("Import Skin")
        .on_press(move |_| {
            spawn(async move {
                busy.set(true);
                status.set(Some(("Opening the file picker…".to_string(), false)));

                let dialog = rfd::AsyncFileDialog::new()
                    .set_title("Choose a skin PNG")
                    .add_filter("Skin image", &["png"]);

                let Some(handle) = dialog.pick_file().await else {
                    tracing::warn!("skin file dialog closed without a selection");
                    status.set(Some(("No file chosen".to_string(), false)));
                    busy.set(false);
                    return;
                };

                status.set(None);

                let file_name = handle.file_name();
                let bytes = Bytes::from(handle.read().await);

                match validate_skin(&bytes) {
                    Ok(slim) => {
                        let entry_name = file_display_name(&file_name);
                        let added = library.write().add(
                            entry_name,
                            file_name,
                            SkinKind::File,
                            slim,
                            &bytes,
                        );
                        match added {
                            Ok(id) => {
                                library.write().set_active(Some(id.clone()));
                                selected.set(Some(id));
                                preview.set(Some((bytes, slim)));
                                status.set(Some((
                                    "Imported and set as your active skin".to_string(),
                                    false,
                                )));
                            }
                            Err(err) => status.set(Some((err, true))),
                        }
                    }
                    Err(err) => status.set(Some((err, true))),
                }

                busy.set(false);
            });
        })
}

fn select_skin(
    id: String,
    mut selected: State<Option<String>>,
    mut preview: State<Option<(Bytes, bool)>>,
    library: State<Library>,
) {
    selected.set(Some(id.clone()));
    match library.read().entry_bytes(&id) {
        Some((entry, bytes)) => preview.set(Some((bytes, entry.slim))),
        None => preview.set(None),
    }
}

fn add_from_url(
    url: String,
    mut library: State<Library>,
    mut selected: State<Option<String>>,
    mut status: State<Option<(String, bool)>>,
    mut preview: State<Option<(Bytes, bool)>>,
    mut busy: State<bool>,
) {
    if url.trim().is_empty() {
        status.set(Some(("Paste a skin image URL first".to_string(), true)));
        return;
    }

    spawn(async move {
        busy.set(true);
        status.set(None);

        match fetch_url(&url).await {
            Ok((bytes, slim)) => {
                let display = url
                    .trim()
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .map(file_display_name)
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "URL skin".to_string());

                let added = library
                    .write()
                    .add(display, url.trim().to_string(), SkinKind::Url, slim, &bytes);
                match added {
                    Ok(id) => {
                        library.write().set_active(Some(id.clone()));
                        selected.set(Some(id));
                        preview.set(Some((bytes, slim)));
                        status.set(Some((
                            "Saved to your library and set as your active skin".to_string(),
                            false,
                        )));
                    }
                    Err(err) => status.set(Some((err, true))),
                }
            }
            Err(err) => status.set(Some((err, true))),
        }

        busy.set(false);
    });
}

fn add_from_name(
    name: String,
    mut library: State<Library>,
    mut selected: State<Option<String>>,
    mut status: State<Option<(String, bool)>>,
    mut preview: State<Option<(Bytes, bool)>>,
    mut busy: State<bool>,
) {
    if name.trim().is_empty() {
        status.set(Some(("Enter a player name first".to_string(), true)));
        return;
    }

    spawn(async move {
        busy.set(true);
        status.set(None);

        match fetch_by_name(&name).await {
            Ok((bytes, slim, canonical)) => {
                let added = library.write().add(
                    canonical.clone(),
                    name.trim().to_string(),
                    SkinKind::Name,
                    slim,
                    &bytes,
                );
                match added {
                    Ok(id) => {
                        library.write().set_active(Some(id.clone()));
                        selected.set(Some(id));
                        preview.set(Some((bytes, slim)));
                        status.set(Some((
                            format!("Skin of \"{canonical}\" saved and set as your active skin"),
                            false,
                        )));
                    }
                    Err(err) => status.set(Some((err, true))),
                }
            }
            Err(err) => status.set(Some((err, true))),
        }

        busy.set(false);
    });
}

fn section_label(text: impl Into<String>) -> impl IntoElement {
    label()
        .text(text.into())
        .font_size(11.)
        .font_weight(FontWeight::SEMI_BOLD)
        .color(colors::fg_secondary())
}

fn skin_cell(
    name: String,
    active: bool,
    selected: bool,
    on_press: impl Fn(()) + 'static,
) -> impl IntoElement {
    let border = if selected {
        colors::brand()
    } else {
        colors::component_border()
    };

    rect()
        .vertical()
        .center()
        .spacing(4.)
        .width(Size::px(CELL_WIDTH))
        .height(Size::px(CELL_HEIGHT))
        .corner_radius(CornerRadius::new_all(8.))
        .background(colors::page_elevated())
        .border(border_all_color(if selected { 2. } else { 1. }, border))
        .cursor(CursorIcon::Pointer)
        .on_press(move |_| on_press(()))
        .child(Icon::new(IconType::Users01).size(24.).color(if active {
            colors::brand()
        } else {
            colors::fg_secondary()
        }))
        .child(
            label()
                .text(truncate(&name, 11))
                .font_size(10.)
                .color(colors::fg_primary()),
        )
        .child(if active {
            label()
                .text("ACTIVE")
                .font_size(8.)
                .font_weight(FontWeight::BOLD)
                .color(colors::brand())
                .into_element()
        } else {
            label().text("").font_size(8.).into_element()
        })
        .into_element()
}

fn truncate(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let cut: String = name.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}
