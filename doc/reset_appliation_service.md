# Cleanup all rooms
When working on the application service as a developer you might want to reset everything.
A normal matrix user cannot do that, because the rooms were created by the application service users.
It's also hard to keep track of all the rooms, so an automated cleanup is simpler.

The script bellow deletes all the room aliases that were created by the application service.
It also queries all the rooms for each application service user and leaves/forgets them.
It will not delete the users, because that's not possible in Matrix.

## ⚠ Warning ⚠
The script is intended to be used during development, do not use in against your production homeserver!

It doesn't do any checks, which means that rooms that were bridged will no longer work,
because the room alias will be deleted and all virtual users leave the room,
even if there are still Matrix users in that room.
Since the bot user is also a virtual user, all admin rooms will no longer work as well.

## Prerequisites
To run the scripts `bash`, `curl` and `jq` are needed.

The client program for the database depends on the database you use.

## SQLite

```sql
sqlite3 ./homeserver.db
sqlite> .output /tmp/users.txt
sqlite> SELECT name FROM users WHERE name LIKE '@rocketchat%';
sqlite> .exit
```

Where `rocketchat` is the `sender_localpart` that was configured for the application service.

## Postgres

```sql
psql -d synapse -U synapse_user
synapse=> \o /tmp/users.txt
synapse=> SELECT name FROM users WHERE name LIKE '@rocketchat%';
synapse=> \q
```
Where `rocketchat` is the `sender_localpart` that was configured for the application service.

## Bash Script

```bash
#!/bin/bash

USERS_FILE=/tmp/users.txt
HOMESERVER_URL=http://localhost:8009
BOT_USER_ID=@rocketchat:example.com
TOKEN=as_token

function get_alias_for_room(){
    alias_url="${HOMESERVER_URL}/_matrix/client/r0/rooms/${1}/state/m.room.canonical_alias/?access_token=${TOKEN}"
    curl -sS ${alias_url} | jq -r '.alias'
}

function room_aliases_sanity_check(){
    not_joined_rooms=($(
        curl -sS "${HOMESERVER_URL}/_matrix/client/r0/sync?access_token=${TOKEN}" | \
        jq -r '.rooms.leave, .rooms.invite | to_entries[] | .key'
    ))

    for room_id in "${not_joined_rooms[@]}"; do
        room_alias=$(get_alias_for_room ${room_id})
        if [[ "$ROOM_ALIAS" != "null" ]]; then
            echo "Room ${ROOM_ID} has an alias ${room_alias}, but the bot user left the room or didn't join it yet."
            echo "This should never happen, you have to clean this up manually."
            exit 1
        fi
    done

    echo "Room alias sanity check passed."
}

function url_encode_room_alias(){
    sed 's/#/%23/g; s/:/%3A/g' <<< ${1}
}

function confirm_room_alias_deletion(){
    encoded_alias=$(url_encode_room_alias $room_alias)    
    room_alias_err=$(
        curl -sS "${HOMESERVER_URL}/_matrix/client/r0/directory/room/${encoded_alias}?access_token=${TOKEN}" | \
        jq -r '.errcode'
    )

    if [[ "$room_alias_err" != "M_NOT_FOUND" ]]; then
      echo "Looks like the room alias ${room_alias} was not deleted, exiting."
      exit 1
    fi 
}

function delete_room_alias(){
    room_alias=$(get_alias_for_room ${$1})    
    echo "Deleting room alias ${room_alias} from room ${room_id}."    
    encoded_alias=$(url_encode_room_alias $room_alias)
    curl -sS --fail -X DELETE "${HOMESERVER_URL}/_matrix/client/r0/directory/room/${encoded_alias}?access_token=${TOKEN}" 2>&1 >/dev/null || \
    echo "Could not delete room alias $1, exiting" && \
    exit 1
    confirm_room_alias_deletion $room_alias
}

function delete_all_application_service_room_aliases(){
    joined_rooms=($(
        curl -sS "${HOMESERVER_URL}/_matrix/client/r0/sync?access_token=${TOKEN}" | \
        jq -r '.rooms.join | to_entries[] | .key'
    ))

    for room_id in "${joined_rooms[@]}"; do
        if [[ "$room_alias" != "null" ]]; then
            delete_room_alias ${room_id}
        fi
    done

    echo "All room aliases created by the application service are deleted."
}

function get_rooms_to_leave(){
    curl -sS "${HOMESERVER_URL}/_matrix/client/r0/sync?access_token=${TOKEN}&user_id=${1}" | \
    jq -r '.rooms.join, .rooms.invite | to_entries[] | .key'
}

function get_left_rooms(){
    curl -sS "${HOMESERVER_URL}/_matrix/client/r0/sync?access_token=${TOKEN}&user_id=${1}" | \
    jq -r '.rooms.leave | to_entries[] | .key'
}

function leave_room_for_user(){
    room_id=${1}
    user_id=${2}
    echo "Leaving room ${room_id} for user ${user_id}."
    curl -sS --fail -X POST "${HOMESERVER_URL}/_matrix/client/r0/rooms/${room_id}/leave?access_token=${TOKEN}&user_id=${user_id}" 2>&1 >/dev/null || \
    echo "Could not leave room ${room_id} for user ${user_id}" && \
    exit 1
}

function forget_room(){
    room_id=${1}
    user_id=${2}
    echo "Forgetting room ${room_id} for user ${user_id}."
    curl -sS --fail -X POST "${HOMESERVER_URL}/_matrix/client/r0/rooms/${room_id}/forget?access_token=${TOKEN}&user_id=${user_id}" 2>&1 >/dev/null || \
    echo "Could not forget room ${room_id} for user ${user_id}" && \
    exit 1
}

function leave_and_forget_all_application_service_rooms(){
    while read user_id; do
        rooms_to_leave=($(get_rooms_to_leave ${user_id}))
        for room_id in "${rooms_to_leave[@]}"; do
            leave_room_for_user ${room_id} ${user_id}
        done

        left_rooms=($(get_left_rooms ${user_id}))
        for room_id in "${left_rooms[@]}"; do
             forget_room_for_user${room_id} ${user_id}
        done
    done <$USERS_FILE
    
    echo "All application service users have left and forgotten all the rooms, yay!"
}

echo "$(room_aliases_sanity_check)"
echo "$(delete_all_application_service_room_aliases)"
echo "$(leave_and_forget_all_application_service_rooms)"
```