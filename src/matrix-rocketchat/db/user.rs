use std::time::{SystemTime, UNIX_EPOCH};

use diesel;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use ruma_identifiers::UserId;

use errors::*;
use i18n::*;
use super::schema::users;

/// A Matrix `User`.
#[derive(Associations, Debug, Identifiable, Queryable)]
#[primary_key(matrix_user_id)]
#[table_name="users"]
pub struct User {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: UserId,
    /// The language the user prefers to get messages in.
    pub language: String,
    /// Time when the user sent the last message in seconds since UNIX_EPOCH
    pub last_message_sent: i64,
    /// created timestamp
    pub created_at: String,
    /// updated timestamp
    pub updated_at: String,
}

/// A new Matrix `User`, not yet saved.
#[derive(Insertable)]
#[table_name="users"]
pub struct NewUser<'a> {
    /// The users unique id on the Matrix server.
    pub matrix_user_id: UserId,
    /// The language the user prefers to get messages in.
    pub language: &'a str,
}

impl User {
    /// Insert a new `User` into the database.
    pub fn insert(connection: &SqliteConnection, user: &NewUser) -> Result<User> {
        diesel::insert(user).into(users::table).execute(connection).chain_err(|| ErrorKind::DBInsertError)?;
        User::find(connection, &user.matrix_user_id)
    }

    /// Find a `User` by his matrix user ID, return an error if the user is not found
    pub fn find(connection: &SqliteConnection, matrix_user_id: &UserId) -> Result<User> {
        users::table.find(matrix_user_id).first(connection).chain_err(|| ErrorKind::DBSelectError).map_err(Error::from)
    }

    /// Find or create `User` with a given Matrix user ID.
    pub fn find_or_create_by_matrix_user_id(connection: &SqliteConnection, matrix_user_id: UserId) -> Result<User> {
        match User::find_by_matrix_user_id(connection, &matrix_user_id)? {
            Some(user) => Ok(user),
            None => {
                let new_user = NewUser {
                    matrix_user_id: matrix_user_id.clone(),
                    language: DEFAULT_LANGUAGE,
                };
                User::insert(connection, &new_user)
            }
        }
    }

    /// Find a `User` by his matrix user ID. Returns `None`, if the user is not found.
    pub fn find_by_matrix_user_id(connection: &SqliteConnection, matrix_user_id: &UserId) -> Result<Option<User>> {
        let users = users::table.find(matrix_user_id).load(connection).chain_err(|| ErrorKind::DBSelectError)?;
        Ok(users.into_iter().next())
    }

    /// Update last message sent.
    pub fn set_last_message_sent(&mut self, connection: &SqliteConnection) -> Result<()> {
        let last_message_sent =
            SystemTime::now().duration_since(UNIX_EPOCH).chain_err(|| ErrorKind::InternalServerError)?.as_secs() as i64;
        self.last_message_sent = last_message_sent;
        diesel::update(users::table.find(&self.matrix_user_id)).set(users::last_message_sent.eq(last_message_sent))
            .execute(connection)
            .chain_err(|| ErrorKind::DBUpdateError)?;
        Ok(())
    }
}
