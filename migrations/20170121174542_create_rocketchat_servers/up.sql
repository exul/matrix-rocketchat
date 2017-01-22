CREATE TABLE rocketchat_servers (
  id INTEGER PRIMARY KEY,
  rocketchat_url VARCHAR NOT NULL,
  rocketchat_token VARCHAR,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (rocketchat_url),
  UNIQUE (rocketchat_token)
)
