//! Built-in local Poly+ cosmetics unlock proxy.
//!
//! Listens on loopback (`COSMETICS_PROXY_PORT`) and mirrors
//! `https://plus.polyfrost.org` for the Poly+ mod, which the launcher points
//! at it with `-Dpolyplus.apiUrl`:
//!
//! * `GET  /cosmetics/player` — every catalog item reads as owned; equipment
//!   choices are persisted under `~/.aethelone/` (seeded once from the
//!   account's official locker when one exists).
//! * `PUT  /cosmetics/player` — writes that local equipment state.
//! * `POST /account/login` — tried against the real backend first; when the
//!   backend rejects the session (offline accounts have no Mojang session),
//!   a local bearer token is minted so Poly+ proceeds and the locker below
//!   can load. Real Microsoft sessions pass through untouched.
//! * everything else — forwarded to the real backend unchanged, including the
//!   caller's Authorization header, so logins, store and socials keep working.
//! * `GET  /websocket` — relays to the real backend when the client presents a
//!   working upstream token (so other players still see your official
//!   unlocks); local-session and tokenless clients are answered locally
//!   (pings/pongs) so the mod's equipment sync stays healthy. Textures are
//!   never proxied: they come straight from the public CDN.
//!
//! Nothing is uploaded, mirrored or shared; other players always see their
//! official ownership. The whole feature is local-only by design.

use std::collections::BTreeMap;
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
    HeaderMap, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE,
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

#[derive(Default)]
struct ProxyState {
    catalog: RwLock<Option<(Instant, Value)>>,
    equipped: RwLock<BTreeMap<String, Value>>,
    particle_color: RwLock<Option<Value>>,
    seeded: AtomicBool,
}

impl ProxyState {
    fn load() -> Self {
        let state = Self::default();
        if let Some(value) = read_json(&state_file()) {
            if let Some(object) = value.get("equipped").and_then(Value::as_object) {
                *state.equipped.write().unwrap() = object.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            }
            *state.particle_color.write().unwrap() = value.get("particle_color").cloned();
            state.seeded.store(
                value.get("seeded").and_then(Value::as_bool).unwrap_or(false),
                Ordering::Relaxed,
            );
        }
        state
    }

    fn persist(&self) {
        let value = json!({
            "equipped": Value::Object(self.equipped.read().unwrap().clone().into_iter().collect()),
            "particle_color": self.particle_color.read().unwrap().clone(),
            "seeded": self.seeded.load(Ordering::Relaxed),
        });
        write_json_atomic(&state_file(), &value);
    }
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
    let method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|err| err.to_string())?;
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
    request.send().await.map_err(|err| format!("upstream: {err}"))
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

async fn catalog() -> Result<Value, String> {
    catalog_from(&CLIENT, UPSTREAM, &catalog_file()).await
}

async fn catalog_from(
    client: &reqwest::Client,
    base: &str,
    disk: &Option<PathBuf>,
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
    refresh_catalog(client, base, disk).await
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
    match forward_to(client, base, &Method::GET, "/cosmetics", &HeaderMap::new(), Bytes::new())
        .await
    {
        Ok(response) if response.status() == 200 => match response.bytes().await {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) => {
                    if let Some(path) = disk {
                        let _ = std::fs::write(path, &bytes);
                    }
                    *STATE.catalog.write().unwrap() = Some((Instant::now(), value.clone()));
                    Ok(value)
                }
                Err(err) => Err(format!("catalog decode failed: {err}")),
            },
            Err(err) => Err(format!("catalog read failed: {err}")),
        },
        Ok(response) => Err(format!("catalog refresh failed: {}", response.status())),
        Err(err) => {
            if let Some(value) = read_json(disk) {
                debug!("cosmetics proxy: using on-disk catalog cache ({err})");
                *STATE.catalog.write().unwrap() = Some((Instant::now(), value.clone()));
                Ok(value)
            } else {
                Err(format!("catalog refresh failed: {err}"))
            }
        }
    }
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

async fn seed_from_upstream(headers: &HeaderMap) {
    let result = async {
        let response = forward_request(&Method::GET, "/cosmetics/player", headers, Bytes::new()).await?;
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
            if let Some(object) = official.get("equipped").and_then(Value::as_object) {
                let mut equipped = STATE.equipped.write().unwrap();
                equipped.clear();
                for (slot, id) in object {
                    if !id.is_null() {
                        equipped.insert(slot.clone(), id.clone());
                    }
                }
            }
            let color = official.get("particle_color").cloned();
            if color.as_ref().is_some_and(|value| !value.is_null()) {
                *STATE.particle_color.write().unwrap() = color;
            }
            info!("cosmetics proxy: seeded from the official locker");
        }
        Err(err) => debug!("cosmetics proxy: official seed skipped ({err}); starting empty"),
    }
}

async fn handle_player_get(request: &Request<Incoming>) -> Response<Full<Bytes>> {
    if !STATE.seeded.load(Ordering::Relaxed) {
        let _guard = SEED_LOCK.lock().await;
        if !STATE.seeded.load(Ordering::Relaxed) {
            seed_from_upstream(request.headers()).await;
            STATE.seeded.store(true, Ordering::Relaxed);
            STATE.persist();
        }
    }
    let catalog = match catalog().await {
        Ok(catalog) => catalog,
        Err(err) => return error_json(StatusCode::BAD_GATEWAY, &err),
    };
    let (cosmetics, emotes) = split_catalog(&catalog);
    let equipped = STATE.equipped.read().unwrap().clone();
    info!(
        groups = cosmetics.len(),
        emotes = emotes.len(),
        "cosmetics proxy: served the local player locker (every item owned)"
    );
    json_response(
        StatusCode::OK,
        &json!({
            "cosmetics": cosmetics,
            "emotes": emotes,
            "equipped": Value::Object(equipped.into_iter().collect()),
            "particle_color": STATE.particle_color.read().unwrap().clone(),
        }),
    )
}

async fn handle_player_put(request: Request<Incoming>) -> Response<Full<Bytes>> {
    let body = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "bad body"),
    };
    let patch: Value = match serde_json::from_slice(&body) {
        Ok(patch) => patch,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "bad json"),
    };
    if let Some(object) = patch.get("equipped").and_then(Value::as_object) {
        let mut equipped = STATE.equipped.write().unwrap();
        for (slot, id) in object {
            if id.is_null() {
                equipped.remove(slot);
            } else {
                equipped.insert(slot.clone(), id.clone());
            }
        }
    }
    if let Some(color) = patch.get("particle_color")
        && !color.is_null()
    {
        *STATE.particle_color.write().unwrap() = Some(color.clone());
    }
    STATE.persist();
    let equipped = STATE.equipped.read().unwrap().clone();
    info!(
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
    match forward_request(method, path_and_query, headers, body).await {
        Ok(response) => {
            let status =
                StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let headers = response.headers().clone();
            let body = response.bytes().await.unwrap_or_default();
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
        Err(err) => error_json(StatusCode::BAD_GATEWAY, &err),
    }
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
    let response = build_forward_response(&Method::POST, &path_and_query, &headers, body).await;
    let status = response.status();
    if status.is_success() {
        return response;
    }
    if status != StatusCode::UNAUTHORIZED
        && status != StatusCode::FORBIDDEN
        && status != StatusCode::BAD_GATEWAY
        && !status.is_server_error()
    {
        return response;
    }
    let token = format!("{LOCAL_TOKEN_PREFIX}{}", uuid::Uuid::new_v4());
    info!(
        status = %status,
        "cosmetics proxy: real backend rejected the session; issuing a local token instead"
    );
    json_response(StatusCode::OK, &json!({ "token": token }))
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
    let auth = request
        .headers()
        .get(hyper::header::AUTHORIZATION)
        .cloned();
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
        response.into_websocket().await.map_err(|err| err.to_string())
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

async fn relay_session(
    mut client: TokioIo<Upgraded>,
    mut websocket: reqwest_websocket::WebSocket,
) {
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
                let Some(opcode) = self.opcode.take() else { return Err(()) };
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
        if method == Method::GET
            && path == oneclient_common::constants::COSMETICS_PROXY_PROBE
        {
            json_response(
                StatusCode::OK,
                &json!({ "proxy": oneclient_common::constants::COSMETICS_PROXY_MARKER_VALUE }),
            )
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
        assert_eq!(emotes, vec![json!({"id": 99, "name": "Wave", "url": "u", "hash": "h2"})]);
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
        assert!(result.is_err(), "a stalled upstream must surface as an error");
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
        let value = catalog_from(&client, &format!("http://{addr}"), &disk)
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
        assert!(result.is_err(), "a silent peer must not complete the handshake");
        assert!(
            elapsed < Duration::from_secs(10),
            "the handshake must be bounded: took {elapsed:?}"
        );
    }
}
