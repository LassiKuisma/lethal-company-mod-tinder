CREATE TYPE rating_type AS ENUM ('Like', 'Dislike');

CREATE TABLE ratings (
  mod_id  UUID PRIMARY KEY NOT NULL,
  rating  rating_type NOT NULL,

  FOREIGN KEY (mod_id) REFERENCES mods(id)
);
