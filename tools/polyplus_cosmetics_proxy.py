#!/usr/bin/env python3
"""AethelONE local Poly+ cosmetics proxy (personal use, local only).

Relays every PolyPlus API call to the real backend (plus.polyfrost.org)
unchanged, except GET/PUT /cosmetics/player which are answered locally so
the whole catalog reads as owned and your equipment choices stay on this
machine. Nothing is uploaded, mirrored, or shared.

Usage:
    python3 tools/polyplus_cosmetics_proxy.py [--port 8777]

Then in AethelONE: cluster settings -> JVM Arguments, add:
    -Dpolyplus.apiUrl=http://127.0.0.1:8777

Stop the proxy before launching without the flag, otherwise Poly+ online
features (socials, cosmetics) have nowhere to connect.
"""

from __future__ import annotations

import http.client
import json
import ssl
import socket
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

UPSTREAM_HOST = "plus.polyfrost.org"
UPSTREAM_PORT = 443
LISTEN_HOST = "127.0.0.1"
DEFAULT_PORT = 8777
CATALOG_TTL = 600.0
STATE_DIR = Path.home() / ".aethelone"
STATE_FILE = STATE_DIR / "cosmetics_state.json"
CATALOG_FILE = STATE_DIR / "cosmetics_catalog.json"

HOP_BY_HOP = {
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
}

_catalog_lock = threading.Lock()
_catalog_cache: tuple[float, dict] | None = None
_state_lock = threading.Lock()


def log(msg: str) -> None:
    print(f"[cosmetics-proxy] {msg}", flush=True)


# ---------------------------------------------------------------- state


def load_state() -> dict:
    try:
        return json.loads(STATE_FILE.read_text())
    except (OSError, ValueError):
        return {}


def save_state(state: dict) -> None:
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    tmp = STATE_FILE.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, indent=2))
    tmp.replace(STATE_FILE)


# ---------------------------------------------------------------- catalog


def upstream_request(
    method: str,
    path: str,
    headers: dict[str, str],
    body: bytes | None = None,
    timeout: float = 30.0,
) -> tuple[int, list[tuple[str, str]], bytes]:
    conn = http.client.HTTPSConnection(UPSTREAM_HOST, UPSTREAM_PORT, timeout=timeout)
    try:
        conn.request(method, path, body=body, headers=headers)
        resp = conn.getresponse()
        data = resp.read()
        resp_headers = [
            (k, v)
            for k, v in resp.getheaders()
            if k.lower() not in HOP_BY_HOP and k.lower() != "content-length"
        ]
        return resp.status, resp_headers, data
    finally:
        conn.close()


def get_catalog() -> dict:
    """Full public catalog, disk-cached, refreshed at most every 10 min."""
    global _catalog_cache
    with _catalog_lock:
        now = time.monotonic()
        if _catalog_cache and now - _catalog_cache[0] < CATALOG_TTL:
            return _catalog_cache[1]
        try:
            status, _, data = upstream_request("GET", "/cosmetics", {"Host": UPSTREAM_HOST})
            if status == 200:
                catalog = json.loads(data)
                STATE_DIR.mkdir(parents=True, exist_ok=True)
                CATALOG_FILE.write_bytes(data)
                _catalog_cache = (now, catalog)
                return catalog
        except (OSError, ValueError, http.client.HTTPException) as err:
            log(f"catalog refresh failed: {err}")
        try:
            catalog = json.loads(CATALOG_FILE.read_text())
            _catalog_cache = (now, catalog)
            log("using on-disk catalog cache")
            return catalog
        except (OSError, ValueError):
            raise RuntimeError("cosmetic catalog unavailable")


def split_catalog(catalog: dict) -> tuple[list, list]:
    groups = catalog.get("cosmetics", [])
    cosmetics = [g for g in groups if g.get("type") != "emote"]
    emotes = [
        {"id": v["id"], "name": v.get("name", "Emote"), "url": v.get("url"), "hash": v["hash"]}
        for g in groups
        if g.get("type") == "emote"
        for v in g.get("variants", [])
    ]
    return cosmetics, emotes


def seed_from_upstream(auth_headers: dict[str, str]) -> dict:
    """First run: capture the account's real equipment/color as a starting point."""
    try:
        status, _, data = upstream_request(
            "GET", "/cosmetics/player", auth_headers, timeout=15.0
        )
        if status == 200:
            official = json.loads(data)
            return {
                "equipped": {k: v for k, v in official.get("equipped", {}).items() if v is not None},
                "particle_color": official.get("particle_color"),
                "seeded": True,
            }
    except (OSError, ValueError, http.client.HTTPException) as err:
        log(f"official seed failed ({err}); starting empty")
    return {"equipped": {}, "particle_color": None, "seeded": True}


# ---------------------------------------------------------------- handler


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "AethelONECosmeticsProxy/1.0"

    def log_message(self, *_args) -> None:  # quiet per-request access log
        pass

    # -- helpers ----------------------------------------------------

    def read_client_body(self) -> bytes:
        if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
            chunks = []
            while True:
                size_line = self.rfile.readline().strip()
                if b";" in size_line:
                    size_line = size_line.split(b";", 1)[0]
                size = int(size_line, 16)
                if size == 0:
                    self.rfile.readline()
                    break
                chunks.append(self.rfile.read(size))
                self.rfile.readline()
            return b"".join(chunks)
        length = int(self.headers.get("Content-Length", "0"))
        return self.rfile.read(length) if length else b""

    def filtered_request_headers(self) -> dict[str, str]:
        return {
            k: v
            for k, v in self.headers.items()
            if k.lower() not in HOP_BY_HOP
        }

    def send_json(self, payload: dict, status: int = 200) -> None:
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    # -- overrides --------------------------------------------------

    def handle_player_get(self) -> None:
        auth = self.filtered_request_headers()
        try:
            state = load_state()
            if not state.get("seeded"):
                with _state_lock:
                    state = load_state()
                    if not state.get("seeded"):
                        state = seed_from_upstream(auth)
                        save_state(state)
            cosmetics, emotes = split_catalog(get_catalog())
        except RuntimeError as err:
            self.send_json({"error": str(err)}, 502)
            return
        equipped = {k: v for k, v in state.get("equipped", {}).items() if v is not None}
        self.send_json(
            {
                "cosmetics": cosmetics,
                "emotes": emotes,
                "equipped": equipped,
                "particle_color": state.get("particle_color"),
            }
        )

    def handle_player_put(self) -> None:
        body = self.read_client_body()
        try:
            patch = json.loads(body or b"{}")
        except ValueError:
            self.send_json({"error": "bad json"}, 400)
            return
        with _state_lock:
            state = load_state()
            equipped = state.get("equipped", {})
            for slot, cosmetic_id in patch.get("equipped", {}).items():
                if cosmetic_id is None:
                    equipped.pop(slot, None)
                else:
                    equipped[slot] = cosmetic_id
            state["equipped"] = equipped
            save_state(state)
        self.send_json({"equipped": equipped})

    # -- generic forward --------------------------------------------

    def forward(self) -> None:
        method = self.command
        path = self.path
        body = self.read_client_body() if method in ("POST", "PUT", "PATCH", "DELETE") else None
        headers = self.filtered_request_headers()
        try:
            status, resp_headers, data = upstream_request(method, path, headers, body)
        except (OSError, http.client.HTTPException) as err:
            self.send_json({"error": f"upstream: {err}"}, 502)
            return
        self.send_response(status)
        for key, value in resp_headers:
            self.send_header(key, value)
        if status not in (204, 304):
            self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if method != "HEAD" and data:
            self.wfile.write(data)

    # -- websocket --------------------------------------------------

    def relay_websocket(self) -> None:
        path = self.path
        try:
            upstream = socket.create_connection((UPSTREAM_HOST, UPSTREAM_PORT), timeout=30)
            upstream = ssl.create_default_context().wrap_socket(upstream, server_hostname=UPSTREAM_HOST)
        except OSError as err:
            self.send_json({"error": f"upstream ws: {err}"}, 502)
            return

        lines = [f"{self.command} {path} HTTP/1.1"]
        for key, value in self.headers.items():
            if key.lower() in ("host", "content-length", "content-type"):
                continue
            lines.append(f"{key}: {value}")
        lines.append(f"Host: {UPSTREAM_HOST}")
        lines.append("")
        lines.append("")
        upstream.sendall("\r\n".join(lines).encode())

        downstream = self.connection
        upstream_file = upstream.makefile("rb")

        try:
            head = b""
            while b"\r\n\r\n" not in head:
                chunk = upstream_file.read(1)
                if not chunk:
                    raise ConnectionError("upstream closed during handshake")
                head += chunk
            head, remainder = head.split(b"\r\n\r\n", 1)
            downstream.sendall(head + b"\r\n\r\n" + remainder)
        except (OSError, ConnectionError) as err:
            log(f"ws handshake relay failed: {err}")
            upstream.close()
            return

        def pump(src: socket.socket, dst: socket.socket) -> None:
            try:
                while True:
                    data = src.recv(65536)
                    if not data:
                        break
                    dst.sendall(data)
            except OSError:
                pass
            finally:
                for sock in (src, dst):
                    try:
                        sock.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass

        t = threading.Thread(target=pump, args=(upstream, downstream), daemon=True)
        t.start()
        try:
            pump(downstream, upstream)
        finally:
            t.join(timeout=5)
            upstream.close()

    # -- dispatch ---------------------------------------------------

    def is_websocket(self) -> bool:
        return (
            "upgrade" in self.headers.get("Connection", "").lower()
            and self.headers.get("Upgrade", "").lower() == "websocket"
        )

    def dispatch(self) -> None:
        try:
            if self.is_websocket():
                self.relay_websocket()
                return
            path = urlsplit(self.path).path
            if path == "/cosmetics/player":
                if self.command == "GET":
                    self.handle_player_get()
                    return
                if self.command == "PUT":
                    self.handle_player_put()
                    return
            self.forward()
        except BrokenPipeError:
            pass
        except Exception as err:  # noqa: BLE001 - never take the game client down silently
            log(f"error handling {self.command} {self.path}: {err!r}")
            try:
                self.send_json({"error": str(err)}, 500)
            except OSError:
                pass

    def do_GET(self) -> None:
        self.dispatch()

    def do_PUT(self) -> None:
        self.dispatch()

    def do_POST(self) -> None:
        self.dispatch()

    def do_PATCH(self) -> None:
        self.dispatch()

    def do_DELETE(self) -> None:
        self.dispatch()

    def do_HEAD(self) -> None:
        self.dispatch()


def main() -> int:
    port = DEFAULT_PORT
    args = sys.argv[1:]
    if "--port" in args:
        port = int(args[args.index("--port") + 1])
    server = ThreadingHTTPServer((LISTEN_HOST, port), Handler)
    server.daemon_threads = True
    log(f"listening on http://{LISTEN_HOST}:{port} -> https://{UPSTREAM_HOST}")
    log(f"add to JVM Arguments: -Dpolyplus.apiUrl=http://{LISTEN_HOST}:{port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        log("stopped")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
