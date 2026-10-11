use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};

use freya::query::{Query, QueryCapability, UseQuery, use_query};
use oneclient_core::LauncherError;
use oneclient_mc::{self as minecraft, PlayerProfileView};

const PROFILE_STALE: Duration = Duration::from_secs(30 * 60);
const PROFILE_CLEAN: Duration = Duration::from_secs(2 * 60 * 60);
/// A profile changes rarely, so a settled lookup is replayed from memory for
/// this long instead of re-reading the on-disk cache or round-tripping the
/// session server again.
const PROFILE_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// An unknown player is remembered for far less: long enough that a mistyped
/// name cannot hammer Mojang, short enough that a name which does turn out to
/// exist is picked up quickly.
const PROFILE_MISS_TTL: Duration = Duration::from_secs(60);
/// Bound on cached lookups; the entry closest to expiring is dropped first.
const PROFILE_CACHE_MAX_ENTRIES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FetchPlayerProfileQuery;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerProfileQueryKeys {
    pub uuid: String,
    pub access_token: Option<String>,
}

/// One settled lookup. Only the rendered error is kept because
/// [LauncherError] is not `Clone`, so a replay rebuilds it from that text.
#[derive(Clone)]
struct CachedProfile {
    /// Deadline after which the entry is fetched again instead of replayed.
    fresh_until: Instant,
    result: Result<PlayerProfileView, String>,
}

impl CachedProfile {
    /// The lookup as the query reported it, fresh or not; the caller has
    /// already checked [`CachedProfile::fresh_until`].
    fn replay(&self) -> Result<PlayerProfileView, LauncherError> {
        match &self.result {
            Ok(view) => Ok(view.clone()),
            Err(message) => Err(LauncherError::Minecraft(message.clone())),
        }
    }
}

/// Process-local and memory-only: repeated lookups for the same player are
/// answered here without touching the network or the on-disk profile cache
/// underneath it.
static PROFILE_CACHE: LazyLock<RwLock<HashMap<PlayerProfileQueryKeys, CachedProfile>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cache identity for a lookup. The id is normalized the way the session
/// server URL builds it so both spellings of one player share an entry, and an
/// empty token folds to `None` because that is how it would be sent.
fn cache_key(keys: &PlayerProfileQueryKeys) -> PlayerProfileQueryKeys {
    PlayerProfileQueryKeys {
        uuid: keys
            .uuid
            .chars()
            .filter(|c| *c != '-' && !c.is_whitespace())
            .collect(),
        access_token: keys
            .access_token
            .as_deref()
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned),
    }
}

/// Reads an entry without judging its age: the caller does that from
/// [`CachedProfile::fresh_until`], so a hit costs a single read lock.
fn cached_profile(key: &PlayerProfileQueryKeys) -> Option<CachedProfile> {
    PROFILE_CACHE.read().unwrap().get(key).cloned()
}

/// Whether the failure came from the server answering rather than from the
/// network failing underneath it, which is what makes it worth remembering.
fn server_answered(err: &LauncherError) -> bool {
    match err {
        LauncherError::McError(err) => !err.is_transient(),
        LauncherError::RequestError(err) => !err.is_transient(),
        _ => false,
    }
}

/// The TTL a settled result earns, or `None` when it must not be remembered:
/// a network fault says nothing about the player, so the next run retries.
fn cached_ttl(result: &Result<PlayerProfileView, LauncherError>) -> Option<Duration> {
    match result {
        Ok(_) => Some(PROFILE_CACHE_TTL),
        Err(err) if server_answered(err) => Some(PROFILE_MISS_TTL),
        Err(_) => None,
    }
}

/// Files a settled lookup under its normalized key, evicting whatever is
/// closest to expiring once the cache is full.
fn store_profile(key: &PlayerProfileQueryKeys, result: &Result<PlayerProfileView, LauncherError>) {
    let Some(ttl) = cached_ttl(result) else {
        return;
    };
    let entry = CachedProfile {
        fresh_until: Instant::now() + ttl,
        result: match result {
            Ok(view) => Ok(view.clone()),
            Err(err) => Err(err.to_string()),
        },
    };

    let mut cache = PROFILE_CACHE.write().unwrap();
    cache.insert(key.clone(), entry);
    evict_oldest(&mut cache);
}

/// Drops whatever is closest to expiring until the cache fits again.
fn evict_oldest(cache: &mut HashMap<PlayerProfileQueryKeys, CachedProfile>) {
    while cache.len() > PROFILE_CACHE_MAX_ENTRIES {
        let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.fresh_until)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        cache.remove(&oldest);
    }
}

impl QueryCapability for FetchPlayerProfileQuery {
    type Ok = PlayerProfileView;
    type Err = LauncherError;
    type Keys = PlayerProfileQueryKeys;

    async fn run(&self, keys: &Self::Keys) -> Result<Self::Ok, Self::Err> {
        let key = cache_key(keys);
        // An id that is not there yet names no player, so it is never pinned:
        // the next run has to try it again.
        let cacheable = !key.uuid.is_empty();

        if cacheable
            && let Some(entry) = cached_profile(&key)
            && entry.fresh_until > Instant::now()
        {
            tracing::debug!(uuid = %key.uuid, "player profile: served from the profile cache");
            return entry.replay();
        }

        let access_token = keys.access_token.clone();

        let state = crate::launcher::state()?;
        let client = &state.services.requester;

        let result =
            minecraft::fetch_player_profile_view(client, &keys.uuid, access_token.as_deref())
                .await
                .map_err(LauncherError::McError);

        if cacheable {
            store_profile(&key, &result);
        }

        result
    }

    /// Invalidation is only handed out for a lookup that actually names a
    /// player; an empty id must not clear every cached profile.
    fn matches(&self, keys: &Self::Keys) -> bool {
        !cache_key(keys).uuid.is_empty()
    }
}

pub fn use_player_profile(
    uuid: impl Into<String>,
    access_token: Option<impl Into<String>>,
) -> UseQuery<FetchPlayerProfileQuery> {
    use_query(
        Query::new(
            PlayerProfileQueryKeys {
                uuid: uuid.into(),
                access_token: access_token.map(|s| s.into()),
            },
            FetchPlayerProfileQuery,
        )
        .stale_time(PROFILE_STALE)
        .clean_time(PROFILE_CLEAN),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(uuid: &str) -> PlayerProfileQueryKeys {
        PlayerProfileQueryKeys {
            uuid: uuid.to_string(),
            access_token: None,
        }
    }

    /// Session server's answer for an id no player owns.
    fn unknown_player() -> LauncherError {
        LauncherError::McError(minecraft::McError::Request(
            oneclient_net::RequestError::HttpStatus {
                status: 404,
                url: "https://sessionserver.mojang.com/session/minecraft/profile/abc".to_string(),
                snippet: "Not Found".to_string(),
            },
        ))
    }

    /// The network gave out before the server could answer.
    fn offline() -> LauncherError {
        LauncherError::McError(minecraft::McError::Io(polyio::IOError::IOError(
            std::io::Error::from(std::io::ErrorKind::TimedOut),
        )))
    }

    #[test]
    fn both_spellings_of_one_id_share_a_single_entry() {
        let dashed = cache_key(&keys("069a79f4-44e9-4726-a5be-fca90e38aaf5"));
        let bare = cache_key(&keys("069a79f444e94726a5befca90e38aaf5"));
        assert_eq!(dashed, bare);
    }

    #[test]
    fn an_empty_token_is_cached_as_anonymous() {
        let mut anonymous = keys("abc");
        anonymous.access_token = Some(String::new());
        assert_eq!(cache_key(&anonymous), cache_key(&keys("abc")));
    }

    #[test]
    fn a_settled_profile_earns_the_long_ttl() {
        let result: Result<PlayerProfileView, LauncherError> = Ok(PlayerProfileView::placeholder());
        assert_eq!(cached_ttl(&result), Some(PROFILE_CACHE_TTL));
    }

    #[test]
    fn an_unknown_player_is_remembered_briefly() {
        let result: Result<PlayerProfileView, LauncherError> = Err(unknown_player());
        assert_eq!(cached_ttl(&result), Some(PROFILE_MISS_TTL));
    }

    #[test]
    fn a_network_fault_is_not_remembered() {
        let result: Result<PlayerProfileView, LauncherError> = Err(offline());
        assert_eq!(cached_ttl(&result), None);
    }

    #[test]
    fn a_replayed_miss_carries_the_message_it_was_cached_with() {
        let entry = CachedProfile {
            fresh_until: Instant::now(),
            result: Err("HTTP 404 from session server".to_string()),
        };

        match entry.replay() {
            Ok(_) => panic!("a cached miss must not replay as a profile"),
            Err(err) => assert!(err.to_string().contains("HTTP 404"), "{err}"),
        }
    }

    #[test]
    fn a_settled_lookup_is_answered_from_the_cache() {
        let key = cache_key(&PlayerProfileQueryKeys {
            uuid: uuid::Uuid::new_v4().to_string(),
            access_token: None,
        });
        let expected = PlayerProfileView::placeholder();

        store_profile(&key, &Ok(expected.clone()));

        let entry = cached_profile(&key).expect("settled profile should be cached");
        assert!(entry.fresh_until > Instant::now(), "entry should be fresh");
        match entry.replay() {
            Ok(view) => assert_eq!(view, expected),
            Err(err) => panic!("expected a cached profile, got {err}"),
        }
    }

    #[test]
    fn the_entry_closest_to_expiring_is_evicted_first() {
        let mut cache = HashMap::new();
        let base = Instant::now();
        for i in 0..=PROFILE_CACHE_MAX_ENTRIES {
            cache.insert(
                cache_key(&keys(&format!("player-{i}"))),
                CachedProfile {
                    fresh_until: base + Duration::from_secs(i as u64),
                    result: Ok(PlayerProfileView::placeholder()),
                },
            );
        }

        evict_oldest(&mut cache);

        assert_eq!(cache.len(), PROFILE_CACHE_MAX_ENTRIES);
        assert!(
            !cache.contains_key(&cache_key(&keys("player-0"))),
            "the earliest deadline should be gone"
        );
        assert!(
            cache.contains_key(&cache_key(&keys(&format!(
                "player-{}",
                PROFILE_CACHE_MAX_ENTRIES
            )))),
            "the latest deadline should survive"
        );
    }
}
