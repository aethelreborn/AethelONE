//! A year-in-review style calendar no launcher shows the GitHub-style grid
//! makes streaks and dry spells visible across months in a way the line
//! chart cannot, and hover names the exact day

use std::collections::HashMap;

use chrono::{Datelike, Duration, Local, NaiveDate};
use freya::prelude::*;

use oneclient_core::game::{DayPlaytime, WEEKDAY_LABELS};

use crate::theme::colors;
use crate::utils::{format_day, format_duration, parse_day};

use super::chart_card;

const WEEKS: i64 = 26;
const CELL: f32 = 14.;
const GAP: f32 = 4.;

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(PartialEq)]
pub(super) struct PlaytimeHeatmap {
    daily: Vec<DayPlaytime>,
}

impl PlaytimeHeatmap {
    pub(super) fn new(daily: Vec<DayPlaytime>) -> Self {
        Self { daily }
    }
}

impl Component for PlaytimeHeatmap {
    fn render(&self) -> impl IntoElement {
        let mut readout = use_state(|| "Hover a day for details".to_string());

        let today = Local::now().date_naive();
        let start = monday_on_or_before(today - Duration::days(WEEKS * 7 - 1));

        let by_day: HashMap<NaiveDate, i64> = self
            .daily
            .iter()
            .filter_map(|day| parse_day(&day.date).map(|date| (date, day.secs)))
            .collect();

        let mut nonzero: Vec<i64> = by_day
            .iter()
            .filter(|(date, secs)| **date >= start && **date <= today && **secs > 0)
            .map(|(_, secs)| *secs)
            .collect();
        nonzero.sort_unstable();
        let thresholds = quartiles(&nonzero);

        let active = nonzero.len();
        let span_days = (today - start).num_days() + 1;
        let subtitle = if active == 0 {
            format!("No playtime in the last {} weeks", WEEKS)
        } else {
            let best = nonzero.last().copied().unwrap_or(0);
            format!(
                "Active on {active} of {span_days} days \u{b7} best day {}",
                format_duration(best)
            )
        };

        let mut columns: Vec<Vec<(NaiveDate, Option<i64>)>> = Vec::new();
        let mut cursor = start;
        while cursor <= today {
            let mut column = Vec::with_capacity(7);
            for offset in 0..7 {
                let date = cursor + Duration::days(offset);
                let secs = (date <= today).then(|| *by_day.get(&date).unwrap_or(&0));
                column.push((date, secs));
            }
            columns.push(column);
            cursor += Duration::days(7);
        }

        let month_row = rect()
            .horizontal()
            .spacing(GAP)
            .padding(Gaps::new(0., 0., 4., 30.))
            .children(columns.iter().enumerate().map(|(index, column)| {
                let month = column[0].0.month0() as usize;
                let month_text = if index == 0 || columns[index - 1][0].0.month0() as usize != month
                {
                    MONTH_ABBR[month].to_string()
                } else {
                    String::new()
                };
                rect()
                    .width(Size::px(CELL))
                    .child(
                        label()
                            .text(month_text)
                            .font_size(9.)
                            .max_lines(1)
                            .color(colors::fg_secondary()),
                    )
                    .into_element()
            }));

        let weekday_col = rect().vertical().spacing(GAP).children((0..7).map(|row| {
            let weekday = match row {
                0 => WEEKDAY_LABELS[0],
                2 => WEEKDAY_LABELS[2],
                4 => WEEKDAY_LABELS[4],
                _ => "",
            };
            rect()
                .horizontal()
                .width(Size::px(26.))
                .height(Size::px(CELL))
                .main_align(Alignment::End)
                .child(
                    label()
                        .text(weekday.to_string())
                        .font_size(9.)
                        .max_lines(1)
                        .color(colors::fg_secondary()),
                )
                .into_element()
        }));

        let grid = rect()
            .horizontal()
            .spacing(GAP)
            .children(columns.iter().map(|column| {
                rect()
                    .vertical()
                    .spacing(GAP)
                    .children(column.iter().map(|(date, secs)| {
                        let Some(secs) = secs else {
                            return rect()
                                .width(Size::px(CELL))
                                .height(Size::px(CELL))
                                .corner_radius(CornerRadius::new_all(3.))
                                .background(Color::TRANSPARENT)
                                .into_element();
                        };
                        let entry = if *secs > 0 {
                            format!("{} \u{b7} {}", format_day(*date), format_duration(*secs))
                        } else {
                            format!("{} \u{b7} no playtime", format_day(*date))
                        };
                        rect()
                            .width(Size::px(CELL))
                            .height(Size::px(CELL))
                            .corner_radius(CornerRadius::new_all(3.))
                            .background(level_color(*secs, &thresholds))
                            .on_pointer_enter(move |_| {
                                if *readout.peek() != entry {
                                    readout.set(entry.clone());
                                }
                            })
                            .into_element()
                    }))
                    .into_element()
            }));

        let legend = rect()
            .horizontal()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(4.)
            .child(
                label()
                    .text("Less")
                    .font_size(9.)
                    .color(colors::fg_secondary()),
            )
            .children((0..5).map(|step| {
                rect()
                    .width(Size::px(10.))
                    .height(Size::px(10.))
                    .corner_radius(CornerRadius::new_all(2.))
                    .background(legend_color(step))
                    .into_element()
            }))
            .child(
                label()
                    .text("More")
                    .font_size(9.)
                    .color(colors::fg_secondary()),
            )
            .child(rect().width(Size::flex(1.0)))
            .child(
                label()
                    .text(readout.read().clone())
                    .font_size(10.)
                    .color(colors::fg_secondary()),
            );

        let body = rect()
            .vertical()
            .width(Size::fill())
            .spacing(8.)
            .child(month_row)
            .child(
                rect()
                    .horizontal()
                    .spacing(8.)
                    .child(weekday_col)
                    .child(grid),
            )
            .child(legend);

        chart_card("Playtime calendar", subtitle, None, body.into_element())
    }
}

fn monday_on_or_before(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

fn quartiles(nonzero: &[i64]) -> [i64; 3] {
    if nonzero.is_empty() {
        return [1, 1, 1];
    }
    let at = |share: f64| {
        let index = ((nonzero.len() as f64 - 1.0) * share).round() as usize;
        nonzero[index]
    };
    let first = at(0.25).max(1);
    let second = at(0.5).max(first);
    let third = at(0.75).max(second);
    [first, second, third]
}

fn level_color(secs: i64, thresholds: &[i64; 3]) -> Color {
    if secs <= 0 {
        return colors::component_bg();
    }
    if secs <= thresholds[0] {
        colors::brand().with_a(70)
    } else if secs <= thresholds[1] {
        colors::brand().with_a(120)
    } else if secs <= thresholds[2] {
        colors::brand().with_a(180)
    } else {
        colors::brand().with_a(240)
    }
}

fn legend_color(step: i64) -> Color {
    match step {
        0 => colors::component_bg(),
        1 => colors::brand().with_a(70),
        2 => colors::brand().with_a(120),
        3 => colors::brand().with_a(180),
        _ => colors::brand().with_a(240),
    }
}
