CREATE TABLE users_in_rooms(
  matrix_user_id VARCHAR NOT NULL,
  matrix_room_id VARCHAR NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(matrix_user_id) REFERENCES users(matrix_user_id),
  FOREIGN KEY(matrix_room_id) REFERENCES rooms(matrix_room_id),
  CONSTRAINT users_in_rooms_pk PRIMARY KEY (matrix_user_id, matrix_room_id)
)
