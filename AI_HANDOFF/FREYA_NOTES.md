# Freya UI notes (this codebase's framework)

The app uses **Freya** (Rust UI framework). These facts were learned the hard
way; the framework differs from React/Rust-egui muscle memory.

## Components

- `Button::new()` builders: `.primary() .secondary() .ghost() .danger()
  .small() .icon() .enabled(bool) .text(...) .icon(...child Icon) .tooltip(...)
  .width(...) .on_press(FnMut(Event<PressEventData>))`
- `TextInput::new(State<String>)` — `.placeholder() .width() .height()`
  `.on_submit(EventHandler<String>) .leading(Element)`. Non-`Inner` height
  auto-fills the parent (`.multiline(true)` for textareas). No `max_chars`.
- `Dropdown::new(selected_label, options).on_select(|idx: usize|)`; add
  `.outlined() .width() .height()` as needed.
- `ScrollArea::new()` — NO `.vertical()` method; `.scrollbar_gutter(true)`
  available; wrap a single child.
- `OverlayPopup::new().on_close(...)` + centered `rect()` = modal pattern
  (see `view/app/clusters/delete_instance.rs`).
- `Icon::new(IconType::X).size(px).rotate(deg).color(...)`. IconType list
  includes: Plus, Play, Trash01, Pencil01, Folder, FolderCheck,
  FolderDownload, Copy01, Star01, Settings01, SearchMd, RefreshCw01,
  ChevronDown, ChevronRight, Bell01, Announcement01, Rocket02, Globe01,
  Check, ClipboardCheck, Download01, DownloadCloud02, Square, DotsGrid,
  ParagraphWrap. There is NO `Server01` and no save/disk icon.
- `Markdown::new(text).style(MarkdownStyle { … })` — style copied from
  `view/app/settings/changelog.rs` for consistent look.

## State & queries

- `State<T>` is `Copy`; `.read() / .peek() / .set(v) / .write()`. `.set()` on
  a captured copy is fine inside `move` closures (`FnMut` context).
- Render-time state writes are OK **when guarded by `.peek()`** (baseline
  adoption pattern — see clusters page selection logic).
- Freya query: `Query::new(keys, QueryImpl)` where keys are `Hash + Eq +
  Clone + Copy`; `QueryCapability { Ok, Err, Keys; fn matches(&self, &Keys)
  -> bool }` for keyed data (tuple-struct pattern:
  `Query::new(keys.clone(), MyQuery(keys))`).
- `settled_or_loading(&query)`, `query_error(&query)`,
  `query_is_loading(&query)` live in `hooks/queries/state.rs` and are
  re-exported from `hooks`.
- Invalidate: `QueriesStorage::<MyQuery>::invalidate_all().await`.

## Layout

- `rect().vertical()/.horizontal()/.center() .width(Size::fill|px|
  flex(x)|percent(x)|window_percent) .height(...) .spacing(px)
  .padding(Gaps::new(a,t?)/new_all/new_symmetric(y,x)) .corner_radius(
  CornerRadius::new_all(r)) .background(color) .border(border_all_color(1.,
  colors::component_border())) .content(Content::Flex) .cross_align/
  .main_align(Alignment::…) .overflow(Overflow::Clip) .child()/.children()/
  .maybe_child()/.append_children()`
- Color helpers: `colors::page_elevated()`, `component_bg()`,
  `component_border()`, `fg_primary()`, `fg_secondary()`, `brand()`,
  `success()`, `danger()`, `code_info()`; `Color::TRANSPARENT`,
  `.with_a(u8)` for alpha. `border_all_color` lives in `crate::ui`.
- Reusable page pieces: `chart_card(title, subtitle, trailing, chart)` and
  `card_header` (analytics/mod.rs), `section(title, caption, grid)` and
  `group_section(...)` (clusters/page.rs), `dialog(...)` (cluster module),
  `empty_shell/empty_title/empty_hint` (package_manager), `centered_note`
  (ui), `fixed_grid(cards, columns, item_height, gap)`,
  `grid_columns_for_width`, `window_logical_size`, `last_played_label`.

## App-specific utilities (crate::utils)

`format_size`, `format_duration(secs)`, `format_day(NaiveDate)`,
`parse_day`, `format_time_ago(rfc3339) -> Option<String>`, `default_cluster(
clusters, active_id)`, `default_line`, `default_loader`, `default_version_key`,
`line_title`, `resolve_cluster`, `sort_clusters_for_home`, `split_clusters`,
`GridSelection`, `ReleaseLine`.

## Actions (crate::hooks::Actions)

`.launch_cluster(id)`, `.launch_server(cluster_id, address)` (Quick Play
Multiplayer; guard with `supports_quick_play(&cluster.mc_version)` ≥ 1.20),
`.launch_world(cluster_id, world)`, `.kill_cluster(id)`,
`.notify(title).body(text).info()/.error()/.toast_only().send()`,
`.toggle_favorite_cluster(id)`, `.set_seen_changelog_version(v)`.
`Actions` is Clone-not-Copy. `use_dispatch()` to obtain it.
