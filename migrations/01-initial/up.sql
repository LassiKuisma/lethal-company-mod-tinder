CREATE TABLE Categories (
  id    INTEGER PRIMARY KEY AUTOINCREMENT,
  name  TEXT UNIQUE NOT NULL
);

CREATE TABLE Mods (
  id           UUID PRIMARY KEY NOT NULL,
  name         TEXT NOT NULL,
  description  TEXT NOT NULL,
  iconUrl      TEXT NOT NULL,
  fullName     TEXT NOT NULL,
  owner        TEXT NOT NULL,
  packageUrl   TEXT NOT NULL,
  updatedDate  DATE NOT NULL,
  rating       INTEGER NOT NULL,
  deprecated   BOOLEAN NOT NULL,
  nsfw         BOOLEAN NOT NULL
);

CREATE TABLE ModCategory (
  modId       UUID,
  categoryId  INTEGER,

  PRIMARY KEY(modId, categoryId),
  FOREIGN KEY(modId) REFERENCES Mods(id),
  FOREIGN KEY(categoryId) REFERENCES Categories(id)
);
