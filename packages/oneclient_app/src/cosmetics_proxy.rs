//! Built-in local Poly+ cosmetics unlock proxy.
//!
//! Listens on loopback (`COSMETICS_PROXY_PORT`) and mirrors
//! `https://plus.polyfrost.org` for the Poly+ mod, which the launcher points
//! at it with `-Dpolyplus.apiUrl`:
//!
//! * `GET  /cosmetics/player` — every catalog item reads as owned; equipment
//!   choices are persisted under `~/.aethelone/` (seeded once from the
//!   account's official locker when one exists), keyed per account.
//! * `PUT  /cosmetics/player` — writes that local equipment state for the
//!   account behind the request's session token.
//! * `POST /account/login` — tried against the real backend first; when the
//!   backend rejects the session (offline accounts have no Mojang session),
//!   a local bearer token is minted so Poly+ proceeds and the locker below
//!   can load. Real Microsoft sessions pass through untouched.
//! * everything else — forwarded to the real backend unchanged, including the
//!   caller's Authorization header, so logins, store and socials keep working.
//!   Forwarded `GET`s are also held in a short-lived memory cache and
//!   revalidated with `ETag`/`If-None-Match` (`304`), so a backend hiccup
//!   replays the last copy instead of failing the request.
//! * `GET  /websocket` — relays to the real backend when the client presents a
//!   working upstream token (so other players still see your official
//!   unlocks); local-session and tokenless clients are answered locally
//!   (pings/pongs) so the mod's equipment sync stays healthy. Textures are
//!   never proxied: they come straight from the public CDN.
//!
//! The launcher launches every account through this one proxy, so the
//! equipment above is stored per account: the mod only attaches a bearer
//! token to the locker routes, and the account behind a request is resolved
//! through the `/account/login` exchange that minted that token (the
//! `player`/`username` query parameters the mod sends there). Requests whose
//! token is unknown share a `__default__` bucket.
//!
//! Nothing is uploaded, mirrored or shared; other players always see their
//! official ownership. The whole feature is local-only by design.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, OnceLock, RwLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{
    CONNECTION, HeaderMap, HeaderValue, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest_websocket::{CloseCode, Message, Upgrade};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

const UPSTREAM: &str = oneclient_common::constants::PLUS_BACKEND_URL;
/// Prefix of bearer tokens this proxy mints when the real backend rejects the
/// session; the websocket keys off it to answer locally instead of relaying a
/// token upstream would refuse.
const LOCAL_TOKEN_PREFIX: &str = "oneclient-local.";
const CATALOG_TTL: Duration = Duration::from_secs(600);
/// Total budget for one upstream round-trip (request + response body). The mod
/// gives the proxy a 30s socket timeout, so a stalled backend has to surface
/// here first, as an error the pipeline can handle, instead of leaving the
/// game's cosmetics screen waiting on a response that never arrives.
const FORWARD_TIMEOUT: Duration = Duration::from_secs(15);
/// Budget for the upstream websocket handshake (connect + `101` response).
const WS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const PING_INTERVAL: Duration = Duration::from_secs(30);
const MAX_FRAME_LEN: usize = 8 * 1024 * 1024;
const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

static STARTED: OnceLock<()> = OnceLock::new();
static STATE: LazyLock<ProxyState> = LazyLock::new(ProxyState::load);
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(build_forward_client);
static SEED_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
static CATALOG_REFRESHING: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------- state

/// Identifies the local player a cosmetics request belongs to: the profile id
/// or player name the mod sends with `/account/login` for the account whose
/// bearer token the request carries. Falls back to [`DEFAULT_ACCOUNT`] when
/// no account can be recognised.
type AccountId = String;

/// Bucket for requests with no recognisable account, and the parking spot for
/// state written before equipment was tracked per account (which the first
/// identified account claims, see [`loadout_mut`]). Chosen so it cannot
/// collide with a real account id (profile ids are UUIDs, player names are
/// Minecraft names).
const DEFAULT_ACCOUNT: &str = "__default__";

/// One account's cosmetics loadout: what it has equipped, its particle
/// colour, and whether its official locker has been seeded already.
#[derive(Clone, Default)]
struct Loadout {
    equipped: BTreeMap<String, Value>,
    particle_color: Option<Value>,
    seeded: bool,
}

impl Loadout {
    fn from_json(value: &Value) -> Self {
        Self {
            equipped: value
                .get("equipped")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .map(|(slot, id)| (slot.clone(), id.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            particle_color: value.get("particle_color").cloned(),
            seeded: value
                .get("seeded")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "equipped": Value::Object(self.equipped.clone().into_iter().collect()),
            "particle_color": self.particle_color.clone(),
            "seeded": self.seeded,
        })
    }
}

/// Bearer tokens issued by `/account/login`, oldest first. The mod attaches
/// its current token to every locker request, so this is the only link from
/// such a request to the account behind it. Bounded because a launcher run
/// only ever mints a handful of tokens (one per game session, plus refreshes).
#[derive(Default)]
struct Sessions {
    entries: Vec<(String, AccountId)>,
}

const SESSION_LIMIT: usize = 32;

impl Sessions {
    fn register(&mut self, token: &str, account: &AccountId) {
        if token.is_empty() {
            return;
        }
        self.entries.retain(|(known, _)| known != token);
        self.entries.push((token.to_owned(), account.clone()));
        if self.entries.len() > SESSION_LIMIT {
            self.entries.remove(0);
        }
    }

    fn resolve(&self, token: &str) -> Option<AccountId> {
        self.entries
            .iter()
            .rev()
            .find(|(known, _)| known == token)
            .map(|(_, account)| account.clone())
    }
}

#[derive(Default)]
struct ProxyState {
    catalog: RwLock<Option<(Instant, Value)>>,
    accounts: RwLock<HashMap<AccountId, Loadout>>,
    sessions: RwLock<Sessions>,
}

impl ProxyState {
    fn load() -> Self {
        Self::from_json(read_json(&state_file()))
    }

    fn from_json(value: Option<Value>) -> Self {
        let state = Self::default();
        let Some(value) = value else { return state };
        let mut accounts = state.accounts.write().unwrap();
        if let Some(object) = value.get("accounts").and_then(Value::as_object) {
            for (account, entry) in object {
                accounts.insert(account.clone(), Loadout::from_json(entry));
            }
        } else if value.get("equipped").is_some()
            || value.get("particle_color").is_some()
            || value.get("seeded").is_some()
        {
            // State written before equipment was tracked per account: park it
            // in the account-less bucket until a real account claims it.
            accounts.insert(DEFAULT_ACCOUNT.to_owned(), Loadout::from_json(&value));
        }
        drop(accounts);
        state
    }

    fn to_json(&self) -> Value {
        let accounts: serde_json::Map<String, Value> = self
            .accounts
            .read()
            .unwrap()
            .iter()
            .map(|(account, loadout)| (account.clone(), loadout.to_json()))
            .collect();
        json!({ "accounts": Value::Object(accounts) })
    }

    fn persist(&self) {
        write_json_atomic(&state_file(), &self.to_json());
    }

    /// Snapshot of `account`'s loadout, claiming any account-less state parked
    /// in [`DEFAULT_ACCOUNT`] on the way (see [`loadout_mut`]).
    fn loadout(&self, account: &AccountId) -> Loadout {
        let mut accounts = self.accounts.write().unwrap();
        loadout_mut(&mut accounts, account).clone()
    }

    fn with_loadout<R>(&self, account: &AccountId, update: impl FnOnce(&mut Loadout) -> R) -> R {
        let mut accounts = self.accounts.write().unwrap();
        update(loadout_mut(&mut accounts, account))
    }

    fn seeded(&self, account: &AccountId) -> bool {
        self.loadout(account).seeded
    }

    fn register_session(&self, token: &str, account: &AccountId) {
        self.sessions.write().unwrap().register(token, account);
    }

    fn session_account(&self, token: &str) -> Option<AccountId> {
        self.sessions.read().unwrap().resolve(token)
    }
}

/// Returns `account`'s loadout, storing an empty one when the account is new
/// to this proxy.
///
/// State written before equipment was tracked per account lives under
/// [`DEFAULT_ACCOUNT`]: the first identified account to show up is the one
/// that owned it, so it claims the whole bucket (equipment, particle colour
/// and the "already seeded" flag travel with it), leaving later accounts to
/// start empty and seed their own official locker.
fn loadout_mut<'a>(
    accounts: &'a mut HashMap<AccountId, Loadout>,
    account: &str,
) -> &'a mut Loadout {
    if !accounts.contains_key(account) {
        let inherited = if account == DEFAULT_ACCOUNT {
            Loadout::default()
        } else {
            accounts.remove(DEFAULT_ACCOUNT).unwrap_or_default()
        };
        accounts.insert(account.to_owned(), inherited);
    }
    accounts
        .get_mut(account)
        .expect("a loadout was just stored for the account")
}

fn state_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|base| base.home_dir().join(".aethelone"))
}

fn state_file() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("cosmetics_state.json"))
}

fn catalog_file() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("cosmetics_catalog.json"))
}

fn read_json(path: &Option<PathBuf>) -> Option<Value> {
    let path = path.as_ref()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_json_atomic(path: &Option<PathBuf>, value: &Value) {
    let Some(path) = path.as_ref() else { return };
    if let Some(parent) = path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        warn!("cosmetics proxy: cannot create {}: {err}", parent.display());
        return;
    }
    let tmp = path.with_extension("tmp");
    match serde_json::to_vec_pretty(value) {
        Ok(bytes) => {
            let written = std::fs::write(&tmp, &bytes).is_ok();
            if written && std::fs::rename(&tmp, path).is_ok() {
                return;
            }
            debug!("cosmetics proxy: failed to persist {}", path.display());
        }
        Err(err) => debug!("cosmetics proxy: state encode failed: {err}"),
    }
}

// -------------------------------------------------------------- forwarding

fn is_hop_by_hop(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
            | "accept-encoding"
            | "expect"
    )
}

fn build_forward_client() -> reqwest::Client {
    match reqwest::Client::builder()
        .tcp_keepalive(Some(Duration::from_secs(15)))
        .connect_timeout(Duration::from_secs(10))
        .timeout(FORWARD_TIMEOUT)
        // follow nothing: the game client receives redirects and handles them
        .redirect(reqwest::redirect::Policy::none())
        .http1_only()
        .tls_backend_rustls()
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            warn!("cosmetics proxy: forward client build failed: {err}");
            reqwest::Client::builder()
                .timeout(FORWARD_TIMEOUT)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new())
        }
    }
}

async fn forward_request(
    method: &Method,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, String> {
    forward_to(&CLIENT, UPSTREAM, method, path_and_query, headers, body).await
}

async fn forward_to(
    client: &reqwest::Client,
    base: &str,
    method: &Method,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, String> {
    let url = format!("{base}{path_and_query}");
    let method =
        reqwest::Method::from_bytes(method.as_str().as_bytes()).map_err(|err| err.to_string())?;
    let mut request = client.request(method, &url);
    for (name, value) in headers {
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        request = request.header(name.as_str(), value.as_bytes());
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    request
        .send()
        .await
        .map_err(|err| format!("upstream: {err}"))
}

fn json_response(status: StatusCode, value: &Value) -> Response<Full<Bytes>> {
    let body = serde_json::to_vec(value)
        .map(Bytes::from)
        .unwrap_or_else(|_| Bytes::from_static(b"{}"));
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header(
            oneclient_common::constants::COSMETICS_PROXY_MARKER,
            oneclient_common::constants::COSMETICS_PROXY_MARKER_VALUE,
        )
        .body(Full::new(body))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b"{}"))))
}

fn error_json(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    json_response(status, &json!({ "error": message }))
}

// ---------------------------------------------------------------- catalog

/// Empty catalog baked into the binary: a fresh install (or a wiped cache)
/// with the backend down must still answer `/cosmetics` and keep the locally
/// stored equipment reachable, instead of failing with `502`.
static BUNDLED_CATALOG: &[u8] = include_bytes!("../assets/cosmetics_catalog.json");

async fn catalog() -> Result<Value, String> {
    catalog_from(&CLIENT, UPSTREAM, &catalog_file(), Some(BUNDLED_CATALOG)).await
}

/// Answers from the first tier that can: the in-memory cache, the on-disk
/// cache (served instantly while a refresh runs in the background), a blocking
/// upstream refresh, and finally `fallback` (the bundled catalog), which is
/// served with a warning so a downed backend never blanks the locker.
async fn catalog_from(
    client: &reqwest::Client,
    base: &str,
    disk: &Option<PathBuf>,
    fallback: Option<&[u8]>,
) -> Result<Value, String> {
    if let Some((fetched_at, value)) = STATE.catalog.read().unwrap().clone()
        && fetched_at.elapsed() < CATALOG_TTL
    {
        return Ok(value);
    }
    // Answer instantly from the on-disk cache while revalidating behind the
    // response: the game's locker screen must never wait on a slow backend.
    if let Some(value) = read_json(disk) {
        spawn_catalog_refresh(client.clone(), base.to_owned(), disk.clone());
        return Ok(value);
    }
    match refresh_catalog(client, base, disk).await {
        Ok(value) => Ok(value),
        Err(err) => {
            let Some(bytes) = fallback else {
                return Err(err);
            };
            match serde_json::from_slice::<Value>(bytes) {
                Ok(value) => {
                    warn!("cosmetics proxy: {err}; serving the bundled fallback catalog");
                    Ok(value)
                }
                Err(decode) => Err(format!("{err}; bundled fallback decode failed: {decode}")),
            }
        }
    }
}

fn spawn_catalog_refresh(client: reqwest::Client, base: String, disk: Option<PathBuf>) {
    if CATALOG_REFRESHING
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    tokio::spawn(async move {
        if let Err(err) = refresh_catalog(&client, &base, &disk).await {
            debug!("cosmetics proxy: background catalog refresh failed ({err})");
        }
        CATALOG_REFRESHING.store(false, Ordering::Relaxed);
    });
}

async fn refresh_catalog(
    client: &reqwest::Client,
    base: &str,
    disk: &Option<PathBuf>,
) -> Result<Value, String> {
    let response = forward_to(
        client,
        base,
        &Method::GET,
        "/cosmetics",
        &HeaderMap::new(),
        Bytes::new(),
    )
    .await
    .map_err(|err| format!("catalog refresh failed: {err}"))?;
    if response.status() != 200 {
        return Err(format!("catalog refresh failed: {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("catalog read failed: {err}"))?;
    let value = serde_json::from_slice::<Value>(&bytes)
        .map_err(|err| format!("catalog decode failed: {err}"))?;
    if let Some(path) = disk {
        let _ = std::fs::write(path, &bytes);
    }
    // Only a successful refresh restarts the TTL clock: a failed one must
    // leave the stale entry (or no entry) untouched, so the next request
    // retries instead of waiting out another full TTL on a broken cache.
    *STATE.catalog.write().unwrap() = Some((Instant::now(), value.clone()));
    Ok(value)
}

fn split_catalog(catalog: &Value) -> (Vec<Value>, Vec<Value>) {
    let groups = catalog
        .get("cosmetics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut cosmetics = Vec::new();
    let mut emotes = Vec::new();
    for group in groups {
        if group.get("type").and_then(Value::as_str) == Some("emote") {
            let Some(variants) = group.get("variants").and_then(Value::as_array) else {
                continue;
            };
            for variant in variants {
                let (Some(id), Some(hash)) = (variant.get("id"), variant.get("hash")) else {
                    continue;
                };
                let name = variant
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| json!("Emote"));
                let url = variant.get("url").cloned().unwrap_or(Value::Null);
                emotes.push(json!({ "id": id, "name": name, "url": url, "hash": hash }));
            }
        } else {
            cosmetics.push(group);
        }
    }
    (cosmetics, emotes)
}

// ------------------------------------------------------------ player state

/// The account behind a locker request. The mod attaches the bearer token
/// `/account/login` issued to every `/cosmetics/player` call, so a token this
/// proxy has seen resolves to the account behind its login; requests with no
/// token (or a token from before a restart) share [`DEFAULT_ACCOUNT`].
fn request_account(headers: &HeaderMap) -> AccountId {
    bearer_token(headers)
        .and_then(|token| STATE.session_account(token))
        .unwrap_or_else(|| DEFAULT_ACCOUNT.to_owned())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(hyper::header::AUTHORIZATION)?.to_str().ok()?;
    let token = match value.get(..7) {
        Some(prefix) if prefix.eq_ignore_ascii_case("bearer ") => value.get(7..)?,
        _ => value,
    };
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// The account behind a `/account/login` call: the mod names the player's
/// profile id (`player`) towards a local backend, and the player name
/// (`username`) towards every backend, as query parameters.
fn account_from_login(path_and_query: &str, body: &[u8]) -> Option<AccountId> {
    query_param(path_and_query, "player")
        .or_else(|| query_param(path_and_query, "username"))
        .or_else(|| {
            let value = serde_json::from_slice::<Value>(body).ok()?;
            ["player", "username", "selectedProfile", "playerId"]
                .iter()
                .find_map(|key| value.get(*key).and_then(Value::as_str).map(str::to_owned))
        })
        .filter(|account| !account.is_empty())
}

/// Percent-decoding query lookup, for the parameters the mod URL-encodes into
/// the login call.
fn query_param(path_and_query: &str, name: &str) -> Option<String> {
    let url = reqwest::Url::parse(&format!("http://localhost{path_and_query}")).ok()?;
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

async fn seed_from_upstream(account: &AccountId, headers: &HeaderMap) {
    let result = async {
        let response =
            forward_request(&Method::GET, "/cosmetics/player", headers, Bytes::new()).await?;
        if response.status() != 200 {
            return Err(format!("seed status {}", response.status()));
        }
        response
            .json::<Value>()
            .await
            .map_err(|err| format!("seed decode: {err}"))
    }
    .await;
    match result {
        Ok(official) => {
            STATE.with_loadout(account, |loadout| {
                if let Some(object) = official.get("equipped").and_then(Value::as_object) {
                    loadout.equipped.clear();
                    for (slot, id) in object {
                        if !id.is_null() {
                            loadout.equipped.insert(slot.clone(), id.clone());
                        }
                    }
                }
                let color = official.get("particle_color").cloned();
                if color.as_ref().is_some_and(|value| !value.is_null()) {
                    loadout.particle_color = color;
                }
            });
            info!(account = %account, "cosmetics proxy: seeded from the official locker");
        }
        Err(err) => debug!("cosmetics proxy: official seed skipped ({err}); starting empty"),
    }
}

async fn handle_player_get(request: &Request<Incoming>) -> Response<Full<Bytes>> {
    let account = request_account(request.headers());
    if !STATE.seeded(&account) {
        let _guard = SEED_LOCK.lock().await;
        if !STATE.seeded(&account) {
            seed_from_upstream(&account, request.headers()).await;
            STATE.with_loadout(&account, |loadout| loadout.seeded = true);
            STATE.persist();
        }
    }
    let catalog = match catalog().await {
        Ok(catalog) => catalog,
        Err(err) => {
            // Offline with no usable cache at all: the locally stored
            // equipment must still reach the game (with an empty item list)
            // instead of the whole locker coming up blank behind a 502.
            warn!("cosmetics proxy: catalog unavailable ({err}); serving the local equipment only");
            json!({ "cosmetics": [] })
        }
    };
    let (cosmetics, emotes) = split_catalog(&catalog);
    let loadout = STATE.loadout(&account);
    info!(
        account = %account,
        groups = cosmetics.len(),
        emotes = emotes.len(),
        "cosmetics proxy: served the local player locker (every item owned)"
    );
    json_response(
        StatusCode::OK,
        &json!({
            "cosmetics": cosmetics,
            "emotes": emotes,
            "equipped": Value::Object(loadout.equipped.into_iter().collect()),
            "particle_color": loadout.particle_color,
        }),
    )
}

async fn handle_player_put(request: Request<Incoming>) -> Response<Full<Bytes>> {
    let account = request_account(request.headers());
    let body = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "bad body"),
    };
    let patch: Value = match serde_json::from_slice(&body) {
        Ok(patch) => patch,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "bad json"),
    };
    STATE.with_loadout(&account, |loadout| {
        if let Some(object) = patch.get("equipped").and_then(Value::as_object) {
            for (slot, id) in object {
                if id.is_null() {
                    loadout.equipped.remove(slot);
                } else {
                    loadout.equipped.insert(slot.clone(), id.clone());
                }
            }
        }
        if let Some(color) = patch.get("particle_color")
            && !color.is_null()
        {
            loadout.particle_color = Some(color.clone());
        }
    });
    STATE.persist();
    let equipped = STATE.loadout(&account).equipped;
    info!(
        account = %account,
        slots = equipped.len(),
        "cosmetics proxy: equipment change stored locally"
    );
    json_response(
        StatusCode::OK,
        &json!({ "equipped": Value::Object(equipped.into_iter().collect()) }),
    )
}

async fn handle_forward(request: Request<Incoming>) -> Response<Full<Bytes>> {
    let method = request.method().clone();
    let path = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    let headers = request.headers().clone();
    let body = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => return error_json(StatusCode::BAD_REQUEST, &format!("body: {err}")),
    };
    build_forward_response(&method, &path, &headers, body).await
}

async fn build_forward_response(
    method: &Method,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Response<Full<Bytes>> {
    if method == Method::GET {
        // `GET`s ride the response cache: replayed while fresh, revalidated
        // with `If-None-Match` once stale (see `forward_get_from`).
        return forward_get_from(&CLIENT, UPSTREAM, path_and_query, headers, body).await;
    }
    match forward_request(method, path_and_query, headers, body).await {
        Ok(response) => {
            let status =
                StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let headers = response.headers().clone();
            let body = response.bytes().await.unwrap_or_default();
            pass_through(status, &headers, body)
        }
        Err(err) => error_json(StatusCode::BAD_GATEWAY, &err),
    }
}

/// Hands an upstream response back to the game: hop-by-hop headers stay
/// behind, everything else (and the body) is passed on untouched.
fn pass_through(status: StatusCode, headers: &HeaderMap, body: Bytes) -> Response<Full<Bytes>> {
    let mut builder = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    builder
        .body(Full::new(body))
        .unwrap_or_else(|_| error_json(StatusCode::BAD_GATEWAY, "response build"))
}

/// Logins go to the real backend first; only a rejected session (offline
/// accounts have no Mojang sessionserver proof, so plus.polyfrost.org answers
/// 401) falls back to a locally minted token, which is enough for Poly+ to
/// finish authorizing and load the unlocked locker.
async fn handle_account_login(request: Request<Incoming>) -> Response<Full<Bytes>> {
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "/account/login".to_string());
    let headers = request.headers().clone();
    let body = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => return error_json(StatusCode::BAD_REQUEST, &format!("body: {err}")),
    };
    let account = account_from_login(&path_and_query, &body);
    let response = build_forward_response(&Method::POST, &path_and_query, &headers, body).await;
    let status = response.status();
    if status.is_success() {
        return remember_session(response, account).await;
    }
    if status != StatusCode::UNAUTHORIZED
        && status != StatusCode::FORBIDDEN
        && status != StatusCode::BAD_GATEWAY
        && !status.is_server_error()
    {
        return response;
    }
    let token = format!("{LOCAL_TOKEN_PREFIX}{}", uuid::Uuid::new_v4());
    if let Some(account) = account.as_ref() {
        STATE.register_session(&token, account);
    }
    info!(
        status = %status,
        "cosmetics proxy: real backend rejected the session; issuing a local token instead"
    );
    json_response(StatusCode::OK, &json!({ "token": token }))
}

/// Binds a freshly issued session token to the account that logged in, so the
/// locker requests that follow (the mod only sends the token) can be keyed to
/// that account. The response itself is handed back untouched.
async fn remember_session(
    response: Response<Full<Bytes>>,
    account: Option<AccountId>,
) -> Response<Full<Bytes>> {
    let Some(account) = account else {
        return response;
    };
    let (parts, body) = response.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => return error_json(StatusCode::BAD_GATEWAY, &format!("login body: {err}")),
    };
    if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
        && let Some(token) = value.get("token").and_then(Value::as_str)
    {
        STATE.register_session(token, &account);
        debug!(account = %account, "cosmetics proxy: session token bound to its account");
    }
    Response::from_parts(parts, Full::new(bytes))
}

// -------------------------------------------------------- response cache

/// How long a forwarded `GET` may be replayed without consulting the upstream.
/// A refresh that fails never re-stamps this: the stale copy keeps answering
/// (the catalog tiers work the same way) while the next request retries, so a
/// backend hiccup degrades to last-known data instead of a `502`.
const FORWARD_CACHE_TTL: Duration = Duration::from_secs(300);
/// Bound on cached `GET`s; the entry closest to expiring is dropped first.
const FORWARD_CACHE_MAX_ENTRIES: usize = 64;

/// One forwarded `200 GET`, kept past its deadline so a dead backend can still
/// be answered from memory.
#[derive(Clone)]
struct CachedGet {
    /// Deadline after which the entry is revalidated instead of replayed.
    fresh_until: Instant,
    /// Validator handed to the caller: the upstream's `ETag` when it sent one,
    /// otherwise a quoted `sha1` of the body.
    etag: String,
    headers: HeaderMap,
    body: Bytes,
}

impl CachedGet {
    /// Wraps a response that just arrived from the upstream, ready to be
    /// replayed for a full [`FORWARD_CACHE_TTL`].
    fn new(etag: String, headers: HeaderMap, body: Bytes) -> Self {
        Self {
            fresh_until: Instant::now() + FORWARD_CACHE_TTL,
            etag,
            headers,
            body,
        }
    }

    /// Restarts the TTL. Only a round-trip that proved the representation is
    /// unchanged may call this; a failed one must leave the deadline alone so
    /// the next request retries instead of waiting out another full TTL on a
    /// refresh that never happened.
    fn refresh(&mut self) {
        self.fresh_until = Instant::now() + FORWARD_CACHE_TTL;
    }
}

/// Keyed by path and query; the validator lives inside the entry, so a URL is
/// always answered from the copy its caller last saw.
static FORWARD_CACHE: LazyLock<RwLock<BTreeMap<String, CachedGet>>> =
    LazyLock::new(|| RwLock::new(BTreeMap::new()));

/// `If-None-Match` comparison (RFC 9110 §13.1.2): `*` matches everything, a
/// list matches member-wise, and the weak prefix is ignored on both sides.
fn if_none_match_hits(value: &str, etag: &str) -> bool {
    fn strip_weak(value: &str) -> &str {
        let value = value.trim();
        value.strip_prefix("W/").unwrap_or(value).trim()
    }
    if value.trim() == "*" {
        return true;
    }
    value
        .split(',')
        .any(|candidate| strip_weak(candidate) == strip_weak(etag))
}

/// The validator to hand back with a forwarded `GET`: the upstream's `ETag`
/// when it sent one, else a quoted `sha1` of the body, so a later
/// `If-None-Match` can be answered with `304` without re-sending the payload.
fn forward_etag(headers: &HeaderMap, body: &[u8]) -> String {
    if let Some(value) = headers.get(hyper::header::ETAG)
        && let Ok(text) = value.to_str()
        && !text.is_empty()
    {
        return text.to_owned();
    }
    let mut hasher = Sha1::new();
    hasher.update(body);
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("\"{hex}\"")
}

fn cached_get(key: &str) -> Option<CachedGet> {
    FORWARD_CACHE.read().unwrap().get(key).cloned()
}

/// Files an entry under its path and query, evicting whatever is closest to
/// expiring once the cache is full. The deadline is never touched here: only
/// [`CachedGet::new`] and [`CachedGet::refresh`] hand out TTLs.
fn store_get(key: &str, entry: &CachedGet) {
    let mut cache = FORWARD_CACHE.write().unwrap();
    cache.insert(key.to_owned(), entry.clone());
    while cache.len() > FORWARD_CACHE_MAX_ENTRIES {
        let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.fresh_until)
            .map(|(key, _)| key.to_owned())
        else {
            break;
        };
        cache.remove(&oldest);
    }
}

/// Answers from the cached copy: `304` when the caller's `If-None-Match`
/// still names it, otherwise the stored headers and body.
fn replay_get(entry: &CachedGet, caller: Option<&HeaderValue>) -> Response<Full<Bytes>> {
    if let Some(caller) = caller
        && caller
            .to_str()
            .is_ok_and(|value| if_none_match_hits(value, &entry.etag))
    {
        return not_modified(&entry.etag);
    }
    let mut builder = Response::builder().status(StatusCode::OK);
    for (name, value) in entry.headers.iter() {
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    builder
        .header(hyper::header::ETAG, entry.etag.as_str())
        .body(Full::new(entry.body.clone()))
        .unwrap_or_else(|_| error_json(StatusCode::BAD_GATEWAY, "cached response build"))
}

fn not_modified(etag: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(hyper::header::ETAG, etag)
        .body(Full::new(Bytes::new()))
        .unwrap_or_else(|_| error_json(StatusCode::BAD_GATEWAY, "not modified build"))
}

/// Forwards a `GET` behind the response cache: a fresh entry is replayed (or
/// answered with `304` when the caller's `If-None-Match` still matches), a
/// stale one is revalidated with the validator we handed out last time, and a
/// failed round-trip serves the stale body rather than erroring — only a
/// successful one earns the entry another TTL.
async fn forward_get_from(
    client: &reqwest::Client,
    base: &str,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Response<Full<Bytes>> {
    let caller = headers.get(hyper::header::IF_NONE_MATCH);
    let cached = cached_get(path_and_query);
    if let Some(entry) = cached.as_ref()
        && entry.fresh_until > Instant::now()
    {
        debug!("cosmetics proxy: served a forwarded GET from the response cache");
        return replay_get(entry, caller);
    }
    let mut upstream_headers = headers.clone();
    if let Some(entry) = cached.as_ref()
        && let Ok(value) = HeaderValue::from_bytes(entry.etag.as_bytes())
    {
        // Revalidate with *our* validator: the caller's may name an older
        // representation and would make the upstream resend a whole body.
        upstream_headers.insert(hyper::header::IF_NONE_MATCH, value);
    }
    let forwarded = forward_to(
        client,
        base,
        &Method::GET,
        path_and_query,
        &upstream_headers,
        body,
    )
    .await;
    let response = match forwarded {
        Ok(response) => response,
        Err(err) => {
            if let Some(entry) = cached.as_ref() {
                warn!(
                    "cosmetics proxy: forwarded GET refresh failed ({err}); \
                     serving the cached response"
                );
                return replay_get(entry, caller);
            }
            return error_json(StatusCode::BAD_GATEWAY, &err);
        }
    };
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = response.headers().clone();
    if status == StatusCode::NOT_MODIFIED {
        let Some(mut entry) = cached else {
            // Nothing cached to revalidate against: hand the `304` through.
            return pass_through(status, &upstream_headers, Bytes::new());
        };
        // The representation is unchanged, so the stored copy earns a TTL.
        entry.refresh();
        store_get(path_and_query, &entry);
        return replay_get(&entry, caller);
    }
    let body = match response.bytes().await {
        Ok(body) => body,
        Err(err) => {
            if let Some(entry) = cached.as_ref() {
                warn!(
                    "cosmetics proxy: forwarded GET body read failed ({err}); \
                     serving the cached response"
                );
                return replay_get(entry, caller);
            }
            return error_json(StatusCode::BAD_GATEWAY, &format!("body: {err}"));
        }
    };
    if status != StatusCode::OK {
        let Some(entry) = cached.as_ref() else {
            return pass_through(status, &upstream_headers, body);
        };
        // Failures are never cached, and never restart the TTL: the stale
        // copy answers while the next request retries the refresh.
        warn!(
            "cosmetics proxy: forwarded GET answered {status}; \
             serving the cached response"
        );
        return replay_get(entry, caller);
    }
    let mut stored_headers = HeaderMap::new();
    for (name, value) in upstream_headers.iter() {
        if !is_hop_by_hop(name.as_str()) && name != &hyper::header::ETAG {
            stored_headers.append(name, value.clone());
        }
    }
    let etag = forward_etag(&upstream_headers, &body);
    let entry = CachedGet::new(etag, stored_headers, body);
    store_get(path_and_query, &entry);
    replay_get(&entry, caller)
}

// --------------------------------------------------------------- websocket

fn ws_accept(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

fn is_ws_upgrade(request: &Request<Incoming>) -> bool {
    let upgraded = request
        .headers()
        .get(UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let connection = request
        .headers()
        .get(CONNECTION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("upgrade"));
    request.method() == Method::GET && upgraded && connection
}

fn ws_handshake(request: &mut Request<Incoming>) -> Response<Full<Bytes>> {
    let Some(key) = request
        .headers()
        .get(SEC_WEBSOCKET_KEY)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
    else {
        return error_json(StatusCode::BAD_REQUEST, "missing Sec-WebSocket-Key");
    };
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "/websocket".to_string());
    let auth = request.headers().get(hyper::header::AUTHORIZATION).cloned();
    let upgrade = hyper::upgrade::on(&mut *request);
    tokio::spawn(async move {
        match upgrade.await {
            Ok(upgraded) => ws_session(TokioIo::new(upgraded), path_and_query, auth).await,
            Err(err) => debug!("cosmetics proxy: ws upgrade failed: {err}"),
        }
    });
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(CONNECTION, "upgrade")
        .header(UPGRADE, "websocket")
        .header(SEC_WEBSOCKET_ACCEPT, ws_accept(&key))
        .body(Full::new(Bytes::new()))
        .unwrap_or_else(|_| error_json(StatusCode::INTERNAL_SERVER_ERROR, "handshake"))
}

async fn upstream_ws_connect(
    path_and_query: &str,
    auth: hyper::header::HeaderValue,
) -> Result<reqwest_websocket::WebSocket, String> {
    upstream_ws_connect_to(UPSTREAM, path_and_query, auth, WS_HANDSHAKE_TIMEOUT).await
}

async fn upstream_ws_connect_to(
    base: &str,
    path_and_query: &str,
    auth: hyper::header::HeaderValue,
    handshake_timeout: Duration,
) -> Result<reqwest_websocket::WebSocket, String> {
    let client = reqwest::Client::builder()
        .tcp_keepalive(Some(Duration::from_secs(15)))
        .connect_timeout(Duration::from_secs(10))
        .http1_only()
        .tls_backend_rustls()
        .build()
        .map_err(|err| err.to_string())?;
    // Only the handshake is bounded: once the upgrade completes no request
    // timeout applies, so a healthy long-lived session is never cut off.
    let handshake = async {
        let response = client
            .get(format!("{base}{path_and_query}"))
            .header(hyper::header::AUTHORIZATION, auth)
            .upgrade()
            .send()
            .await
            .map_err(|err| err.to_string())?;
        response
            .into_websocket()
            .await
            .map_err(|err| err.to_string())
    };
    match tokio::time::timeout(handshake_timeout, handshake).await {
        Ok(result) => result,
        Err(_) => Err("upstream ws handshake timed out".to_owned()),
    }
}

async fn ws_session(
    client: TokioIo<Upgraded>,
    path_and_query: String,
    auth: Option<hyper::header::HeaderValue>,
) {
    let local_session = auth.as_ref().is_some_and(|value| {
        value
            .to_str()
            .is_ok_and(|text| text.contains(LOCAL_TOKEN_PREFIX))
    });
    let mut upstream = None;
    if let Some(auth) = auth {
        if local_session {
            debug!("cosmetics proxy: local session token; skipping the upstream ws");
        } else {
            match upstream_ws_connect(&path_and_query, auth).await {
                Ok(websocket) => upstream = Some(websocket),
                Err(err) => debug!("cosmetics proxy: no upstream ws ({err}); echoing locally"),
            }
        }
    }
    match upstream {
        Some(websocket) => {
            info!("cosmetics proxy: relaying game websocket to the official backend");
            relay_session(client, websocket).await
        }
        None => {
            info!("cosmetics proxy: game websocket answered locally");
            echo_session(client).await
        }
    }
}

async fn relay_session(mut client: TokioIo<Upgraded>, mut websocket: reqwest_websocket::WebSocket) {
    let mut assembler = FrameAssembler::default();
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping.tick().await;
    loop {
        tokio::select! {
            frame = read_frame(&mut client) => {
                let outcome: Result<Option<()>, ()> = match frame {
                    Ok(None) => Ok(None),
                    Ok(Some(frame)) => match assembler.push(frame) {
                        Err(()) => Err(()),
                        Ok(None) => Ok(None),
                        Ok(Some((opcode, payload))) => match opcode {
                            0x1 | 0x2 => websocket
                                .send(to_upstream(opcode, &payload))
                                .await
                                .map(|_| None)
                                .map_err(|_| ()),
                            0x8 => Err(()),
                            0x9 => {
                                let _ = write_frame(&mut client, 0xA, &payload).await;
                                let _ = websocket.send(Message::Ping(Bytes::from(payload))).await;
                                Ok(None)
                            }
                            0xA => {
                                let _ = websocket.send(Message::Pong(Bytes::from(payload))).await;
                                Ok(None)
                            }
                            _ => Ok(None),
                        },
                    },
                    Err(_) => Err(()),
                };
                if outcome.is_err() {
                    break;
                }
            }
            message = websocket.next() => {
                match message {
                    Some(Ok(message)) => {
                        if !forward_to_client(&mut client, &message).await {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            _ = ping.tick() => {
                if write_frame(&mut client, 0x9, b"").await.is_err()
                    || websocket.send(Message::Ping(Bytes::new())).await.is_err()
                {
                    break;
                }
            }
        }
    }
    let _ = websocket
        .send(Message::Close {
            code: CloseCode::Normal,
            reason: String::new(),
        })
        .await;
    let _ = write_frame(&mut client, 0x8, &1000u16.to_be_bytes()).await;
}

async fn echo_session(mut client: TokioIo<Upgraded>) {
    let mut assembler = FrameAssembler::default();
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping.tick().await;
    loop {
        tokio::select! {
            frame = read_frame(&mut client) => {
                let outcome: Result<Option<()>, ()> = match frame {
                    Ok(None) => Err(()),
                    Ok(Some(frame)) => match assembler.push(frame) {
                        Err(()) => Err(()),
                        Ok(None) => Ok(None),
                        Ok(Some((opcode, payload))) => match opcode {
                            0x8 => Err(()),
                            0x9 => write_frame(&mut client, 0xA, &payload)
                                .await
                                .map(|_| None)
                                .map_err(|_| ()),
                            _ => Ok(None),
                        },
                    },
                    Err(_) => Err(()),
                };
                if outcome.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if write_frame(&mut client, 0x9, b"").await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = write_frame(&mut client, 0x8, &1000u16.to_be_bytes()).await;
}

fn to_upstream(opcode: u8, payload: &[u8]) -> Message {
    if opcode == 0x2 {
        Message::Binary(Bytes::copy_from_slice(payload))
    } else {
        Message::Text(String::from_utf8_lossy(payload).into_owned())
    }
}

async fn forward_to_client(client: &mut TokioIo<Upgraded>, message: &Message) -> bool {
    let (opcode, payload): (u8, Vec<u8>) = match message {
        Message::Text(text) => (0x1, text.as_bytes().to_vec()),
        Message::Binary(binary) => (0x2, binary.to_vec()),
        Message::Ping(binary) => (0x9, binary.to_vec()),
        Message::Pong(binary) => (0xA, binary.to_vec()),
        Message::Close { code, reason } => {
            let mut payload = close_code_u16(*code).to_be_bytes().to_vec();
            payload.extend_from_slice(reason.as_bytes());
            let _ = write_frame(client, 0x8, &payload).await;
            return false;
        }
    };
    write_frame(client, opcode, &payload).await.is_ok()
}

fn close_code_u16(code: CloseCode) -> u16 {
    match code {
        CloseCode::Normal => 1000,
        CloseCode::Away => 1001,
        CloseCode::Protocol => 1002,
        CloseCode::Unsupported => 1003,
        CloseCode::Status => 1005,
        CloseCode::Abnormal => 1006,
        CloseCode::Invalid => 1007,
        CloseCode::Policy => 1008,
        CloseCode::Size => 1009,
        CloseCode::Extension => 1010,
        CloseCode::Error => 1011,
        CloseCode::Restart => 1012,
        CloseCode::Again => 1013,
        CloseCode::Tls => 1015,
        CloseCode::Reserved(code) | CloseCode::Iana(code) => code,
        _ => 1000,
    }
}

// ------------------------------------------------------------- frame codec

#[derive(Debug, PartialEq, Eq)]
struct Frame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

async fn read_exact_or_eof<R: AsyncReadExt + Unpin>(
    io: &mut R,
    buffer: &mut [u8],
) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = io.read(&mut buffer[filled..]).await?;
        if read == 0 {
            return if filled == 0 {
                Ok(false)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partial frame header",
                ))
            };
        }
        filled += read;
    }
    Ok(true)
}

async fn read_frame<R: AsyncReadExt + Unpin>(io: &mut R) -> io::Result<Option<Frame>> {
    let mut head = [0u8; 2];
    if !read_exact_or_eof(io, &mut head).await? {
        return Ok(None);
    }
    let fin = head[0] & 0x80 != 0;
    let opcode = head[0] & 0x0F;
    let masked = head[1] & 0x80 != 0;
    let len = match head[1] & 0x7F {
        126 => {
            let mut bytes = [0u8; 2];
            io.read_exact(&mut bytes).await?;
            u16::from_be_bytes(bytes) as usize
        }
        127 => {
            let mut bytes = [0u8; 8];
            io.read_exact(&mut bytes).await?;
            u64::from_be_bytes(bytes) as usize
        }
        short => short as usize,
    };
    if len > MAX_FRAME_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds size limit",
        ));
    }
    let mut mask = [0u8; 4];
    if masked {
        io.read_exact(&mut mask).await?;
    }
    let mut payload = vec![0u8; len];
    io.read_exact(&mut payload).await?;
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    Ok(Some(Frame {
        fin,
        opcode,
        payload,
    }))
}

async fn write_frame<W: AsyncWriteExt + Unpin>(
    io: &mut W,
    opcode: u8,
    payload: &[u8],
) -> io::Result<()> {
    let mut out = Vec::with_capacity(payload.len() + 10);
    out.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        out.push(len as u8);
    } else if len <= u16::MAX as usize {
        out.push(126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
    io.write_all(&out).await?;
    io.flush().await
}

#[derive(Default)]
struct FrameAssembler {
    opcode: Option<u8>,
    buffer: Vec<u8>,
}

impl FrameAssembler {
    /// Returns completed messages `(opcode, payload)`; data fragments are
    /// reassembled and control frames pass straight through.
    fn push(&mut self, frame: Frame) -> Result<Option<(u8, Vec<u8>)>, ()> {
        match frame.opcode {
            opcode @ 0x8..=0xA => {
                if !frame.fin || self.opcode.is_some() {
                    return Err(());
                }
                Ok(Some((opcode, frame.payload)))
            }
            0x0 => {
                let Some(opcode) = self.opcode.take() else {
                    return Err(());
                };
                self.buffer.extend(&frame.payload);
                if frame.fin {
                    Ok(Some((opcode, std::mem::take(&mut self.buffer))))
                } else {
                    Ok(None)
                }
            }
            opcode @ 0x1..=0x2 => {
                if self.opcode.is_some() {
                    return Err(());
                }
                if frame.fin {
                    Ok(Some((opcode, frame.payload)))
                } else {
                    self.opcode = Some(opcode);
                    self.buffer = frame.payload;
                    Ok(None)
                }
            }
            _ => Err(()),
        }
    }
}

// ------------------------------------------------------------------ server

async fn handle(
    mut request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let response = if is_ws_upgrade(&request) {
        ws_handshake(&mut request)
    } else {
        let method = request.method().clone();
        let path = request.uri().path().to_string();
        if method == Method::GET && path == oneclient_common::constants::COSMETICS_PROXY_PROBE {
            json_response(
                StatusCode::OK,
                &json!({ "proxy": oneclient_common::constants::COSMETICS_PROXY_MARKER_VALUE }),
            )
        } else if method == Method::GET && path == "/cosmetics" {
            match catalog().await {
                Ok(value) => json_response(StatusCode::OK, &value),
                Err(err) => error_json(StatusCode::BAD_GATEWAY, &err),
            }
        } else if method == Method::GET && path == "/cosmetics/player" {
            handle_player_get(&request).await
        } else if method == Method::PUT && path == "/cosmetics/player" {
            handle_player_put(request).await
        } else if method == Method::POST && path == "/account/login" {
            handle_account_login(request).await
        } else {
            handle_forward(request).await
        }
    };
    Ok(response)
}

pub fn start() {
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        let addr = SocketAddr::from((
            Ipv4Addr::LOCALHOST,
            oneclient_common::constants::COSMETICS_PROXY_PORT,
        ));
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                info!("cosmetics proxy listening on {addr}");
                run(listener).await;
            }
            Err(err) => warn!("cosmetics proxy failed to bind {addr}: {err}"),
        }
    });
}

async fn run(listener: TcpListener) {
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _peer)) => stream,
            Err(err) => {
                debug!("cosmetics proxy accept error: {err}");
                continue;
            }
        };
        tokio::spawn(async move {
            let service = service_fn(handle);
            if let Err(err) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades()
                .await
            {
                debug!("cosmetics proxy connection ended: {err}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn accept_matches_rfc6455_sample() {
        assert_eq!(
            ws_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn splits_emotes_from_catalog_groups() {
        let catalog = json!({
            "cosmetics": [
                {"id": 1, "type": "cape", "name": "Capes", "variants": [{"id": 10, "hash": "h1"}]},
                {"id": 2, "type": "emote", "name": "Emotes", "variants": [
                    {"id": 99, "name": "Wave", "url": "u", "hash": "h2"},
                    {"name": "broken"},
                ]},
            ]
        });
        let (cosmetics, emotes) = split_catalog(&catalog);
        assert_eq!(cosmetics.len(), 1);
        assert_eq!(
            emotes,
            vec![json!({"id": 99, "name": "Wave", "url": "u", "hash": "h2"})]
        );
    }

    #[test]
    fn equipment_is_held_per_account() {
        let state = ProxyState::default();
        let alice = "alice".to_owned();
        let bob = "bob".to_owned();
        state.with_loadout(&alice, |loadout| {
            loadout.equipped.insert("cape".to_owned(), json!(11));
        });
        state.with_loadout(&bob, |loadout| {
            loadout.equipped.insert("cape".to_owned(), json!(22));
        });
        assert_eq!(state.loadout(&alice).equipped["cape"], json!(11));
        assert_eq!(state.loadout(&bob).equipped["cape"], json!(22));

        // Unequipping for one account leaves the other loadout alone.
        state.with_loadout(&bob, |loadout| {
            assert!(loadout.equipped.remove("cape").is_some());
        });
        assert!(state.loadout(&bob).equipped.is_empty());
        assert_eq!(state.loadout(&alice).equipped["cape"], json!(11));
    }

    #[test]
    fn seeding_is_tracked_per_account() {
        let state = ProxyState::default();
        let alice = "alice".to_owned();
        let bob = "bob".to_owned();
        state.with_loadout(&alice, |loadout| loadout.seeded = true);
        assert!(state.seeded(&alice));
        assert!(!state.seeded(&bob), "each account seeds its own locker");
    }

    #[test]
    fn state_written_before_accounts_is_claimed_by_the_first_account() {
        let state = ProxyState::from_json(Some(json!({
            "equipped": {"cape": 7},
            "particle_color": "red",
            "seeded": true,
        })));
        let alice = "alice".to_owned();
        let bob = "bob".to_owned();
        let claimed = state.loadout(&alice);
        assert_eq!(claimed.equipped["cape"], json!(7));
        assert_eq!(claimed.particle_color, Some(json!("red")));
        assert!(claimed.seeded, "the owner keeps the already-seeded flag");
        let fresh = state.loadout(&bob);
        assert!(fresh.equipped.is_empty());
        assert!(!fresh.seeded, "a later account seeds its own locker");
    }

    #[test]
    fn per_account_state_survives_a_disk_round_trip() {
        let state = ProxyState::default();
        let alice = "alice".to_owned();
        let bob = "bob".to_owned();
        state.with_loadout(&alice, |loadout| {
            loadout.equipped.insert("cape".to_owned(), json!(11));
            loadout.particle_color = Some(json!("#fff"));
            loadout.seeded = true;
        });
        state.with_loadout(&bob, |loadout| {
            loadout.equipped.insert("hat".to_owned(), json!(22));
        });

        let restored = ProxyState::from_json(Some(state.to_json()));
        let alice = restored.loadout(&alice);
        assert_eq!(alice.equipped["cape"], json!(11));
        assert_eq!(alice.particle_color, Some(json!("#fff")));
        assert!(alice.seeded);
        let bob = restored.loadout(&bob);
        assert_eq!(bob.equipped["hat"], json!(22));
        assert!(!bob.seeded);
    }

    #[test]
    fn sessions_bind_login_tokens_to_their_account() {
        let mut sessions = Sessions::default();
        let alice = "alice".to_owned();
        sessions.register("token-a", &alice);
        sessions.register("token-b", &"bob".to_owned());
        assert_eq!(sessions.resolve("token-a"), Some(alice));
        assert_eq!(sessions.resolve("token-b"), Some("bob".to_owned()));
        assert_eq!(sessions.resolve("token-c"), None);
    }

    #[test]
    fn login_calls_name_the_account_they_belong_to() {
        assert_eq!(
            account_from_login("/account/login?server_id=probe&username=Alice", b""),
            Some("Alice".to_owned())
        );
        assert_eq!(
            account_from_login(
                "/account/login?player=069a79f4-44e9-4726-a5be-fca90e38aaf5",
                b""
            ),
            Some("069a79f4-44e9-4726-a5be-fca90e38aaf5".to_owned())
        );
        assert_eq!(
            account_from_login("/account/login", br#"{"username":"Bob"}"#),
            Some("Bob".to_owned())
        );
        assert_eq!(account_from_login("/account/login?username=", b""), None);
        assert_eq!(account_from_login("/account/login", b"{}"), None);
    }

    #[test]
    fn locker_requests_read_the_bearer_token() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer_token(&headers), None);
        headers.insert(
            hyper::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-a"),
        );
        assert_eq!(bearer_token(&headers), Some("token-a"));
        headers.insert(
            hyper::header::AUTHORIZATION,
            HeaderValue::from_static("bearer token-b"),
        );
        assert_eq!(bearer_token(&headers), Some("token-b"));
        headers.insert(
            hyper::header::AUTHORIZATION,
            HeaderValue::from_static("   "),
        );
        assert_eq!(bearer_token(&headers), None);

        // The token is what names the account: an unrecognised one shares the
        // account-less bucket instead of inventing an identity.
        STATE.register_session("token-a", &"alice".to_owned());
        headers.insert(
            hyper::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-a"),
        );
        assert_eq!(request_account(&headers), "alice");
        headers.insert(
            hyper::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token-unknown"),
        );
        assert_eq!(request_account(&headers), DEFAULT_ACCOUNT);
    }

    #[test]
    fn assembler_reassembles_fragments_and_passes_control() {
        let mut assembler = FrameAssembler::default();
        assert_eq!(
            assembler.push(Frame {
                fin: false,
                opcode: 0x1,
                payload: b"he".to_vec()
            }),
            Ok(None)
        );
        assert_eq!(
            assembler.push(Frame {
                fin: true,
                opcode: 0x0,
                payload: b"llo".to_vec()
            }),
            Ok(Some((0x1, b"hello".to_vec())))
        );
        assert_eq!(
            assembler.push(Frame {
                fin: true,
                opcode: 0x9,
                payload: b"p".to_vec()
            }),
            Ok(Some((0x9, b"p".to_vec())))
        );
        assert_eq!(
            assembler.push(Frame {
                fin: true,
                opcode: 0x0,
                payload: Vec::new()
            }),
            Err(())
        );
    }

    #[tokio::test]
    async fn frames_roundtrip_both_directions() {
        let (mut server, mut client_side) = tokio::io::duplex(64 * 1024);

        // masked client frame (opcode 1, payload "hi", mask key all zero)
        client_side
            .write_all(&[0x81, 0x82, 0, 0, 0, 0, b'h', b'i'])
            .await
            .unwrap();
        let frame = read_frame(&mut server).await.unwrap().unwrap();
        assert_eq!(
            frame,
            Frame {
                fin: true,
                opcode: 0x1,
                payload: b"hi".to_vec()
            }
        );

        // extended-length server frame (200 bytes)
        let payload = vec![7u8; 200];
        write_frame(&mut server, 0x2, &payload).await.unwrap();
        let frame = read_frame(&mut client_side).await.unwrap().unwrap();
        assert_eq!(frame.opcode, 0x2);
        assert_eq!(frame.payload, payload);

        // clean eof
        drop(client_side);
        assert!(read_frame(&mut server).await.unwrap().is_none());
    }

    #[tokio::test]
    #[ignore = "talks to plus.polyfrost.org"]
    async fn live_player_endpoint_serves_full_catalog() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(run(listener));
        let response = reqwest::get(format!("http://127.0.0.1:{port}/cosmetics/player"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let value: Value = response.json().await.unwrap();
        let cosmetics = value.get("cosmetics").and_then(Value::as_array).unwrap();
        assert!(cosmetics.len() > 5, "expected full catalog, got {value}");
        assert!(value.get("emotes").is_some());
        assert!(value.get("equipped").is_some());
    }

    #[tokio::test]
    async fn websocket_handshake_and_echo_keep_the_connection_alive() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(run(listener));
        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let mut websocket = client
            .get(format!("ws://127.0.0.1:{port}/websocket"))
            .upgrade()
            .send()
            .await
            .unwrap()
            .into_websocket()
            .await
            .unwrap();

        // ping is answered locally in echo mode
        websocket
            .send(Message::Ping(Bytes::from_static(b"hi")))
            .await
            .unwrap();
        let pong = tokio::time::timeout(Duration::from_secs(5), websocket.next())
            .await
            .expect("pong timeout")
            .expect("connection closed")
            .unwrap();
        match pong {
            Message::Pong(payload) => assert_eq!(payload.as_ref(), b"hi"),
            other => panic!("expected pong, got {other:?}"),
        }

        // data frames are drained without breaking the session
        websocket
            .send(Message::Text("{\"packet\":\"SetEquippedCosmetic\"}".into()))
            .await
            .unwrap();
        websocket
            .send(Message::Ping(Bytes::from_static(b"p2")))
            .await
            .unwrap();
        let pong = tokio::time::timeout(Duration::from_secs(5), websocket.next())
            .await
            .expect("second pong timeout")
            .expect("connection closed")
            .unwrap();
        match pong {
            Message::Pong(payload) => assert_eq!(payload.as_ref(), b"p2"),
            other => panic!("expected pong, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "talks to plus.polyfrost.org"]
    async fn live_forward_reaches_upstream() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(run(listener));
        let response = reqwest::get(format!("http://127.0.0.1:{port}/cosmetics?nb=2&page=0"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let value: Value = response.json().await.unwrap();
        assert!(value.get("cosmetics").is_some());
    }

    #[tokio::test]
    #[ignore = "talks to plus.polyfrost.org"]
    async fn live_account_login_falls_back_to_a_local_token_when_upstream_rejects() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(run(listener));
        // Junk credentials make the real backend answer 401 (the offline-account
        // case); the proxy must still hand the mod a usable local token.
        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{port}/account/login?server_id=probe&username=__oneclient_probe__"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let value: Value = response.json().await.unwrap();
        let token = value.get("token").and_then(Value::as_str).unwrap();
        assert!(
            token.starts_with(LOCAL_TOKEN_PREFIX),
            "expected a local token, got {token}"
        );
    }

    #[tokio::test]
    #[ignore = "talks to plus.polyfrost.org"]
    async fn live_equipping_round_trips_and_persists() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(run(listener));
        let base = format!("http://127.0.0.1:{port}/cosmetics/player");

        let put = reqwest::Client::new()
            .put(&base)
            .json(&json!({"equipped": {"cape": 4321}}))
            .send()
            .await
            .unwrap();
        assert_eq!(put.status(), 200);

        let value: Value = reqwest::get(&base).await.unwrap().json().await.unwrap();
        assert_eq!(value["equipped"]["cape"], json!(4321));

        let put = reqwest::Client::new()
            .put(&base)
            .json(&json!({"equipped": {"cape": null}}))
            .send()
            .await
            .unwrap();
        assert_eq!(put.status(), 200);
        let value: Value = reqwest::get(&base).await.unwrap().json().await.unwrap();
        assert!(value["equipped"].get("cape").is_none());
    }

    /// Accepts one connection and then sits on it without ever answering,
    /// mimicking a backend that ACKs the request but stalls before headers.
    async fn stall_upstream() -> SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        addr
    }

    #[tokio::test]
    async fn forward_fails_fast_when_the_upstream_stalls() {
        let addr = stall_upstream().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let started = Instant::now();
        let result = forward_to(
            &client,
            &format!("http://{addr}"),
            &Method::GET,
            "/cosmetics",
            &HeaderMap::new(),
            Bytes::new(),
        )
        .await;
        let elapsed = started.elapsed();
        assert!(
            result.is_err(),
            "a stalled upstream must surface as an error"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "a stalled upstream must not hang: took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn catalog_answers_from_disk_while_the_upstream_stalls() {
        let addr = stall_upstream().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let dir =
            std::env::temp_dir().join(format!("oneclient-catalog-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let disk_path = dir.join("cosmetics_catalog.json");
        let expected = json!({"cosmetics": [{"id": 7}]});
        std::fs::write(&disk_path, serde_json::to_vec(&expected).unwrap()).unwrap();
        let disk = Some(disk_path);
        *STATE.catalog.write().unwrap() = None;
        CATALOG_REFRESHING.store(false, Ordering::Relaxed);

        let started = Instant::now();
        let value = catalog_from(
            &client,
            &format!("http://{addr}"),
            &disk,
            Some(BUNDLED_CATALOG),
        )
        .await
        .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(value, expected, "must serve the on-disk cache");
        assert!(
            elapsed < Duration::from_secs(10),
            "disk cache must be served instantly: took {elapsed:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn catalog_falls_back_to_the_bundled_catalog_when_nothing_is_cached() {
        let addr = stall_upstream().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let disk: Option<PathBuf> = None;
        *STATE.catalog.write().unwrap() = None;
        let base = format!("http://{addr}");

        // Without a fallback tier the failure still surfaces as an error…
        let err = catalog_from(&client, &base, &disk, None)
            .await
            .expect_err("a stalled upstream with no cache must fail");
        assert!(
            err.contains("catalog refresh failed"),
            "unexpected error: {err}"
        );

        // …and with the bundled tier the caller always gets a usable catalog.
        let started = Instant::now();
        let value = catalog_from(&client, &base, &disk, Some(BUNDLED_CATALOG))
            .await
            .expect("the bundled catalog must answer when nothing else can");
        let elapsed = started.elapsed();
        let cosmetics = value
            .get("cosmetics")
            .and_then(Value::as_array)
            .expect("the bundled catalog must keep the cosmetics shape");
        assert!(cosmetics.is_empty(), "expected an empty catalog: {value}");
        assert!(
            elapsed < Duration::from_secs(10),
            "the bundled fallback must not wait on the upstream: took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn upstream_ws_handshake_times_out_against_a_silent_server() {
        let addr = stall_upstream().await;
        let started = Instant::now();
        let result = upstream_ws_connect_to(
            &format!("http://{addr}"),
            "/websocket",
            hyper::header::HeaderValue::from_static("Bearer probe"),
            Duration::from_millis(300),
        )
        .await;
        let elapsed = started.elapsed();
        assert!(
            result.is_err(),
            "a silent peer must not complete the handshake"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "the handshake must be bounded: took {elapsed:?}"
        );
    }

    async fn body_of(response: Response<Full<Bytes>>) -> Bytes {
        response.into_body().collect().await.unwrap().to_bytes()
    }

    fn etag_of(response: &Response<Full<Bytes>>) -> Option<&str> {
        response
            .headers()
            .get(hyper::header::ETAG)
            .and_then(|value| value.to_str().ok())
    }

    /// Pushes an entry past its deadline so the next request has to revalidate.
    fn expire_cached_get(key: &str) {
        let mut cache = FORWARD_CACHE.write().unwrap();
        let entry = cache.get_mut(key).expect("expected a cached GET");
        entry.fresh_until = Instant::now();
    }

    /// Answers `200` with a fixed body (and its ETag), or `304` when the
    /// request already carries that validator, counting both round-trips.
    async fn etag_upstream(hits: Arc<AtomicUsize>, revalidated: Arc<AtomicUsize>) -> SocketAddr {
        const ETAG: &str = "\"upstream-v1\"";
        const BODY: &str = "{\"a\":1}";
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let hits = hits.clone();
                let revalidated = revalidated.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        match stream.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => request.extend_from_slice(&chunk[..read]),
                        }
                    }
                    let head = String::from_utf8_lossy(&request).to_ascii_lowercase();
                    hits.fetch_add(1, Ordering::Relaxed);
                    let response = if head.contains(&format!("if-none-match: {ETAG}")) {
                        revalidated.fetch_add(1, Ordering::Relaxed);
                        format!(
                            "HTTP/1.1 304 Not Modified\r\netag: {ETAG}\r\nconnection: close\r\n\r\n"
                        )
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\netag: {ETAG}\r\n\
                             content-length: {}\r\nconnection: close\r\n\r\n{BODY}",
                            BODY.len()
                        )
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn forwarded_get_replays_from_the_cache_and_revalidates_with_etag() {
        let path = "/store?page=0";
        FORWARD_CACHE.write().unwrap().remove(path);
        let hits = Arc::new(AtomicUsize::new(0));
        let revalidated = Arc::new(AtomicUsize::new(0));
        let addr = etag_upstream(hits.clone(), revalidated.clone()).await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let base = format!("http://{addr}");

        let first = forward_get_from(&client, &base, path, &HeaderMap::new(), Bytes::new()).await;
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(etag_of(&first), Some("\"upstream-v1\""));
        assert_eq!(body_of(first).await, Bytes::from_static(b"{\"a\":1}"));
        assert_eq!(hits.load(Ordering::Relaxed), 1);

        // A fresh entry is replayed without another round-trip…
        let second = forward_get_from(&client, &base, path, &HeaderMap::new(), Bytes::new()).await;
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(body_of(second).await, Bytes::from_static(b"{\"a\":1}"));
        assert_eq!(
            hits.load(Ordering::Relaxed),
            1,
            "a fresh entry must be replayed from memory"
        );

        // …and a caller still holding the validator gets a bodiless `304`.
        let mut conditional = HeaderMap::new();
        conditional.insert(
            hyper::header::IF_NONE_MATCH,
            HeaderValue::from_static("\"upstream-v1\""),
        );
        let conditional_response =
            forward_get_from(&client, &base, path, &conditional, Bytes::new()).await;
        assert_eq!(conditional_response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(etag_of(&conditional_response), Some("\"upstream-v1\""));
        assert_eq!(
            hits.load(Ordering::Relaxed),
            1,
            "a matching If-None-Match must not reach the upstream"
        );

        // Past the TTL the proxy revalidates with the validator it handed out…
        expire_cached_get(path);
        let refreshed =
            forward_get_from(&client, &base, path, &HeaderMap::new(), Bytes::new()).await;
        assert_eq!(refreshed.status(), StatusCode::OK);
        assert_eq!(body_of(refreshed).await, Bytes::from_static(b"{\"a\":1}"));
        assert_eq!(hits.load(Ordering::Relaxed), 2);
        assert_eq!(
            revalidated.load(Ordering::Relaxed),
            1,
            "a stale entry must be revalidated with If-None-Match"
        );
        assert!(
            cached_get(path).unwrap().fresh_until > Instant::now(),
            "a successful revalidation must restart the TTL"
        );

        // …so the caller's own `304` costs one more conditional round-trip.
        expire_cached_get(path);
        let conditional_response =
            forward_get_from(&client, &base, path, &conditional, Bytes::new()).await;
        assert_eq!(conditional_response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(etag_of(&conditional_response), Some("\"upstream-v1\""));
        assert_eq!(hits.load(Ordering::Relaxed), 3);
        assert_eq!(revalidated.load(Ordering::Relaxed), 2);

        FORWARD_CACHE.write().unwrap().remove(path);
    }

    #[tokio::test]
    async fn a_stale_entry_answers_while_the_upstream_is_unreachable() {
        let path = "/store?page=1";
        let fresh_until = Instant::now(); // already past by the time we ask
        let mut headers = HeaderMap::new();
        headers.append(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        FORWARD_CACHE.write().unwrap().insert(
            path.to_owned(),
            CachedGet {
                fresh_until,
                etag: "\"cached-v1\"".to_owned(),
                headers,
                body: Bytes::from_static(b"{\"cached\":true}"),
            },
        );
        let addr = stall_upstream().await;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let base = format!("http://{addr}");

        let response =
            forward_get_from(&client, &base, path, &HeaderMap::new(), Bytes::new()).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a dead upstream must not turn a cached GET into a 502"
        );
        assert_eq!(etag_of(&response), Some("\"cached-v1\""));
        assert_eq!(
            body_of(response).await,
            Bytes::from_static(b"{\"cached\":true}")
        );
        assert_eq!(
            cached_get(path).unwrap().fresh_until,
            fresh_until,
            "a failed refresh must not re-stamp the TTL"
        );

        // The caller's own validator is still honoured from the stale copy.
        let mut conditional = HeaderMap::new();
        conditional.insert(
            hyper::header::IF_NONE_MATCH,
            HeaderValue::from_static("\"cached-v1\""),
        );
        let response = forward_get_from(&client, &base, path, &conditional, Bytes::new()).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(etag_of(&response), Some("\"cached-v1\""));

        FORWARD_CACHE.write().unwrap().remove(path);
    }

    #[test]
    fn if_none_match_matches_star_lists_and_weak_validators() {
        assert!(if_none_match_hits("*", "\"a\""));
        assert!(if_none_match_hits("\"a\"", "\"a\""));
        assert!(if_none_match_hits("W/\"a\"", "\"a\""));
        assert!(if_none_match_hits("\"b\", \"a\"", "\"a\""));
        assert!(if_none_match_hits(" \"a\" ", "\"a\""));
        assert!(!if_none_match_hits("\"b\"", "\"a\""));
        assert!(!if_none_match_hits("", "\"a\""));
    }

    #[test]
    fn etag_prefers_the_upstream_validator_and_hashes_the_body() {
        let mut headers = HeaderMap::new();
        headers.insert(
            hyper::header::ETAG,
            HeaderValue::from_static("\"upstream-v1\""),
        );
        assert_eq!(forward_etag(&headers, b"body"), "\"upstream-v1\"");

        let empty = HeaderMap::new();
        let etag = forward_etag(&empty, b"body");
        assert!(etag.starts_with('"') && etag.ends_with('"'));
        assert_eq!(etag, forward_etag(&empty, b"body"), "same body, same ETag");
        assert_ne!(
            etag,
            forward_etag(&empty, b"other"),
            "the body feeds the hash"
        );
    }
}
