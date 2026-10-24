use std::collections::BTreeMap;

use freya::prelude::*;
use serde::Deserialize;
use serde_json::Value;

use crate::components::{
    Button, Icon, IconType, PlayerModel, ScrollArea, TextInput, remote_icon,
};
use crate::hooks::{try_default_account, use_cached_image, use_current_account};
use crate::theme::colors;
use crate::ui::{
    border_all_color, centered_note, flow_grid, grid_columns_for_width, window_logical_size,
};

const KINDS: [&str; 8] = [
    "cape",
    "wings",
    "hat",
    "backpack",
    "glove",
    "aura",
    "boots",
    "shoulder",
];

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    cosmetics: Vec<RawGroup>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RawGroup {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    allowed_slots: Vec<String>,
    #[serde(default)]
    variants: Vec<RawVariant>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct RawVariant {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    allowed_slots: Vec<String>,
    #[serde(default)]
    cover_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct WardrobeItem {
    group_name: String,
    kind: String,
    variant_id: i64,
    name: String,
    slots: Vec<String>,
    cover_url: Option<String>,
}

#[derive(PartialEq)]
struct CoverImage {
    url: Option<String>,
    size: f32,
}

impl Component for CoverImage {
    fn render(&self) -> impl IntoElement {
        let query = use_cached_image(self.url.clone(), 256);
        remote_icon(self.url.as_deref(), &query, self.size)
    }
}

#[derive(PartialEq)]
pub struct Cosmetics;

impl Component for Cosmetics {
    fn render(&self) -> impl IntoElement {
        let account = try_default_account(&use_current_account());
        let account_key = account.as_ref().map(|account| account.id.to_string());
        let account_name = account.as_ref().map(|account| account.username.clone());
        let account_uuid = account.as_ref().map(|account| account.id.to_string());
        let has_account = account_key.is_some();

        let mut catalog = use_state(|| None::<Result<Vec<WardrobeItem>, String>>);
        let mut equipped = use_state(BTreeMap::<String, Value>::new);
        let selected_kind = use_state(|| "all".to_string());
        let search = use_state(String::new);
        let reload = use_state(|| 0u64);
        let grid_width = use_state(|| 0.0f32);

        let reload_value = *reload.read();
        let fetch_key = (reload_value, account_key.clone());
        let fetch_account = account_key.clone();
        use_side_effect_with_deps(&fetch_key, move |_| {
            let account_key = fetch_account.clone();
            spawn(async move {
                let result = crate::cosmetics_proxy::catalog_snapshot()
                    .await
                    .and_then(parse_catalog);
                catalog.set(Some(result));
                if let Some(account_key) = account_key {
                    equipped.set(crate::cosmetics_proxy::equipped_for(&account_key));
                }
            });
        });

        let catalog_value = catalog.read().clone();
        let items = catalog_value
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|items| items.as_slice());
        let catalog_empty = items.map(|items| items.is_empty()).unwrap_or(false);

        let selected = selected_kind.read().clone();
        let search_value = search.read().to_lowercase();
        let filtered: Vec<WardrobeItem> = items
            .map(|items| {
                items
                    .iter()
                    .filter(|item| {
                        (selected == "all" || item.kind == selected)
                            && (search_value.is_empty()
                                || item.name.to_lowercase().contains(&search_value))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let equipped_now = equipped.read().clone();
        let account_for_toggle = account_key.clone();
        let cards: Vec<Element> = filtered
            .iter()
            .map(|item| {
                let is_equipped =
                    equipped_matches(&equipped_now, &item.slots, item.variant_id);
                let is_partial =
                    !is_equipped && partially_equipped(&equipped_now, &item.slots, item.variant_id);
                let badge = if is_equipped {
                    Some(
                        label()
                            .text("Equipped")
                            .font_size(9.)
                            .color(colors::success())
                            .into_element(),
                    )
                } else if is_partial {
                    Some(
                        label()
                            .text("Partial")
                            .font_size(9.)
                            .color(colors::fg_secondary())
                            .into_element(),
                    )
                } else {
                    None
                };
                let key = account_for_toggle.clone().unwrap_or_default();
                let toggle_item = item.clone();
                let equipped_state = equipped;
                rect()
                    .vertical()
                    .width(Size::flex(1.0))
                    .height(Size::px(196.))
                    .corner_radius(CornerRadius::new_all(12.))
                    .background(colors::page_elevated())
                    .border(border_all_color(
                        if is_equipped { 2. } else { 1. },
                        if is_equipped {
                            colors::brand()
                        } else {
                            colors::component_border()
                        },
                    ))
                    .padding(Gaps::new(10., 10., 10., 10.))
                    .spacing(6.)
                    .on_press(move |_| toggle(&key, &toggle_item, equipped_state))
                    .child(
                        rect()
                            .center()
                            .width(Size::fill())
                            .height(Size::flex(1.0))
                            .corner_radius(CornerRadius::new_all(8.))
                            .background(colors::component_bg())
                            .child(
                                CoverImage {
                                    url: item.cover_url.clone(),
                                    size: 96.,
                                }
                                .into_element(),
                            ),
                    )
                    .child(
                        label()
                            .text(item.name.clone())
                            .font_size(12.)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .max_lines(1)
                            .color(colors::fg_primary()),
                    )
                    .child(
                        rect()
                            .horizontal()
                            .width(Size::fill())
                            .child(
                                label()
                                    .text(kind_label(&item.kind))
                                    .font_size(10.)
                                    .color(colors::fg_secondary()),
                            )
                            .child(rect().width(Size::flex(1.0)))
                            .maybe_child(badge),
                    )
                    .into_element()
            })
            .collect();

        let window_width = window_logical_size().width;
        let available = (window_width - 80. - 170. - 300. - 40.).max(320.);
        let columns = grid_columns_for_width(available, 210., 16.);

        let mut categories: Vec<Element> = Vec::new();
        let count_for = |kind: &str| {
            items
                .map(|items| {
                    items
                        .iter()
                        .filter(|item| kind == "all" || item.kind == kind)
                        .count()
                })
                .unwrap_or(0)
        };
        for kind in std::iter::once("all").chain(KINDS.iter().copied()) {
            let is_selected = selected == kind;
            let count = count_for(kind);
            let mut kind_state = selected_kind;
            let kind_owned = kind.to_string();
            categories.push(
                rect()
                    .horizontal()
                    .width(Size::fill())
                    .height(Size::px(36.))
                    .padding(Gaps::new_symmetric(0., 10.))
                    .corner_radius(CornerRadius::new_all(8.))
                    .background(if is_selected {
                        colors::component_bg()
                    } else {
                        Color::TRANSPARENT
                    })
                    .on_press(move |_| kind_state.set(kind_owned.clone()))
                    .child(
                        label()
                            .text(kind_label(kind))
                            .font_size(13.)
                            .color(if is_selected {
                                colors::fg_primary()
                            } else {
                                colors::fg_secondary()
                            }),
                    )
                    .child(rect().width(Size::flex(1.0)))
                    .child(
                        label()
                            .text(count.to_string())
                            .font_size(11.)
                            .color(colors::fg_secondary()),
                    )
                    .into_element(),
            );
        }

        let retry = {
            let mut reload = reload;
            move || {
                let next = *reload.peek() + 1;
                reload.set(next);
            }
        };
        let body: Element = match &catalog_value {
            None => centered_note("Loading wardrobe…"),
            Some(Err(err)) => rect()
                .vertical()
                .center()
                .width(Size::fill())
                .height(Size::fill())
                .spacing(10.)
                .child(
                    label()
                        .text("Could not load the wardrobe")
                        .font_size(15.)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text(err.clone())
                        .font_size(11.)
                        .color(colors::fg_secondary()),
                )
                .child({
                    let mut retry = retry.clone();
                    Button::new()
                        .secondary()
                        .text("Retry")
                        .on_press(move |_| retry())
                })
                .into_element(),
            Some(Ok(_)) if catalog_empty => rect()
                .vertical()
                .center()
                .width(Size::fill())
                .height(Size::fill())
                .spacing(10.)
                .child(
                    label()
                        .text("No cosmetics available")
                        .font_size(15.)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text("Check your connection and try again.")
                        .font_size(11.)
                        .color(colors::fg_secondary()),
                )
                .child({
                    let mut retry = retry.clone();
                    Button::new()
                        .secondary()
                        .text("Retry")
                        .on_press(move |_| retry())
                })
                .into_element(),
            Some(Ok(_)) if filtered.is_empty() => {
                centered_note("No cosmetics match that filter.")
            }
            Some(Ok(_)) => ScrollArea::new()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(flow_grid(cards, columns, grid_width, 16.))
                .into_element(),
        };

        rect()
            .horizontal()
            .width(Size::fill())
            .height(Size::fill())
            .overflow(Overflow::Clip)
            .padding(40.)
            .spacing(20.)
            .child(
                rect()
                    .vertical()
                    .width(Size::px(170.))
                    .height(Size::fill())
                    .spacing(4.)
                    .child(
                        rect()
                            .padding(Gaps::new(0., 0., 8., 10.))
                            .child(
                                label()
                                    .text("CATEGORIES")
                                    .font_size(10.)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .color(colors::fg_secondary()),
                            ),
                    )
                    .children(categories),
            )
            .child(
                rect()
                    .vertical()
                    .width(Size::flex(1.0))
                    .height(Size::fill())
                    .spacing(14.)
                    .child(
                        rect()
                            .horizontal()
                            .width(Size::fill())
                            .spacing(16.)
                            .child(
                                rect()
                                    .vertical()
                                    .width(Size::flex(1.0))
                                    .child(
                                        label()
                                            .text("Wardrobe")
                                            .font_size(28.)
                                            .font_weight(FontWeight::BOLD)
                                            .color(colors::fg_primary()),
                                    )
                                    .child(
                                        label()
                                            .text("Every Poly+ cosmetic is unlocked")
                                            .font_size(12.)
                                            .color(colors::fg_secondary()),
                                    ),
                            )
                            .child(
                                rect().width(Size::px(260.)).child(
                                    TextInput::new(search)
                                        .placeholder("Search cosmetics")
                                        .compact(),
                                ),
                            ),
                    )
                    .maybe_child((!has_account).then(|| {
                        rect()
                            .width(Size::fill())
                            .padding(Gaps::new(8., 12., 8., 12.))
                            .corner_radius(CornerRadius::new_all(8.))
                            .background(colors::component_bg())
                            .child(
                                label()
                                    .text("Sign in to equip cosmetics.")
                                    .font_size(11.)
                                    .color(colors::fg_secondary()),
                            )
                            .into_element()
                    }))
                    .child(body),
            )
            .child(preview_panel(
                account_name,
                account_uuid,
                account_key,
                items,
                equipped,
            ))
    }
}

fn preview_panel(
    account_name: Option<String>,
    account_uuid: Option<String>,
    account_key: Option<String>,
    items: Option<&[WardrobeItem]>,
    equipped: State<BTreeMap<String, Value>>,
) -> Element {
    let current = equipped.read().clone();
    let has_account = account_key.is_some();

    let mut rows: Vec<Element> = Vec::new();
    if let Some(items) = items {
        for (slot, value) in &current {
            let variant_id = value_id(value);
            let found = variant_id.and_then(|variant_id| {
                items.iter().find(|item| {
                    item.variant_id == variant_id && item.slots.iter().any(|item_slot| item_slot == slot)
                })
            });
            let name = found
                .map(|item| item.name.clone())
                .unwrap_or_else(|| format!("Cosmetic #{}", variant_id.unwrap_or_default()));
            let cover = found.and_then(|item| item.cover_url.clone());
            let slot_name = slot_label(slot).to_string();
            let remove_slot = slot.clone();
            let key = account_key.clone().unwrap_or_default();
            let mut equipped_state = equipped;
            rows.push(
                rect()
                    .horizontal()
                    .width(Size::fill())
                    .height(Size::px(44.))
                    .spacing(8.)
                    .child(CoverImage { url: cover, size: 30. }.into_element())
                    .child(
                        rect()
                            .vertical()
                            .width(Size::flex(1.0))
                            .child(
                                label()
                                    .text(name)
                                    .font_size(12.)
                                    .max_lines(1)
                                    .color(colors::fg_primary()),
                            )
                            .child(
                                label()
                                    .text(slot_name)
                                    .font_size(10.)
                                    .color(colors::fg_secondary()),
                            ),
                    )
                    .child(
                        Button::new()
                            .ghost()
                            .icon()
                            .on_press(move |_| {
                                crate::cosmetics_proxy::unequip_cosmetics(
                                    &key,
                                    std::slice::from_ref(&remove_slot),
                                );
                                let mut next = equipped_state.read().clone();
                                next.remove(&remove_slot);
                                equipped_state.set(next);
                            })
                            .child(Icon::new(IconType::X).size(14.)),
                    )
                    .into_element(),
            );
        }
    }

    let equipped_body: Element = if rows.is_empty() {
        centered_note("Nothing equipped yet")
    } else {
        ScrollArea::new()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .spacing(4.)
            .children(rows)
            .into_element()
    };

    let clear_key = account_key.clone();
    let mut clear_state = equipped;
    let clear_enabled = has_account && !current.is_empty();

    let mut model = PlayerModel::new(account_uuid.unwrap_or_default());
    model = model.width(Size::fill()).height(Size::fill());

    rect()
        .vertical()
        .width(Size::px(300.))
        .height(Size::fill())
        .spacing(12.)
        .child(
            rect()
                .vertical()
                .spacing(2.)
                .child(
                    label()
                        .text("Preview")
                        .font_size(14.)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(colors::fg_primary()),
                )
                .child(
                    label()
                        .text(account_name.unwrap_or_else(|| "No account".to_string()))
                        .font_size(11.)
                        .color(colors::fg_secondary()),
                ),
        )
        .child(
            rect()
                .center()
                .width(Size::fill())
                .height(Size::px(300.))
                .corner_radius(CornerRadius::new_all(16.))
                .background(colors::page_elevated())
                .border(border_all_color(1., colors::component_border()))
                .child(model.into_element()),
        )
        .child(
            rect()
                .horizontal()
                .width(Size::fill())
                .child(
                    label()
                        .text(format!("Equipped ({})", current.len()))
                        .font_size(12.)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .color(colors::fg_primary()),
                )
                .child(rect().width(Size::flex(1.0))),
        )
        .child(equipped_body)
        .child(
            Button::new()
                .danger()
                .small()
                .width(Size::fill())
                .enabled(clear_enabled)
                .text("Clear all")
                .on_press(move |_| {
                    if let Some(key) = clear_key.as_deref() {
                        crate::cosmetics_proxy::clear_cosmetics(key);
                    }
                    clear_state.set(BTreeMap::new());
                }),
        )
        .into()
}

fn parse_catalog(value: Value) -> Result<Vec<WardrobeItem>, String> {
    let response: CatalogResponse = serde_json::from_value(value).map_err(|err| err.to_string())?;
    let mut items = Vec::new();
    for group in response.cosmetics {
        if group.kind == "emote" {
            continue;
        }
        let group_slots = group.allowed_slots.clone();
        let group_variant_count = group.variants.len();
        for variant in group.variants {
            let mut slots = variant.allowed_slots.clone();
            if slots.is_empty() {
                slots = group_slots.clone();
            }
            let name = if group_variant_count > 1
                && !variant.name.is_empty()
                && variant.name != group.name
            {
                format!("{} · {}", group.name, variant.name)
            } else if !group.name.is_empty() {
                group.name.clone()
            } else {
                variant.name.clone()
            };
            items.push(WardrobeItem {
                group_name: group.name.clone(),
                kind: group.kind.clone(),
                variant_id: variant.id,
                name,
                slots,
                cover_url: variant.cover_url,
            });
        }
    }
    items.sort_by(|left, right| {
        kind_rank(&left.kind)
            .cmp(&kind_rank(&right.kind))
            .then(left.group_name.cmp(&right.group_name))
            .then(left.name.cmp(&right.name))
    });
    Ok(items)
}

fn toggle(account: &str, item: &WardrobeItem, mut equipped: State<BTreeMap<String, Value>>) {
    if account.is_empty() || item.slots.is_empty() {
        return;
    }
    let current = equipped.read().clone();
    let matching = item
        .slots
        .iter()
        .filter(|slot| current.get(*slot).and_then(value_id) == Some(item.variant_id))
        .count();
    if matching == item.slots.len() {
        let remove: Vec<String> = item
            .slots
            .iter()
            .filter(|slot| current.get(*slot).and_then(value_id) == Some(item.variant_id))
            .cloned()
            .collect();
        crate::cosmetics_proxy::unequip_cosmetics(account, &remove);
        let mut next = current;
        for slot in remove {
            next.remove(&slot);
        }
        equipped.set(next);
    } else {
        crate::cosmetics_proxy::equip_cosmetic(account, &item.slots, item.variant_id);
        let mut next = current;
        for slot in &item.slots {
            next.insert(slot.clone(), Value::from(item.variant_id));
        }
        equipped.set(next);
    }
}

fn equipped_matches(
    equipped: &BTreeMap<String, Value>,
    slots: &[String],
    variant: i64,
) -> bool {
    !slots.is_empty()
        && slots
            .iter()
            .all(|slot| equipped.get(slot).and_then(value_id) == Some(variant))
}

fn partially_equipped(
    equipped: &BTreeMap<String, Value>,
    slots: &[String],
    variant: i64,
) -> bool {
    slots
        .iter()
        .any(|slot| equipped.get(slot).and_then(value_id) == Some(variant))
}

fn value_id(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn kind_rank(kind: &str) -> usize {
    match kind {
        "cape" => 0,
        "wings" => 1,
        "hat" => 2,
        "backpack" => 3,
        "glove" => 4,
        "aura" => 5,
        "boots" => 6,
        "shoulder" => 7,
        _ => 8,
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "all" => "All",
        "cape" => "Capes",
        "wings" => "Wings",
        "hat" => "Hats",
        "backpack" => "Backpacks",
        "glove" => "Gloves",
        "aura" => "Auras",
        "boots" => "Boots",
        "shoulder" => "Shoulders",
        _ => "Other",
    }
}

fn slot_label(slot: &str) -> &'static str {
    match slot {
        "cape" => "Cape",
        "wings" => "Wings",
        "hat" => "Hat",
        "backpack" => "Backpack",
        "glove" => "Glove",
        "aura" => "Aura",
        "boots" => "Boots",
        "shoulder" => "Shoulder",
        "left_hand" => "Left hand",
        "right_hand" => "Right hand",
        _ => "Slot",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalog(value: Value) -> Vec<WardrobeItem> {
        parse_catalog(value).expect("catalog should parse")
    }

    #[test]
    fn parses_groups_into_variants_and_inherits_slots() {
        let items = catalog(json!({
            "cosmetics": [{
                "id": 1,
                "type": "cape",
                "name": "Cape",
                "allowed_slots": ["cape"],
                "variants": [
                    { "id": 10, "name": "Red", "cover_url": "https://example.com/red.png" },
                    { "id": 11, "name": "Blue", "allowed_slots": ["cape"] }
                ]
            }]
        }));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Cape · Blue");
        assert_eq!(items[0].slots, vec!["cape".to_string()]);
        assert_eq!(items[1].name, "Cape · Red");
        assert_eq!(items[1].cover_url.as_deref(), Some("https://example.com/red.png"));
    }

    #[test]
    fn skips_emote_groups() {
        let items = catalog(json!({
            "cosmetics": [{
                "id": 2,
                "type": "emote",
                "name": "Wave",
                "allowed_slots": [],
                "variants": [{ "id": 20, "name": "Wave" }]
            }]
        }));
        assert!(items.is_empty());
    }

    #[test]
    fn groups_single_variant_under_the_group_name() {
        let items = catalog(json!({
            "cosmetics": [{
                "id": 3,
                "type": "hat",
                "name": "Fedora",
                "allowed_slots": ["hat"],
                "variants": [{ "id": 30, "name": "Fedora" }]
            }]
        }));
        assert_eq!(items[0].name, "Fedora");
    }

    #[test]
    fn value_ids_accept_numbers_and_strings() {
        assert_eq!(value_id(&json!(7)), Some(7));
        assert_eq!(value_id(&json!("7")), Some(7));
        assert_eq!(value_id(&json!("nope")), None);
    }

    #[test]
    fn glove_matches_only_when_both_hands_hold_the_variant() {
        let slots = vec!["left_hand".to_string(), "right_hand".to_string()];
        let mut equipped = BTreeMap::new();
        equipped.insert("left_hand".to_string(), json!(4));
        assert!(!equipped_matches(&equipped, &slots, 4));
        assert!(partially_equipped(&equipped, &slots, 4));
        equipped.insert("right_hand".to_string(), json!(4));
        assert!(equipped_matches(&equipped, &slots, 4));
    }
}
