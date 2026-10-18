-- Optional user-defined group (folder) for organising instances on the
-- Versions page. NULL or empty means ungrouped.
ALTER TABLE clusters ADD COLUMN group_name TEXT;
