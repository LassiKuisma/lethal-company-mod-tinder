CREATE TABLE categories (
  id    SERIAL PRIMARY KEY NOT NULL,
  name  TEXT UNIQUE NOT NULL
);

CREATE TABLE mods (
  id            UUID PRIMARY KEY NOT NULL,
  name          TEXT NOT NULL,
  description   TEXT NOT NULL,
  icon_url      TEXT NOT NULL,
  full_name     TEXT NOT NULL,
  owner         TEXT NOT NULL,
  package_url   TEXT NOT NULL,
  updated_date  DATE NOT NULL,
  rating        INTEGER NOT NULL,
  deprecated    BOOLEAN NOT NULL,
  nsfw          BOOLEAN NOT NULL
);

CREATE TABLE mod_category (
  mod_id       UUID NOT NULL,
  category_id  INTEGER NOT NULL,

  PRIMARY KEY(mod_id, category_id),
  FOREIGN KEY(mod_id)      REFERENCES mods(id),
  FOREIGN KEY(category_id) REFERENCES categories(id)
);
