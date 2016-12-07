/// A Matrix user.
#[derive(Queryable)]
pub struct User {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: String,
    /// Flag to indicate if the user is only used to send messages from Rocket.Chat
    pub is_virtual_user: bool,
    /// Time when the user sent the last message in seconds since UNIX_EPOCH
    pub last_message_sent: i64,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}
