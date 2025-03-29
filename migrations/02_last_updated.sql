CREATE TABLE mods_updated_date (
  id    INTEGER PRIMARY KEY NOT NULL DEFAULT(0) CHECK (id = 0),
  date  DATE NOT NULL
);
