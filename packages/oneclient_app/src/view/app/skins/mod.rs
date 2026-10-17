use bytes::Bytes;
use freya::prelude::*;

use crate::components::{
    Button, ButtonVariant, Icon, IconType, PlayerModel, ScrollArea, Segment, SegmentedControl,
    TextInput,
};
use crate::hooks::{invalidate_local_skin_queries, try_default_account, use_current_account};
use crate::theme::colors;
use crate::ui::border_all_color;

pub mod inject;
pub mod library;

use library::{
    Library, SkinEntry, SkinKind, TextureKind, fetch_by_name, fetch_url, file_display_name,
    validate_texture,
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
        let texture_kind = use_state(|| TextureKind::Skin);
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
        // Slim/Classic override for the selected skin; synced from the entry
        // whenever the selection changes
        let mut model_slim = use_state(|| {
            Library::load()
                .active_bytes()
                .map(|(entry, _)| entry.slim)
                .unwrap_or(true)
        });

        let selected_id = selected.read().clone();
        let selected_entry = selected_id
            .as_deref()
            .and_then(|id| library.read().entry(id).cloned());
        let already_active = selected_entry.as_ref().is_some_and(|entry| {
            library.read().active_for(entry.texture) == Some(entry.id.as_str())
        });
        let busy_now = *busy.peek();

        use_side_effect_with_deps(&selected_id, move |id| {
            let library = library.read();
            let slim = id
                .as_deref()
                .and_then(|id| library.entry(id))
                .filter(|entry| entry.texture == TextureKind::Skin)
                .map(|entry| entry.slim);
            if let Some(slim) = slim {
                model_slim.set(slim);
            }
        });

        let slim_now = *model_slim.read();
        use_side_effect_with_deps(&slim_now, move |slim| {
            apply_model_slim(
                library,
                selected,
                preview,
                status,
                selected.read().clone(),
                *slim,
            );
        });

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
                model_slim,
                library,
                selected,
                status,
                selected_entry.is_some() && !busy_now,
                already_active,
            ))
            .child(side_panel(
                library,
                texture_kind,
                selected,
                selected_id,
                selected_entry,
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
    model_slim: State<bool>,
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
        (Some(entry), _) if entry.texture == TextureKind::Skin => {
            format!("{} · preview", entry.name)
        }
        (Some(entry), _) => format!("{} selected", entry.name),
        (None, Some(name)) => name,
        (None, None) => "No account signed in".to_string(),
    };

    let slim_control_enabled = selected.is_some_and(|entry| entry.texture == TextureKind::Skin);
    let equip_label = match selected {
        Some(entry) if already_active => format!("{} equipped", capitalize(entry.texture.label())),
        Some(entry) => format!("Equip {}", entry.texture.label()),
        None => "Equip skin".to_string(),
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
            rect().center().width(Size::fill()).child(
                label()
                    .text(caption)
                    .font_size(14.)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(colors::fg_primary()),
            ),
        )
        .child(
            rect()
                .horizontal()
                .main_align(Alignment::Center)
                .width(Size::fill())
                .child(
                    SegmentedControl::new(model_slim)
                        .height(32.)
                        .equal_width(72.)
                        .disabled(!slim_control_enabled)
                        .segment(Segment::new(true).label("Slim"))
                        .segment(Segment::new(false).label("Classic")),
                ),
        )
        .child(
            Button::new()
                .variant(ButtonVariant::Primary)
                .width(Size::fill())
                .disabled(!equip_enabled || already_active)
                .text(equip_label)
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
        status.set(Some(("Select a texture first".to_string(), true)));
        return;
    };
    let texture = library
        .read()
        .entry(&id)
        .map(|entry| entry.texture)
        .unwrap_or_default();
    library.write().set_active(Some(id));
    invalidate_local_skin();
    let message = match texture {
        TextureKind::Skin => format!(
            "{} equipped — offline accounts will wear it in game",
            capitalize(texture.label())
        ),
        _ => format!(
            "{} equipped — worn by offline accounts, and by premium accounts that have none of their own",
            capitalize(texture.label())
        ),
    };
    status.set(Some((message, false)));
}

/// Avatars read the active skin from the library through a query, so they
/// need a nudge after any change to it
fn invalidate_local_skin() {
    spawn(invalidate_local_skin_queries());
}

/// Persists the Slim/Classic override onto the selected entry and keeps the
/// preview (and anything else wearing the skin) on the new model
fn apply_model_slim(
    mut library: State<Library>,
    selected: State<Option<String>>,
    mut preview: State<Option<(Bytes, bool)>>,
    mut status: State<Option<(String, bool)>>,
    id: Option<String>,
    slim: bool,
) {
    let Some(id) = id else {
        return;
    };
    if !library.write().set_slim(&id, slim) {
        return;
    }
    let current = preview.read().clone();
    if let Some((bytes, _)) = current {
        preview.set(Some((bytes, slim)));
    }
    invalidate_local_skin();
    status.set(Some((
        if slim {
            "Model set to Slim".to_string()
        } else {
            "Model set to Classic".to_string()
        },
        false,
    )));
}

#[allow(clippy::too_many_arguments)]
fn side_panel(
    mut library: State<Library>,
    texture_kind: State<TextureKind>,
    mut selected: State<Option<String>>,
    selected_id: Option<String>,
    selected_entry: Option<SkinEntry>,
    url_input: State<String>,
    name_input: State<String>,
    mut status: State<Option<(String, bool)>>,
    busy: State<bool>,
    mut preview: State<Option<(Bytes, bool)>>,
) -> impl IntoElement {
    let kind = *texture_kind.read();
    let snapshot = library.read().clone();
    let entries = snapshot.entries_for(kind);
    let entry_count = entries.len();
    let is_busy = *busy.peek();

    let rows: Vec<Vec<SkinEntry>> = entries
        .chunks(CELL_COLUMNS)
        .map(<[SkinEntry]>::to_vec)
        .collect();

    let grid = if rows.is_empty() {
        let hint = match kind {
            TextureKind::Skin => {
                "Import a skin PNG, paste an image URL, or add a Mojang player name."
            }
            TextureKind::Cape => "Import a cape PNG or paste an image URL.",
            TextureKind::Elytra => "Import an elytra PNG or paste an image URL.",
        };
        rect()
            .center()
            .width(Size::fill())
            .height(Size::px(120.))
            .child(
                label()
                    .text(hint)
                    .font_size(13.)
                    .color(colors::fg_secondary()),
            )
            .into_element()
    } else {
        let rows = rows.into_iter().map(|row| {
            let cells = row.into_iter().map(|entry| {
                let id = entry.id.clone();
                let is_active = snapshot.active_for(entry.texture) == Some(entry.id.as_str());
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
    let use_label = format!(
        "Use this {}",
        selected_entry
            .as_ref()
            .map(|entry| entry.texture.label())
            .unwrap_or("skin")
    );

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
            rect().horizontal().child(
                SegmentedControl::new(texture_kind)
                    .height(32.)
                    .equal_width(84.)
                    .segment(Segment::new(TextureKind::Skin).label("Skin"))
                    .segment(Segment::new(TextureKind::Cape).label("Cape"))
                    .segment(Segment::new(TextureKind::Elytra).label("Elytra")),
            ),
        )
        .child(
            rect()
                .horizontal()
                .spacing(12.)
                .child(import_button(
                    library,
                    selected,
                    status,
                    preview,
                    busy,
                    texture_kind,
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
                        .placeholder(format!("https://…/{}.png", kind.label()))
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
                                kind,
                            );
                        })
                }),
        )
        .maybe_child((kind == TextureKind::Skin).then(|| {
            rect()
                .vertical()
                .spacing(16.)
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
                .into_element()
        }))
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
        .child(section_label(format!(
            "{} LIBRARY · {entry_count}",
            capitalize(kind.label()).to_uppercase()
        )))
        .child(grid)
        .child(
            rect()
                .horizontal()
                .spacing(12.)
                .child(
                    Button::new()
                        .variant(ButtonVariant::Primary)
                        .disabled(!use_enabled)
                        .text(use_label)
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
                            let texture = library
                                .read()
                                .entry(&id)
                                .map(|entry| entry.texture)
                                .unwrap_or_default();
                            library.write().remove(&id);
                            selected.set(None);
                            preview.set(
                                library
                                    .read()
                                    .active_bytes()
                                    .map(|(entry, bytes)| (bytes, entry.slim)),
                            );
                            invalidate_local_skin();
                            status.set(Some((
                                format!("{} removed", capitalize(texture.label())),
                                false,
                            )));
                        }),
                ),
        )
        .child(
            label()
                .text("The equipped skin, cape and elytra install CustomSkinLoader into your versions automatically when you launch. Skins apply to offline accounts; capes and elytras also fill in for premium accounts that have none of their own — 1.8 – 26.3.")
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
    texture_kind: State<TextureKind>,
) -> impl IntoElement {
    let kind = *texture_kind.read();
    Button::new()
        .variant(ButtonVariant::Primary)
        .disabled(*busy.peek())
        .child(
            Icon::new(IconType::Plus)
                .size(16.)
                .color(colors::fg_primary()),
        )
        .text(format!("Import {}", capitalize(kind.label())))
        .on_press(move |_| {
            spawn(async move {
                busy.set(true);
                status.set(Some(("Opening the file picker…".to_string(), false)));

                let dialog = rfd::AsyncFileDialog::new()
                    .set_title(format!("Choose a {} PNG", kind.label()))
                    .add_filter("PNG image", &["png"]);

                let Some(handle) = dialog.pick_file().await else {
                    tracing::warn!("skin file dialog closed without a selection");
                    status.set(Some(("No file chosen".to_string(), false)));
                    busy.set(false);
                    return;
                };

                status.set(None);

                let file_name = handle.file_name();
                let bytes = Bytes::from(handle.read().await);

                match validate_texture(&bytes, kind) {
                    Ok(slim) => {
                        let entry_name = file_display_name(&file_name);
                        let added = library.write().add(
                            entry_name,
                            file_name,
                            SkinKind::File,
                            kind,
                            slim,
                            &bytes,
                        );
                        match added {
                            Ok(id) => {
                                library.write().set_active(Some(id.clone()));
                                invalidate_local_skin();
                                selected.set(Some(id));
                                if kind == TextureKind::Skin {
                                    preview.set(Some((bytes, slim)));
                                }
                                status.set(Some((
                                    format!("Imported and set as your active {}", kind.label()),
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
        Some((entry, bytes)) if entry.texture == TextureKind::Skin => {
            preview.set(Some((bytes, entry.slim)));
        }
        // Capes and elytras have no model preview, so the skin preview stays
        Some(_) => {}
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
    kind: TextureKind,
) {
    if url.trim().is_empty() {
        status.set(Some((
            format!("Paste a {} image URL first", kind.label()),
            true,
        )));
        return;
    }

    spawn(async move {
        busy.set(true);
        status.set(None);

        match fetch_url(&url, kind).await {
            Ok((bytes, slim)) => {
                let display = url
                    .trim()
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .map(file_display_name)
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("URL {}", kind.label()));

                let added = library.write().add(
                    display,
                    url.trim().to_string(),
                    SkinKind::Url,
                    kind,
                    slim,
                    &bytes,
                );
                match added {
                    Ok(id) => {
                        library.write().set_active(Some(id.clone()));
                        invalidate_local_skin();
                        selected.set(Some(id));
                        if kind == TextureKind::Skin {
                            preview.set(Some((bytes, slim)));
                        }
                        status.set(Some((
                            format!(
                                "Saved to your library and set as your active {}",
                                kind.label()
                            ),
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
                    TextureKind::Skin,
                    slim,
                    &bytes,
                );
                match added {
                    Ok(id) => {
                        library.write().set_active(Some(id.clone()));
                        invalidate_local_skin();
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

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
