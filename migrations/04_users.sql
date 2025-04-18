CREATE TABLE users (
  id             SERIAL PRIMARY KEY NOT NULL,
  username       TEXT UNIQUE NOT NULL,
  password_hash  TEXT NOT NULL
);

ALTER TABLE ratings RENAME TO legacy_ratings;

CREATE TABLE ratings (
  mod_id   UUID NOT NULL,
  rating   rating_type NOT NULL,
  user_id  INTEGER NOT NULL,

  PRIMARY KEY (mod_id, user_id),
  FOREIGN KEY (mod_id) REFERENCES mods(id),
  FOREIGN KEY (user_id) REFERENCES users(id)
);
