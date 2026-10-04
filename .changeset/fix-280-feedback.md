---
default: patch
---

# Fix the 2.8.0 release feedback: Debian upgrade collision, updater 401/403 retries, cosmetics-unlock diagnostics

- The Debian package keeps the stable name `one-client` again and declares the
  short-lived `aethel-one` rename (`Conflicts`/`Replaces`), so upgrading from
  2.7.x no longer dies on `/usr/bin/oneclient_app` being claimed by both
  packages.
- The in-app updater retries transient 401/403 failures while downloading a
  release asset (and its signature) instead of aborting, and the signature
  request now has a timeout.
- The cosmetics unlock probes the loopback proxy for its marker before
  injecting `-Dpolyplus.apiUrl` (a foreign process on the port can no longer
  swallow Poly+ silently), and every step logs at info level: proxy bind,
  launch-time injection, locker served, equipment stored, websocket relay.
