CREATE TABLE rocketchat_servers (
  id VARCHAR NOT NULL,
  rocketchat_url VARCHAR NOT NULL,
  rocketchat_token VARCHAR,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT rocketchat_servers_pk PRIMARY KEY (id)
  UNIQUE (rocketchat_url),
  UNIQUE (rocketchat_token)
)
