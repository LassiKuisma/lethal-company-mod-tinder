CREATE TABLE rating_type (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE
);

INSERT INTO rating_type (id, name) VALUES
  (0, 'Dislike'),
  (1, 'Like');

CREATE TABLE ratings (
  mod_id     UUID PRIMARY KEY,
  rating_id  INTEGER,

  FOREIGN KEY (mod_id)    REFERENCES mods(id),
  FOREIGN KEY (rating_id) REFERENCES rating_type(id)
);
