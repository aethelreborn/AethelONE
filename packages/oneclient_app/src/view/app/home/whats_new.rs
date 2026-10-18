use freya::prelude::*;
use freya::router::RouterContext;

use crate::components::{Button, Icon, IconType, Markdown, MarkdownStyle, ScrollArea};
use crate::hooks::{
    changelog_entries, latest_changelog_version, use_changelog, use_settings_snapshot,
};
use crate::routes::Route;
use crate::theme::colors;
use crate::ui::border_all_color;

/// Shown on Home only while the newest changelog entry has not been opened;
/// reading it in Settings makes the card disappear on its own
#[derive(PartialEq)]
pub struct WhatsNew;

impl Component for WhatsNew {
    fn render(&self) -> impl IntoElement {
        let query = use_changelog();
        let settings = use_settings_snapshot().settings;

        let Some(version) = latest_changelog_version(&query) else {
            return rect().into_element();
        };
        if settings.seen_changelog_version.as_deref() == Some(version.as_str()) {
            return rect().into_element();
        }
        let Some(entry) = changelog_entries(&query).and_then(|entries| entries.into_iter().next())
        else {
            return rect().into_element();
        };
        let has_body = !entry.body.trim().is_empty();

        rect()
            .vertical()
            .width(Size::fill())
            .spacing(10.)
            .padding(Gaps::new_symmetric(14., 18.))
            .corner_radius(CornerRadius::new_all(14.))
            .background(colors::page_elevated())
            .border(border_all_color(1., colors::component_border()))
            .child(
                rect()
                    .horizontal()
                    .width(Size::fill())
                    .cross_align(Alignment::Center)
                    .content(Content::Flex)
                    .spacing(10.)
                    .child(
                        Icon::new(IconType::Rocket02)
                            .size(16.)
                            .color(colors::brand()),
                    )
                    .child(
                        label()
                            .text(format!("What's new in {}", entry.version))
                            .font_size(15.)
                            .font_weight(FontWeight::MEDIUM)
                            .width(Size::flex(1.0))
                            .color(colors::fg_primary()),
                    )
                    .child(
                        Button::new()
                            .secondary()
                            .small()
                            .text("Read it all")
                            .on_press(move |_| {
                                let _ = RouterContext::get().push(Route::SettingsChangelog {});
                            }),
                    ),
            )
            .maybe_child(has_body.then(|| {
                rect()
                    .width(Size::fill())
                    .height(Size::px(110.))
                    .overflow(Overflow::Clip)
                    .child(
                        ScrollArea::new()
                            .width(Size::fill())
                            .height(Size::fill())
                            .child(Markdown::new(entry.body.clone()).width(Size::fill()).style(
                                MarkdownStyle {
                                    color: colors::fg_primary(),
                                    color_link: colors::code_info(),
                                    color_code: colors::fg_primary(),
                                    background_code: colors::component_bg(),
                                    background_blockquote: colors::component_bg(),
                                    border_blockquote: colors::brand(),
                                    background_divider: colors::component_border(),
                                    headings: [18., 16., 14., 13., 12., 12.],
                                    paragraph_size: 12.,
                                    code_font_size: 11.,
                                    ..MarkdownStyle::default()
                                },
                            )),
                    )
                    .into_element()
            }))
            .into_element()
    }
}
