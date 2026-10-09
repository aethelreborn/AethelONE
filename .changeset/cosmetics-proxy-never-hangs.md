---
default: patch
---

Bound all upstream requests made by the local cosmetics proxy (15s total per request, 15s websocket handshake) so a stalled backend can no longer hang the game's cosmetics screen, and serve the cached cosmetic catalog from disk instantly while revalidating it in the background.
