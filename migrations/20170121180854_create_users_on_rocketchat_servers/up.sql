CREATE TABLE users_on_rocketchat_servers (
  matrix_user_id VARCHAR NOT NULL,
  rocketchat_server_id INTEGER NOT NULL,
  rocketchat_auth_token VARCHAR,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
)
