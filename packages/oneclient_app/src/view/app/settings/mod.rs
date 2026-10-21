mod accounts;
mod appearance;

mod apis;
mod changelog;
mod developer;
mod java;
mod language;
mod launcher;
mod minecraft;
mod storage;

use std::cell::RefCell;

use freya::prelude::*;

pub use accounts::SettingsAccounts;
pub use apis::SettingsApis;
pub use appearance::SettingsAppearance;
pub use changelog::SettingsChangelog;
pub use developer::SettingsDeveloper;
pub use java::SettingsJava;
pub use language::SettingsLanguage;
pub use launcher::SettingsLauncher;
pub use minecraft::SettingsMinecraft;
pub use storage::SettingsStorage;

use crate::{
    components::{Button, Icon, IconType},
    layout::{pending_setting_focus, settings_scroll},
    theme::colors,
    ui::border_all_color,
};

/// The last search jump that already scrolled, so re-measuring the row mid-highlight
/// does not yank the scroll position again.
thread_local! {
    static FOCUS_SCROLLED: RefCell<Option<&'static str>> = const { RefCell::new(None) };
}

/// Distance the search-jump target lands below the top of the settings scroll area
const FOCUS_SCROLL_MARGIN_PX: f32 = 24.;

/// The search bar publishes its jump target through the settings shell. A jump always
/// remounts the page through the search overlay, so this re-reads the target on mount,
/// re-arms the scroll claim, and re-renders once the highlight window expires.
pub fn use_setting_focus() {
    let pending = pending_setting_focus();
    let armed = use_state(|| pending);
    let current = *armed.read();

    let mut first = use_state(|| true);
    if *first.peek() {
        first.set(false);
        FOCUS_SCROLLED.with(|cell| *cell.borrow_mut() = None);
    }

    use_side_effect_with_deps(&current, move |pending| {
        if pending.is_none() {
            return;
        }
        let mut armed = armed;
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            // The page may already be gone because the user started searching again
            if let Some(mut value) = armed.try_write() {
                *value = None;
            }
        });
    });
}

/// Scrolls the search-jump target near the top of the settings scroll area, once per jump
fn scroll_setting_into_view(id: &'static str, row_top: f32) {
    if pending_setting_focus() != Some(id) {
        return;
    }

    let claimed = FOCUS_SCROLLED.with(|cell| {
        let already = *cell.borrow();
        if already == Some(id) {
            false
        } else {
            *cell.borrow_mut() = Some(id);
            true
        }
    });
    if !claimed {
        return;
    }

    let Some((ctx, mut controller)) = settings_scroll() else {
        return;
    };
    let row_in_content = row_top - ctx.viewport_top - ctx.corrected_y;
    controller.scroll_to_y((FOCUS_SCROLL_MARGIN_PX - row_in_content) as i32);
}

pub fn settings_page() -> Rect {
    use_setting_focus();
    rect().vertical().width(Size::fill()).spacing(4.)
}

pub fn section_header(text: &'static str) -> impl IntoElement {
    rect()
        .padding(Gaps::new(16., 0., 8., 2.))
        .child(
            label()
                .text(text)
                .font_size(13.)
                .font_weight(FontWeight::MEDIUM)
                .color(colors::fg_secondary()),
        )
        .into_element()
}

pub fn settings_row(
    icon: IconType,
    title: &'static str,
    description: impl Into<String>,
    trailing: impl IntoElement,
) -> impl IntoElement {
    settings_row_inner(icon, title, description, trailing, RowTone::Normal, None)
}

pub fn settings_row_disabled(
    icon: IconType,
    title: &'static str,
    description: impl Into<String>,
    trailing: impl IntoElement,
) -> impl IntoElement {
    settings_row_inner(icon, title, description, trailing, RowTone::Disabled, None)
}

/// For destructive actions, tinted red like the other danger notices
pub fn settings_row_danger(
    icon: IconType,
    title: &'static str,
    description: impl Into<String>,
    trailing: impl IntoElement,
) -> impl IntoElement {
    settings_row_inner(icon, title, description, trailing, RowTone::Danger, None)
}

/// Same as [`settings_row`] but carries the settings-search id so a search jump can
/// highlight the row and scroll it into view
pub fn settings_row_focus(
    id: &'static str,
    icon: IconType,
    title: &'static str,
    description: impl Into<String>,
    trailing: impl IntoElement,
) -> impl IntoElement {
    settings_row_inner(
        icon,
        title,
        description,
        trailing,
        RowTone::Normal,
        Some(id),
    )
}

/// Disabled twin of [`settings_row_focus`]
pub fn settings_row_disabled_focus(
    id: &'static str,
    icon: IconType,
    title: &'static str,
    description: impl Into<String>,
    trailing: impl IntoElement,
) -> impl IntoElement {
    settings_row_inner(
        icon,
        title,
        description,
        trailing,
        RowTone::Disabled,
        Some(id),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum RowTone {
    Normal,
    Disabled,
    Danger,
}

fn settings_row_inner(
    icon: IconType,
    title: &'static str,
    description: impl Into<String>,
    trailing: impl IntoElement,
    tone: RowTone,
    focus: Option<&'static str>,
) -> impl IntoElement {
    let description = description.into();
    let danger = tone == RowTone::Danger;
    let focused = focus.is_some_and(|id| pending_setting_focus() == Some(id));

    let mut row = rect()
        .maybe(tone == RowTone::Disabled, |el| el.opacity(0.4))
        .horizontal()
        .width(Size::fill())
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(16.)
        .padding(Gaps::new_symmetric(12., 16.))
        .corner_radius(CornerRadius::new_all(12.))
        .background(if danger {
            colors::danger().with_a(26)
        } else {
            colors::page_elevated()
        })
        .maybe(danger, |el| {
            el.border(border_all_color(1., colors::danger().with_a(128)))
        })
        .child(Icon::new(icon).maybe(danger, |icon| icon.color(colors::danger())))
        .child(
            rect()
                .vertical()
                .width(Size::flex(1.0))
                .spacing(2.)
                .child(
                    label()
                        .text(title)
                        .font_size(16.)
                        .font_weight(FontWeight::MEDIUM)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text(description)
                        .font_size(12.)
                        .color(colors::fg_secondary()),
                ),
        )
        .child(trailing);

    if let Some(id) = focus {
        row = row
            .on_sized(move |e: Event<SizedEventData>| scroll_setting_into_view(id, e.area.min_y()));
    }

    if focused {
        row = row.border(border_all_color(2., colors::brand()));
    }

    row.into_element()
}

#[derive(PartialEq)]
struct ResetButton<T: Clone + PartialEq + 'static> {
    values: Vec<(State<T>, T)>,
}

impl<T: Clone + PartialEq + 'static> Component for ResetButton<T> {
    fn render(&self) -> impl IntoElement {
        let at_default = self
            .values
            .iter()
            .all(|(value, default)| *value.read() == *default);

        let values = self.values.clone();

        Button::new()
            .ghost()
            .icon()
            .disabled(at_default)
            .alt(if at_default {
                "Already at the default"
            } else {
                "Reset to default"
            })
            .on_press(move |_| {
                for (value, default) in &values {
                    let mut value = *value;
                    value.set(default.clone());
                }
            })
            .child(
                Icon::new(IconType::RefreshCcw02)
                    .size(14.)
                    .color(colors::fg_secondary()),
            )
    }
}

pub fn resettable<T: Clone + PartialEq + 'static>(
    control: impl IntoElement,
    value: State<T>,
    default: T,
) -> impl IntoElement {
    resettable_all(control, vec![(value, default)])
}

pub fn resettable_all<T: Clone + PartialEq + 'static>(
    control: impl IntoElement,
    values: Vec<(State<T>, T)>,
) -> impl IntoElement {
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(8.)
        .child(control)
        .child(ResetButton { values })
        .into_element()
}
