CREATE TABLE rooms (
  matrix_room_id VARCHAR NOT NULL,
  display_name VARCHAR NOT NULL,
  rocketchat_room_id VARCHAR,
  is_admin_room BOOLEAN NOT NULL DEFAULT false,
  is_bridged BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT rooms_pk PRIMARY KEY (matrix_room_id)
)
