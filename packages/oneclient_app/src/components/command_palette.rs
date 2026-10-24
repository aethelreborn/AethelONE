use freya::prelude::*;
use freya::router::RouterContext;
use oneclient_common::search::SearchQuery;
use oneclient_core::clusters::Cluster;

use crate::Route;
use crate::components::{Icon, IconType, OverlayPopup, TextInput, overlay_is_topmost};
use crate::hooks::{settled_or_loading, use_clusters};
use crate::theme::colors;
use crate::ui::border_all_color;

const PALETTE_WIDTH_PX: f32 = 560.;
const LIST_MAX_HEIGHT_PX: f32 = 380.;
const ROW_HEIGHT_PX: f32 = 40.;
const ROW_GAP_PX: f32 = 2.;
const CARD_BG: Color = Color::from_rgb(21, 28, 34);

#[derive(Clone)]
struct Command {
    title: String,
    subtitle: String,
    icon: IconType,
    route: Route,
}

impl Command {
    fn new(
        title: impl Into<String>,
        subtitle: impl Into<String>,
        icon: IconType,
        route: Route,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            icon,
            route,
        }
    }
}

fn static_commands() -> Vec<Command> {
    vec![
        Command::new("Home", "launcher home", IconType::LayoutTop, Route::Home {}),
        Command::new(
            "Versions",
            "instances and versions",
            IconType::DotsGrid,
            Route::Clusters {},
        ),
        Command::new(
            "Skins",
            "account skins",
            IconType::PaintPour,
            Route::AccountSkins {},
        ),
        Command::new(
            "Cosmetics",
            "wardrobe and equipped items",
            IconType::PaintPour,
            Route::Cosmetics {},
        ),
        Command::new(
            "Statistics",
            "playtime stats",
            IconType::BarChartSquare02,
            Route::Stats {},
        ),
        Command::new(
            "Servers",
            "saved servers",
            IconType::Globe01,
            Route::Servers {},
        ),
        Command::new(
            "Debug",
            "developer options",
            IconType::Terminal,
            Route::Debug {},
        ),
        Command::new(
            "Settings: Appearance",
            "theme and accent",
            IconType::Colors,
            Route::SettingsAppearance {},
        ),
        Command::new(
            "Settings: Minecraft",
            "memory, resolution, JVM",
            IconType::Settings01,
            Route::SettingsMinecraft {},
        ),
        Command::new(
            "Settings: Accounts",
            "microsoft and offline accounts",
            IconType::Users01,
            Route::SettingsAccounts {},
        ),
        Command::new(
            "Settings: Launcher",
            "startup, tray, updates",
            IconType::Settings02,
            Route::SettingsLauncher {},
        ),
        Command::new(
            "Settings: Java",
            "runtimes and providers",
            IconType::Key01,
            Route::SettingsJava {},
        ),
        Command::new(
            "Settings: Storage",
            "data folder and leftovers",
            IconType::Database01,
            Route::SettingsStorage {},
        ),
        Command::new(
            "Settings: APIs",
            "curseforge and modrinth keys",
            IconType::Globe01,
            Route::SettingsApis {},
        ),
        Command::new(
            "Settings: Language",
            "interface language",
            IconType::Globe01,
            Route::SettingsLanguage {},
        ),
        Command::new(
            "Settings: Changelog",
            "what changed recently",
            IconType::Announcement01,
            Route::SettingsChangelog {},
        ),
        Command::new(
            "Settings: Developer",
            "experimental toggles",
            IconType::Terminal,
            Route::SettingsDeveloper {},
        ),
    ]
}

fn cluster_commands(clusters: &[Cluster]) -> Vec<Command> {
    let mut commands = Vec::with_capacity(clusters.len() * 5);
    for cluster in clusters {
        let loader = cluster.mc_loader.get_format_name();
        let id = cluster.id;
        commands.push(Command::new(
            cluster.name.clone(),
            format!("{loader} {}", cluster.mc_version),
            IconType::Play,
            Route::ClusterOverview { cluster_id: id },
        ));
        commands.push(Command::new(
            format!("{}: Mods", cluster.name),
            format!("{loader} {}", cluster.mc_version),
            IconType::FilePlus02,
            Route::ClusterMods { cluster_id: id },
        ));
        commands.push(Command::new(
            format!("{}: Worlds", cluster.name),
            format!("{loader} {}", cluster.mc_version),
            IconType::Folder,
            Route::ClusterWorlds { cluster_id: id },
        ));
        commands.push(Command::new(
            format!("{}: Configs", cluster.name),
            format!("{loader} {}", cluster.mc_version),
            IconType::File02,
            Route::ClusterConfigs { cluster_id: id },
        ));
        commands.push(Command::new(
            format!("{}: Logs", cluster.name),
            format!("{loader} {}", cluster.mc_version),
            IconType::Terminal,
            Route::ClusterLogs { cluster_id: id },
        ));
        commands.push(Command::new(
            format!("{}: Settings", cluster.name),
            format!("{loader} {}", cluster.mc_version),
            IconType::Settings01,
            Route::ClusterSettings { cluster_id: id },
        ));
    }
    commands
}

/// Ctrl+K on Windows/Linux, Cmd+K on macOS
pub fn is_palette_toggle(e: &Event<KeyboardEventData>) -> bool {
    matches!(&e.key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("k"))
        && e.modifiers.contains(Modifiers::ctrl_or_meta())
}

fn run_filter(commands: &[Command], query: &str) -> Vec<Command> {
    let search = SearchQuery::new(query);
    let mut scored: Vec<(oneclient_common::search::MatchScore, Command)> = commands
        .iter()
        .filter_map(|command| {
            search
                .best_score([command.title.as_str(), command.subtitle.as_str()])
                .map(|score| (score, command.clone()))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0));
    scored.into_iter().map(|(_, command)| command).collect()
}

fn run_route(
    route: Route,
    mut open: State<bool>,
    mut query: State<String>,
    mut selected: State<usize>,
) {
    open.set(false);
    query.set(String::new());
    selected.set(0);
    let _ = RouterContext::get().push(route);
}

/// Modal jump-to anywhere in the app: type a few letters of an instance name
/// or a settings page and press Enter. Opened with Ctrl+K / Cmd+K.
#[derive(PartialEq)]
pub struct CommandPalette {
    open: State<bool>,
}

impl CommandPalette {
    #[must_use]
    pub fn new(open: State<bool>) -> Self {
        Self { open }
    }
}

impl Component for CommandPalette {
    fn render(&self) -> impl IntoElement {
        let mut open = self.open;
        let mut query = use_state(String::new);
        let mut selected = use_state(|| 0usize);
        let clusters = settled_or_loading(&use_clusters()).unwrap_or_default();

        if !*open.read() {
            return rect().into_element();
        }

        let mut commands = static_commands();
        commands.extend(cluster_commands(&clusters));
        let rows = run_filter(&commands, &query.read());

        let total = rows.len();
        let highlight = (*selected.read()).min(total.saturating_sub(1));
        let highlighted_route = rows.get(highlight).map(|command| command.route.clone());

        let on_key_down = move |e: Event<KeyboardEventData>| {
            if !overlay_is_topmost(crate::components::OVERLAY_BASE_LEVEL) {
                return;
            }
            match &e.key {
                Key::Named(NamedKey::ArrowDown) => {
                    let next = (*selected.peek() + 1).min(total.saturating_sub(1));
                    selected.set(next);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let next = (*selected.peek()).saturating_sub(1);
                    selected.set(next);
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(route) = highlighted_route.clone() {
                        run_route(route, open, query, selected);
                    }
                }
                _ => {}
            }
        };

        let close = move |_| {
            open.set(false);
            query.set(String::new());
            selected.set(0);
        };

        let list_height = (total as f32 * (ROW_HEIGHT_PX + ROW_GAP_PX)).min(LIST_MAX_HEIGHT_PX);

        let row_elements: Vec<Element> = rows
            .into_iter()
            .enumerate()
            .map(|(position, command)| {
                let highlighted = position == highlight;
                let route = command.route.clone();
                let open_row = open;
                let query_row = query;
                let selected_row = selected;
                command_row(command, highlighted, move |_| {
                    run_route(route.clone(), open_row, query_row, selected_row);
                })
            })
            .collect();

        OverlayPopup::new()
            .on_close(close)
            .child(
                rect()
                    .width(Size::window_percent(100.))
                    .height(Size::window_percent(100.))
                    .center()
                    .child(
                        rect()
                            .vertical()
                            .width(Size::px(PALETTE_WIDTH_PX))
                            .max_width(Size::window_percent(90.))
                            .corner_radius(CornerRadius::new_all(14.))
                            .background(CARD_BG)
                            .border(border_all_color(1., colors::component_border()))
                            .padding(10.)
                            .spacing(8.)
                            .on_global_key_down(on_key_down)
                            .child(
                                rect()
                                    .horizontal()
                                    .width(Size::fill())
                                    .cross_align(Alignment::Center)
                                    .spacing(8.)
                                    .child(
                                        Icon::new(IconType::SearchMd)
                                            .size(16.)
                                            .color(colors::fg_secondary()),
                                    )
                                    .child(
                                        TextInput::new(query)
                                            .auto_focus(true)
                                            .placeholder("Jump to an instance or page…")
                                            .font_size(15.),
                                    ),
                            )
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .height(Size::px(list_height))
                                    .spacing(ROW_GAP_PX)
                                    .children(row_elements),
                            )
                            .maybe(total == 0, |el| {
                                el.child(
                                    label()
                                        .text("No matches")
                                        .font_size(13.)
                                        .color(colors::fg_secondary()),
                                )
                            }),
                    ),
            )
            .into_element()
    }
}

fn command_row(
    command: Command,
    highlighted: bool,
    on_press: impl Into<EventHandler<()>>,
) -> Element {
    let on_press = on_press.into();
    let background = if highlighted {
        colors::ghost_overlay_hover()
    } else {
        Color::TRANSPARENT
    };

    rect()
        .horizontal()
        .width(Size::fill())
        .height(Size::px(ROW_HEIGHT_PX))
        .cross_align(Alignment::Center)
        .padding(Gaps::new_symmetric(0., 10.))
        .corner_radius(CornerRadius::new_all(8.))
        .background(background)
        .on_all_press(move |_| on_press.call(()))
        .child(Icon::new(command.icon).size(15.).color(if highlighted {
            colors::fg_primary()
        } else {
            colors::fg_secondary()
        }))
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .cross_align(Alignment::Center)
                .padding(Gaps::new(0., 0., 0., 10.))
                .child(
                    label()
                        .text(command.title)
                        .font_size(14.)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text(command.subtitle)
                        .font_size(12.)
                        .color(colors::fg_secondary()),
                ),
        )
        .into_element()
}
