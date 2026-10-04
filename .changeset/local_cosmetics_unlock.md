---
default: minor
---

# AethelONE

Poly+ cosmetics are now unlocked locally, with no scripts: the launcher runs a
loopback proxy on startup, injects `-Dpolyplus.apiUrl` at launch when the new
"All Cosmetics Unlocked" setting is on (default), keeps your equipment under
`~/.aethelone/`, and relays every other Poly+ call to the real backend
unchanged. Works offline; other players always see official ownership.
