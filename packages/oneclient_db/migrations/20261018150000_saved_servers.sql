-- Servers the player saved from the servers page. The address is the only
-- natural key so re-adding an existing one just updates its label.
CREATE TABLE saved_servers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    address TEXT NOT NULL UNIQUE,
    label TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_joined_at TEXT
);
