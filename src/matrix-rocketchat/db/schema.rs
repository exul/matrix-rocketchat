#![allow(missing_docs)]

table! {
    users (matrix_user_id) {
        matrix_user_id -> Text,
        language -> Text,
        is_virtual_user -> Bool,
        last_message_sent -> BigInt,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    rooms (matrix_room_id) {
        matrix_room_id -> Text,
        display_name -> Text,
        rocketchat_room_id -> Nullable<Text>,
        is_admin_room -> Bool,
        is_bridged -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

table! {
    users_in_rooms (matrix_user_id, matrix_room_id) {
        matrix_user_id -> Text,
        matrix_room_id -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}
