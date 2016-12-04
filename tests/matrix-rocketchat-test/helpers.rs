use api::call_url;

pub fn create_admin_room(as_url: String, admin_room_id: &str, test_user_id: &str, bot_user_id: &str) {
    let url = as_url + "/transactions/adminroomcreationmessageid";

    let invite_payload = r#"{
            "events": [{
                "event_id": "$1:localhost",
                "room_id": "ADMIN_ROOM_ID",
                "type": "m.room.member",
                "state_key": "BOT_USER_ID",
                "sender": "TEST_USER_ID",
                "content": {
                    "membership": "invite"
                }
            }]
        }"#
        .replace("ADMIN_ROOM_ID", admin_room_id)
        .replace("TEST_USER_ID", test_user_id)
        .replace("BOT_USER_ID", bot_user_id);

    call_url("PUT", &url, &invite_payload);

    let join_payload = r#"{
            "events": [{
                "event_id": "$2:localhost",
                "room_id": "ADMIN_ROOM_ID",
                "type": "m.room.member",
                "state_key": "BOT_USER_ID",
                "sender": "BOT_USER_ID",
                "content": {
                    "membership": "join"
                }
            }]
        }"#
        .replace("ADMIN_ROOM_ID", admin_room_id)
        .replace("BOT_USER_ID", bot_user_id);

    call_url("PUT", &url, &join_payload);
}
