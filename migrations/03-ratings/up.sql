CREATE TABLE RatingType (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE
);

INSERT INTO RatingType (id, name) VALUES
  (0, 'Dislike'),
  (1, 'Like');

CREATE TABLE Ratings (
  modId     UUID PRIMARY KEY,
  ratingId  INTEGER,

  FOREIGN KEY (modId)    REFERENCES Mods(id),
  FOREIGN KEY (ratingId) REFERENCES RatingType(id)
);
