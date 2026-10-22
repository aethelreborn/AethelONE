use chrono::{DateTime, NaiveDate, Utc};
use freya::prelude::*;

use oneclient_core::game::{PlaySession, group_sessions_by_day};

use crate::components::Dropdown;
use crate::hooks::{try_recent_sessions, use_recent_sessions};
use crate::theme::colors;
use crate::utils::format_duration;

const SESSION_LIMIT: i64 = 80;

/// The ranges the timeline can show, in days; zero means every session
const RANGES: [(i64, &str); 4] = [(7, "7 days"), (30, "30 days"), (90, "90 days"), (0, "All")];

#[derive(PartialEq)]
pub struct SessionsTimeline {
    cluster_id: Option<i64>,
}

impl SessionsTimeline {
    pub fn new(cluster_id: Option<i64>) -> Self {
        Self { cluster_id }
    }
}

impl Component for SessionsTimeline {
    fn render(&self) -> impl IntoElement {
        let query = use_recent_sessions(self.cluster_id, SESSION_LIMIT);
        let sessions = try_recent_sessions(&query);
        let mut range = use_state(|| 0i64);
        let days = *range.read();
        let Some(sessions) = sessions.filter(|list| !list.is_empty()) else {
            return rect().into_element();
        };

        let today = Utc::now().date_naive();
        let groups: Vec<(String, Vec<&PlaySession>)> = group_sessions_by_day(&sessions)
            .into_iter()
            .filter(|(day, _)| within_range(day, days, today))
            .collect();

        if groups.is_empty() {
            return rect().into_element();
        }

        let picker = {
            let mut range = range;
            let current = RANGES
                .iter()
                .find(|(span, _)| *span == days)
                .map(|(_, label)| (*label).to_string())
                .unwrap_or_else(|| "All".to_string());
            let labels: Vec<String> = RANGES
                .iter()
                .map(|(_, label)| (*label).to_string())
                .collect();

            Dropdown::new(current, labels)
                .width(Size::px(104.))
                .height(Size::px(28.))
                .on_select(move |idx: usize| {
                    if let Some((span, _)) = RANGES.get(idx) {
                        range.set(*span);
                    }
                })
                .into_element()
        };

        let chart = rect()
            .vertical()
            .width(Size::fill())
            .spacing(16.)
            .children(groups.into_iter().map(|(day, sessions)| {
                let day_total: i64 = sessions.iter().map(|session| session.secs).sum();
                let day_max = sessions
                    .iter()
                    .map(|session| session.secs)
                    .max()
                    .unwrap_or(1)
                    .max(1);

                rect()
                    .vertical()
                    .width(Size::fill())
                    .spacing(8.)
                    .child(
                        rect()
                            .horizontal()
                            .width(Size::fill())
                            .cross_align(Alignment::Center)
                            .content(Content::Flex)
                            .child(
                                label()
                                    .text(day_label(&day, today))
                                    .font_size(12.)
                                    .font_weight(FontWeight::MEDIUM)
                                    .width(Size::flex(1.0))
                                    .color(colors::fg_primary()),
                            )
                            .child(
                                label()
                                    .text(format_duration(day_total))
                                    .font_size(12.)
                                    .color(colors::fg_secondary()),
                            ),
                    )
                    .children(
                        sessions
                            .into_iter()
                            .map(|session| session_row(session, day_max)),
                    )
                    .into_element()
            }))
            .into_element();

        super::chart_card(
            "Recent sessions",
            if days > 0 {
                format!("The last {days} days of your last {SESSION_LIMIT} finished sessions")
            } else {
                format!("Your last {SESSION_LIMIT} finished sessions, newest first")
            },
            Some(picker),
            chart,
        )
    }
}

/// Whether a day belongs to the range the picker shows
fn within_range(day: &str, days: i64, today: NaiveDate) -> bool {
    if days <= 0 {
        return true;
    }

    let Ok(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
        return true;
    };

    (today - date).num_days() < days
}

fn session_row(session: &PlaySession, day_max: i64) -> Element {
    let width = ((session.secs as f64 / day_max as f64) * 100.0).clamp(3.0, 100.0) as f32;

    rect()
        .vertical()
        .width(Size::fill())
        .spacing(4.)
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .cross_align(Alignment::Center)
                .content(Content::Flex)
                .spacing(10.)
                .child(
                    label()
                        .text(session.cluster_name.clone())
                        .font_size(13.)
                        .max_lines(1)
                        .width(Size::flex(1.0))
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text(format!(
                            "{} \u{2013} {}",
                            clock(&session.started_at),
                            clock(&session.ended_at)
                        ))
                        .font_size(12.)
                        .color(colors::fg_secondary()),
                )
                .child(
                    label()
                        .text(format_duration(session.secs))
                        .font_size(12.)
                        .width(Size::px(64.))
                        .color(colors::fg_secondary()),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::px(4.))
                .corner_radius(CornerRadius::new_all(2.))
                .background(colors::component_bg())
                .child(
                    rect()
                        .width(Size::percent(width))
                        .height(Size::fill())
                        .corner_radius(CornerRadius::new_all(2.))
                        .background(colors::brand()),
                ),
        )
        .into_element()
}

fn clock(iso: &str) -> String {
    DateTime::parse_from_rfc3339(iso)
        .map(|at| at.format("%H:%M").to_string())
        .unwrap_or_default()
}

pub(super) fn day_label(day: &str, today: NaiveDate) -> String {
    let Ok(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
        return day.to_string();
    };

    if date == today {
        "Today".to_string()
    } else if Some(date) == today.pred_opt() {
        "Yesterday".to_string()
    } else {
        date.format("%a, %b %-d").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{day_label, within_range};

    #[test]
    fn day_labels_speak_in_today_and_yesterday() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();
        assert_eq!(day_label("2026-10-18", today), "Today");
        assert_eq!(day_label("2026-10-17", today), "Yesterday");
        assert_eq!(day_label("2026-10-11", today), "Sun, Oct 11");
        assert_eq!(day_label("not-a-date", today), "not-a-date");
    }

    #[test]
    fn a_range_keeps_the_days_it_covers() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();

        assert!(
            within_range("2026-10-18", 7, today),
            "today is in every range"
        );
        assert!(
            within_range("2026-10-12", 7, today),
            "six days back is in a week"
        );
        assert!(!within_range("2026-10-11", 7, today), "a week back is not");
        assert!(
            within_range("2026-10-11", 30, today),
            "but it is in a month"
        );
        assert!(
            within_range("2026-01-01", 0, today),
            "every range keeps all of it"
        );
    }

    #[test]
    fn a_day_the_picker_cannot_read_stays_in_the_range() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();

        assert!(
            within_range("not-a-date", 7, today),
            "a day that cannot be read is not a day outside the range"
        );
    }
}
