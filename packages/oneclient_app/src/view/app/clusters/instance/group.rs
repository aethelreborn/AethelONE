use freya::prelude::*;

use crate::components::{Button, Icon, IconType, OverlayPopup, TextInput};
use crate::hooks::{set_cluster_group, use_dispatch};
use crate::theme::colors;
use crate::ui::border_all_color;

const DIALOG_WIDTH: f32 = 460.;

#[derive(PartialEq)]
pub struct GroupInstanceModal {
    cluster_id: i64,
    name: String,
    current: Option<String>,
    existing: Vec<String>,
    on_close: EventHandler<()>,
}

impl GroupInstanceModal {
    pub fn new(
        cluster_id: i64,
        name: String,
        current: Option<String>,
        existing: Vec<String>,
        on_close: impl Into<EventHandler<()>>,
    ) -> Self {
        Self {
            cluster_id,
            name,
            current,
            existing,
            on_close: on_close.into(),
        }
    }
}

impl Component for GroupInstanceModal {
    fn render(&self) -> impl IntoElement {
        let dispatch = use_dispatch();
        let cluster_id = self.cluster_id;
        let mut input = use_state(|| self.current.clone().unwrap_or_default());
        let mut saving = use_state(|| false);
        let close_scrim = self.on_close.clone();

        let submit = {
            let dispatch = dispatch.clone();
            let close = self.on_close.clone();
            move |group: Option<String>| {
                if *saving.read() {
                    return;
                }
                saving.set(true);
                let dispatch = dispatch.clone();
                let close = close.clone();
                spawn(async move {
                    match set_cluster_group(cluster_id, group).await {
                        Ok(()) => close.call(()),
                        Err(err) => {
                            dispatch
                                .notify("Couldn't move instance")
                                .body(err.to_string())
                                .error()
                                .send();
                        }
                    }
                    saving.set(false);
                });
            }
        };
        let submit_click = submit.clone();
        let submit_enter = submit.clone();

        let typed = move || {
            let text = input.read().trim().to_string();
            (!text.is_empty()).then_some(text)
        };

        let others: Vec<String> = self
            .existing
            .iter()
            .filter(|name| Some(name.as_str()) != self.current.as_deref())
            .cloned()
            .collect();

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
                            .width(Size::px(DIALOG_WIDTH))
                            .max_width(Size::window_percent(92.))
                            .spacing(14.)
                            .padding(Gaps::new_all(20.))
                            .corner_radius(CornerRadius::new_all(16.))
                            .background(colors::page_elevated())
                            .border(border_all_color(1., colors::component_border()))
                            .child(
                                label()
                                    .text("Move to group")
                                    .font_size(18.)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(colors::fg_primary()),
                            )
                            .child(
                                label()
                                    .text(format!(
                                        "{} goes into a folder on the Versions page. Leave the name empty to take it back out.",
                                        self.name
                                    ))
                                    .font_size(12.)
                                    .color(colors::fg_secondary()),
                            )
                            .child(
                                TextInput::new(input)
                                    .width(Size::fill())
                                    .placeholder("Group name")
                                    .on_submit(move |_| submit_enter.clone()(typed())),
                            )
                            .maybe_child((!others.is_empty()).then(|| {
                                rect()
                                    .vertical()
                                    .width(Size::fill())
                                    .spacing(6.)
                                    .child(
                                        label()
                                            .text("Existing groups")
                                            .font_size(12.)
                                            .font_weight(FontWeight::MEDIUM)
                                            .color(colors::fg_secondary()),
                                    )
                                    .children(others.iter().map(|name| {
                                        let name = name.clone();
                                        Button::new()
                                            .secondary()
                                            .small()
                                            .child(Icon::new(IconType::Folder).size(12.))
                                            .text(name.clone())
                                            .on_press(move |_| {
                                                input.set(name.clone());
                                            })
                                            .into_element()
                                    }))
                                    .into_element()
                            }))
                            .child(
                                rect()
                                    .horizontal()
                                    .width(Size::fill())
                                    .main_align(Alignment::End)
                                    .spacing(8.)
                                    .child(
                                        Button::new()
                                            .ghost()
                                            .text("Cancel")
                                            .on_press({
                                                let close = self.on_close.clone();
                                                move |_| close.call(())
                                            }),
                                    )
                                    .child(
                                        Button::new()
                                            .primary()
                                            .enabled(!*saving.read())
                                            .text("Save")
                                            .on_press(move |_| {
                                                let group = {
                                                    let text = input.read().trim().to_string();
                                                    (!text.is_empty()).then_some(text)
                                                };
                                                submit_click.clone()(group);
                                            }),
                                    ),
                            ),
                    ),
            )
    }
}
