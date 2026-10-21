use freya::prelude::*;
use oneclient_common::supports_quick_play;
use oneclient_core::clusters::Cluster;
use oneclient_core::game::ServerStat;
use oneclient_core::servers::{SavedServerRow, matches_saved};

use crate::components::{Button, Dropdown, Icon, IconType, ScrollArea, TextInput};
use crate::hooks::{
    add_saved_server, invalidate_saved_servers, mark_server_joined, query_is_loading,
    remove_saved_server, set_saved_server_label, settled_or_loading, try_global_analytics,
    try_saved_servers, try_server_ping, use_active_cluster_id, use_clusters, use_dispatch,
    use_global_analytics, use_saved_servers, use_server_ping,
};
use crate::theme::colors;
use crate::ui::{border_all_color, centered_note};
use crate::utils::{default_cluster, format_duration, format_time_ago};

#[derive(PartialEq)]
pub struct Servers;

impl Component for Servers {
    fn render(&self) -> impl IntoElement {
        let query = use_saved_servers();
        let servers = try_saved_servers(&query);
        let loading = query_is_loading(&query);
        let clusters = use_clusters();
        let cluster_list: Vec<Cluster> = settled_or_loading(&clusters).unwrap_or_default();
        let analytics = try_global_analytics(&use_global_analytics());
        let stats: Vec<ServerStat> = analytics
            .map(|analytics| analytics.servers)
            .unwrap_or_default();
        let dispatch = use_dispatch();

        let mut address = use_state(String::new);
        let mut adding = use_state(|| false);
        let add_dispatch = dispatch.clone();

        let add = move || {
            let value = address.read().trim().to_string();
            if value.is_empty() || *adding.read() {
                return;
            }
            adding.set(true);
            let dispatch = add_dispatch.clone();
            spawn(async move {
                match add_saved_server(&value, None).await {
                    Ok(()) => {
                        address.set(String::new());
                    }
                    Err(err) => {
                        dispatch
                            .notify("Couldn't add server")
                            .body(err.to_string())
                            .error()
                            .send();
                    }
                }
                adding.set(false);
            });
        };

        let body: Element = match servers {
            None if loading => centered_note("Loading servers…"),
            Some(servers) if servers.is_empty() => centered_note(
                "No servers saved yet. Add an address above to watch it and join in one click.",
            ),
            Some(servers) => rect()
                .vertical()
                .width(Size::fill())
                .spacing(8.)
                .children(
                    servers
                        .iter()
                        .map(|server| {
                            let stat = stats
                                .iter()
                                .find(|stat| {
                                    matches_saved(&server.address, &stat.address, stat.port)
                                })
                                .cloned();
                            ServerRow {
                                server: server.clone(),
                                clusters: cluster_list.clone(),
                                stat,
                            }
                            .into_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_element(),
            None => centered_note("Couldn't read the saved servers."),
        };

        rect()
            .vertical()
            .width(Size::fill())
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .child(
                ScrollArea::new()
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(
                        rect()
                            .vertical()
                            .width(Size::fill())
                            .padding(Gaps::new(0., 40., 40., 40.))
                            .spacing(24.)
                            .child(header(add, address, dispatch))
                            .child(body),
                    ),
            )
            .into_element()
    }
}

fn header(
    add: impl FnMut() + Clone + 'static,
    address: State<String>,
    dispatch: crate::Actions,
) -> Element {
    let mut add_press = add.clone();
    let mut add_submit = add;

    rect()
        .vertical()
        .spacing(16.)
        .child(
            rect()
                .vertical()
                .spacing(2.)
                .child(
                    label()
                        .text("Servers")
                        .font_size(32.)
                        .font_weight(FontWeight::BOLD)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text("Saved servers with live status. Nothing here is shared with anyone.")
                        .font_size(13.)
                        .color(colors::fg_secondary()),
                ),
        )
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .cross_align(Alignment::Center)
                .spacing(8.)
                .child(
                    TextInput::new(address)
                        .placeholder("play.example.com or play.example.com:25565")
                        .width(Size::px(360.))
                        .on_submit(move |_| add_submit()),
                )
                .child(
                    Button::new()
                        .primary()
                        .child(Icon::new(IconType::Plus).size(14.))
                        .text("Add server")
                        .on_press(move |_| add_press()),
                )
                .child(rect().width(Size::flex(1.0)))
                .child(
                    Button::new()
                        .secondary()
                        .tooltip("Check every server again")
                        .on_press(move |_| {
                            let dispatch = dispatch.clone();
                            spawn(async move {
                                invalidate_saved_servers().await;
                                dispatch
                                    .notify("Checking servers")
                                    .body("Statuses refresh in a moment.")
                                    .info()
                                    .toast_only()
                                    .send();
                            });
                        })
                        .child(Icon::new(IconType::RefreshCw01).size(14.)),
                ),
        )
        .into_element()
}

#[derive(PartialEq)]
struct ServerRow {
    server: SavedServerRow,
    clusters: Vec<Cluster>,
    stat: Option<ServerStat>,
}

/// Writes the name a server was given, a blank name takes the name away
fn save_server_name(
    draft: State<String>,
    mut editing: State<bool>,
    server_id: i64,
    dispatch: crate::Actions,
) -> impl FnMut() {
    move || {
        let value = draft.read().trim().to_string();
        editing.set(false);
        let dispatch = dispatch.clone();
        spawn(async move {
            let label = if value.is_empty() {
                None
            } else {
                Some(value.as_str())
            };
            if let Err(err) = set_saved_server_label(server_id, label).await {
                dispatch
                    .notify("Couldn't rename server")
                    .body(err.to_string())
                    .error()
                    .send();
            }
        });
    }
}

impl Component for ServerRow {
    fn render(&self) -> impl IntoElement {
        let ping = use_server_ping(self.server.address.clone());
        let status = try_server_ping(&ping);
        let checking = status.is_none();
        let dispatch = use_dispatch();

        let default_index =
            self.stat
                .as_ref()
                .and_then(|stat| {
                    self.clusters
                        .iter()
                        .position(|candidate| candidate.id == stat.last_cluster_id)
                })
                .or_else(|| {
                    default_cluster(self.clusters.clone(), *use_active_cluster_id().read())
                        .and_then(|cluster| {
                            self.clusters
                                .iter()
                                .position(|candidate| candidate.id == cluster.id)
                        })
                })
                .unwrap_or(0);
        let selected = use_state(move || default_index);

        let dot = match &status {
            None => colors::fg_secondary().with_a(120),
            Some(Ok(_)) => colors::success(),
            Some(Err(_)) => colors::danger(),
        };

        let (detail, motd) = match &status {
            None => ("Checking…".to_string(), String::new()),
            Some(Ok(s)) => (
                format!(
                    "{} · {}/{} players · {} ms",
                    s.version_name, s.players_online, s.players_max, s.latency_ms
                ),
                s.motd.clone(),
            ),
            Some(Err(err)) => (format!("Offline — {err}"), String::new()),
        };

        let name = self
            .server
            .label
            .clone()
            .unwrap_or_else(|| self.server.address.clone());
        let show_address = self.server.label.is_some();

        // The name a server is given is edited in place
        let server_id = self.server.id;
        let current_label = self.server.label.clone().unwrap_or_default();
        let mut editing = use_state(|| false);
        let mut draft = use_state(String::new);
        let mut save = save_server_name(draft, editing, server_id, dispatch.clone());
        let mut save_submit = save_server_name(draft, editing, server_id, dispatch.clone());

        let history = self.stat.as_ref().map(|stat| {
            let mut parts = vec![if stat.joins == 1 {
                "1 visit".to_string()
            } else {
                format!("{} visits", stat.joins)
            }];
            if stat.total_secs > 0 {
                parts.push(format_duration(stat.total_secs));
            }
            if let Some(ago) = stat.last_played.as_deref().and_then(format_time_ago) {
                parts.push(format!("last played {ago}"));
            }
            parts.join(" · ")
        });

        let cluster_names: Vec<String> = self
            .clusters
            .iter()
            .map(|cluster| format!("{} · {}", cluster.name, cluster.mc_version))
            .collect();
        let chosen = *selected.read();
        let selected_label = cluster_names.get(chosen).cloned().unwrap_or_default();
        let has_clusters = !self.clusters.is_empty();

        let join_server = self.server.clone();
        let join_clusters = self.clusters.clone();
        let mut selected = selected;
        let join_dispatch = dispatch.clone();
        let remove_dispatch = dispatch.clone();
        let remove_id = self.server.id;
        let remove_address = self.server.address.clone();

        rect()
            .horizontal()
            .width(Size::fill())
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .spacing(12.)
            .padding(Gaps::new_symmetric(14., 16.))
            .corner_radius(CornerRadius::new_all(12.))
            .background(colors::page_elevated())
            .border(border_all_color(1., colors::component_border()))
            .child(
                rect()
                    .width(Size::px(10.))
                    .height(Size::px(10.))
                    .corner_radius(CornerRadius::new_all(5.))
                    .background(dot),
            )
            .child(
                rect()
                    .vertical()
                    .width(Size::flex(1.0))
                    .spacing(2.)
                    .child(if *editing.read() {
                        TextInput::new(draft)
                            .placeholder("Name this server")
                            .auto_focus(true)
                            .on_submit(move |_| save_submit())
                            .width(Size::fill())
                            .into_element()
                    } else {
                        label()
                            .text(name)
                            .font_size(14.)
                            .font_weight(FontWeight::MEDIUM)
                            .color(colors::fg_primary())
                            .into_element()
                    })
                    .maybe_child(show_address.then(|| {
                        label()
                            .text(self.server.address.clone())
                            .font_size(11.)
                            .color(colors::fg_secondary())
                            .into_element()
                    }))
                    .maybe_child((!motd.is_empty()).then(|| {
                        label()
                            .text(motd.clone())
                            .font_size(12.)
                            .max_lines(1)
                            .color(colors::fg_secondary())
                            .into_element()
                    }))
                    .child(label().text(detail).font_size(11.).color(
                        if matches!(status, Some(Err(_))) {
                            colors::danger()
                        } else {
                            colors::fg_secondary()
                        },
                    ))
                    .maybe_child(history.as_ref().map(|text| {
                        label()
                            .text(text.clone())
                            .font_size(11.)
                            .color(colors::fg_secondary().with_a(170))
                            .into_element()
                    })),
            )
            .maybe_child(has_clusters.then(|| {
                Dropdown::new(selected_label, cluster_names)
                    .width(Size::px(220.))
                    .height(Size::px(34.))
                    .on_select(move |idx: usize| {
                        selected.set(idx);
                    })
                    .into_element()
            }))
            .child(
                Button::new()
                    .primary()
                    .enabled(has_clusters && !checking)
                    .tooltip("Launch the game straight onto this server")
                    .on_press(move |_| {
                        let Some(cluster) = join_clusters.get(*selected.read()) else {
                            return;
                        };
                        if !supports_quick_play(&cluster.mc_version) {
                            join_dispatch
                                .notify("Quick play needs Minecraft 1.20")
                                .body(format!(
                                    "{} predates joining straight into a server.",
                                    cluster.name
                                ))
                                .error()
                                .send();
                            return;
                        }
                        join_dispatch.launch_server(cluster.id, join_server.address.clone());
                        let joined_id = join_server.id;
                        spawn(async move {
                            let _ = mark_server_joined(joined_id).await;
                        });
                    })
                    .child(Icon::new(IconType::Play).size(14.))
                    .text("Join"),
            )
            .child(
                Button::new()
                    .ghost()
                    .icon()
                    .tooltip(if *editing.read() {
                        "Save the name"
                    } else {
                        "Name this server"
                    })
                    .on_press(move |_| {
                        if *editing.read() {
                            save();
                            return;
                        }
                        draft.set(current_label.clone());
                        editing.set(true);
                    })
                    .child(
                        Icon::new(if *editing.read() {
                            IconType::Check
                        } else {
                            IconType::Pencil01
                        })
                        .size(14.),
                    ),
            )
            .child(
                Button::new()
                    .ghost()
                    .icon()
                    .tooltip("Forget this server")
                    .on_press(move |_| {
                        let dispatch = remove_dispatch.clone();
                        let address = remove_address.clone();
                        spawn(async move {
                            if let Err(err) = remove_saved_server(remove_id).await {
                                dispatch
                                    .notify("Couldn't remove server")
                                    .body(err.to_string())
                                    .error()
                                    .send();
                            } else {
                                dispatch
                                    .notify("Server removed")
                                    .body(address)
                                    .info()
                                    .toast_only()
                                    .send();
                            }
                        });
                    })
                    .child(Icon::new(IconType::Trash01).size(14.)),
            )
            .into_element()
    }
}
