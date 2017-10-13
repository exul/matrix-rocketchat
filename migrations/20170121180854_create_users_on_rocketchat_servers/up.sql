CREATE TABLE users_on_rocketchat_servers (
  is_virtual_user BOOLEAN NOT NULL,
  last_message_sent BIG INT NOT NULL DEFAULT 0,
  matrix_user_id VARCHAR NOT NULL,
  rocketchat_server_id VARCHAR NOT NULL,
  rocketchat_user_id VARCHAR,
  rocketchat_auth_token VARCHAR,
  rocketchat_username VARCHAR,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
)
