use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::UserId;

use errors::*;
use i18n::*;
use super::schema::users;

/// A Matrix user.
#[derive(Queryable)]
pub struct User {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: UserId,
    /// The language the user prefers to get messages in.
    pub language: String,
    /// Flag to indicate if the user is only used to send messages from Rocket.Chat
    pub is_virtual_user: bool,
    /// Time when the user sent the last message in seconds since UNIX_EPOCH
    pub last_message_sent: i64,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new Matrix user, not yet saved.
#[derive(Insertable)]
#[table_name="users"]
pub struct NewUser<'a> {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: UserId,
    /// The language the user prefers to get messages in.
    pub language: &'a str,
    /// Flag to indicate if the user is only used to send messages from Rocket.Chat
    pub is_virtual_user: bool,
    /// Time when the user sent the last message in seconds since UNIX_EPOCH
    pub last_message_sent: i64,
}

impl User {
    /// Insert a new user into the database.
    pub fn insert(connection: &SqliteConnection, user: &NewUser) -> Result<User> {
        diesel::insert(user).into(users::table).execute(connection).chain_err(|| ErrorKind::DBInsertFailed)?;
        User::find(connection, &user.matrix_user_id)
    }

    /// Find a user by his matrix user ID, return an error if the user is not found
    pub fn find(connection: &SqliteConnection, matrix_user_id: &UserId) -> Result<User> {
        let user = users::table.find(matrix_user_id).first(connection).chain_err(|| "User not found")?;
        Ok(user)
    }

    /// Find or create user with a given Matrix user ID.
    pub fn find_or_create_by_matrix_user_id(connection: &SqliteConnection, matrix_user_id: UserId) -> Result<User> {
        match User::find_by_matrix_user_id(connection, &matrix_user_id)? {
            Some(user) => Ok(user),
            None => {
                let new_user = NewUser {
                    matrix_user_id: matrix_user_id,
                    language: DEFAULT_LANGUAGE,
                    is_virtual_user: false,
                    last_message_sent: 0,
                };
                User::insert(connection, &new_user)
            }

        }
    }

    /// Find a user by his matrix user ID.
    pub fn find_by_matrix_user_id(connection: &SqliteConnection, matrix_user_id: &UserId) -> Result<Option<User>> {
        let users = users::table.find(matrix_user_id).load(connection).chain_err(|| ErrorKind::DBSelectFailed)?;
        Ok(users.into_iter().next())
    }
}
