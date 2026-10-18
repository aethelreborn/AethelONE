-- The enabled mods a session launched with, so the next crash can say what
-- changed since the last run instead of describing a stack trace
ALTER TABLE game_sessions ADD COLUMN mods_snapshot TEXT;
