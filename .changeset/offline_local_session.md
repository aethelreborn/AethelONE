---
default: patch
---

# Offline accounts can now finish Poly+ login and open the unlocked locker

PolyPlus authorizes against the real backend before it shows anything, and
plus.polyfrost.org rejects offline accounts (no Mojang sessionserver proof)
with 401 — so the cosmetics screen never opened and the game spammed "Lost
connection to PolyPlus". The local proxy now tries `POST /account/login`
upstream first and, only when the session is rejected (401/403/5xx/network),
mints a local bearer token so the mod proceeds; the websocket answers such
local sessions directly instead of relaying the refused token upstream.
Real Microsoft sessions keep passing through untouched.
