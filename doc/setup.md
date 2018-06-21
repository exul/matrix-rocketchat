# Setup ⚠ DO NOT USE THIS - IT IS NOT READY ⚠

## Download and Verification

```
MRC_VERSION=0.1.0; \
echo -e "https://github.com/exul/matrix-rocketchat/releases/download/${MRC_VERSION}/matrix-rocketchat-x86_64-unknown-linux-gnu-${MRC_VERSION}.tar.gz
https://github.com/exul/matrix-rocketchat/releases/download/${MRC_VERSION}/matrix-rocketchat-x86_64-unknown-linux-gnu-${MRC_VERSION}.tar.gz.sha256" | \
xargs wget
```

Verify the files:

```
sha256sum -c matrix-rocketchat-x86_64-unknown-linux-gnu-${MRC_VERSION}.tar.gz.sha256
```

If the verification is successful, the output should look like this:

```
matrix-rocketchat-x86_64-unknown-linux-gnu-0.1.0.tar.gz: OK
```

If the check is not successful, the output will look like this:

```
matrix-rocketchat-x86_64-unknown-linux-gnu-0.1.0.tar.gz: FAILED
sha256sum: WARNING: 1 computed checksum did NOT match
```

Download the files again if the checksum doesn't match.

## Binary Installation

Unpack the archive:

```
tar xzf matrix-rocketchat-x86_64-unknown-linux-gnu-${MRC_VERSION}.tar.gz
```

Install the binary:

```
sudo install -m 755  matrix-rocketchat-x86_64-unknown-linux-gnu-${MRC_VERSION}/matrix-rocketchat /usr/local/bin
```

## Configuration

### User and Permissions

Do NOT run the application service as root! To run the appliation service as `matrix-rocketchat` execute the following steps (adjust paths according to your config file):

* Create the user `sudo useradd --system -d /var/lib/matrix-rocketchat -m -g nogroup -s /bin/false matrix-rocketchat`
* Create log directory `sudo mkdir /var/log/matrix-rocketchat`
* Change the ownership of the log directory `sudo chown matrix-rocketchat:nogroup /var/log/matrix-rocketchat`

### Application Service

Download the sample config file:

```
wget https://raw.githubusercontent.com/exul/matrix-rocketchat/${MRC_VERSION}/config/config.yaml.sample
```

Install the file (adjust the directory to your application service config directory and the `matrix-rocketchat` to the user who runs the application service process):

```
sudo install -D -m 600 -o matrix-rocketchat -g nogroup config.yaml.sample /usr/local/etc/matrix-rocketchat/config.yaml
```

Update the values:

```
sudo $EDITOR /usr/local/etc/matrix-rocketchat/config.yaml
```

If you haven't set `$EDITOR`, replace it with your favorite editor.

Now go through **each** of the settings and adjust them if needed:

#### `hs_token`
The token that is used to authenticate the homeserver when sending requests to the application service.

This has to match the parameter `hs_token` in the appliation service registration file.

*Keep this private, anybody with this token can send valid requests to the application servce!*

If unsure generate one via `head /dev/urandom | tr -dc A-Za-z0-9 | head -c 50 ; echo ''`

#### `as_token`
The token that is used to authenticate the application service when sending requests to the homeserver.

This has to match the parameter `as_token` in the appliation service registration file.

*Keep this private, anybody with this token can send valid requests to the homeserver (within the scope of the application service)!*

Use a different token then the `hs_token`!

If unsure generate one via `head /dev/urandom | tr -dc A-Za-z0-9 | head -c 50 ; echo ''`

#### `as_address`
The address to which the application service binds. If you run the application service without a reverse proxy, this has to be a public IP address of your server. You can choose any IP/port that is available on your server. 0.0.0.0 binds to all interfaces.

This example will use a reverse proxy in front of the application service, so it will only listen on localhost, for example `127.0.0.1:8822`.

#### `as_url`

The URL which can be used to access the application service. If you type this
URL in your browser after you started the application service you should see
"Your Rocket.Chat <-> Matrix application service is running".

It can be a domain that points to the public IP address of your server or the
IP address itself.

This has to match the parameter `url` in the application service registration file.

#### `hs_url`
The URL which can be used to access your homeserver. If the homeserver is
running on the same machine you can use the non SSL port 8008. If the
homeserver is running on another machine, use the servername with the SSL
port(for example https://example.org:8448).

#### `hs_domain`
The domain of the homeserver. It is used to create the usernames (the part
after the colon).
This has to match the parameter `server_name` in your homeserver.yaml
(if you are running synapse as your homeserver).

#### `sender_localpart`
The local part of your bot, the full name will be something like
@_rocketchat:example.com This will also be the namespace that the application
service uses on the homeserver so no other application service/user will be
able to use username that starts with this string.
This has to match the parameter `sender_localpart` in the application service registration file.

#### `database_url`
The URL to the SQL-Lite database that the application service will use. You
can choose any file path that the user you run the application service as has
access to. The path can be absolute or relative.

#### `log_level`
Logging verbosity, available values: debug, info, warning, error. The recommended level for production is `info`.

#### `log_to_console`
Flag that indicates if the application service should output the log to the console.

#### `log_to_file`
Flag that indicates if the application service should log to a file.

#### `log_file_path`
Path to the log file (this is only mandatory if logging to a file is enabled)

#### `accept_remote_invites`
Flag to indicate if the bot user accepts invites from rooms on other homeservers.
Which means that users from other homeservers can use this Rocket.Chat bridge
if the flag is set to true.

#### `use_https`
Flag that indicates if the application service should use HTTPS. It's highly
recommended that you use HTTPS if you expose the application service directly
(bind it to a public IP address). If you run the application service behind
a reverse-proxy you can run it on localhost and let the proxy handle HTTPS.

#### `pkcs12_path`
Path to the PKCS 12 file (this is only mandatory if you run the
application service with SSL).

#### `pkcs12_password`
The password to decrypt the PKCS 12 file (this is only mandatory if you run the
application service with SSL).

#### Example Configuration File (matrix-rocket.chat uses those setting):

*Do NOT just copy and paste this!*

Synapse and the application service run on the same server. A reverse proxy is used to expose the appservice to the internet.

```
hs_token: "hs-secret-use-your-own-random-string!"
as_token: "as-secret-use-your-own-random-string!"
as_address: "127.0.0.1:8822"
as_url: "https://matrix-rocket.chat:8822"
hs_url: "http://127.0.0.1:8008"
hs_domain: "matrix-rocket.chat"
sender_localpart: "_rocketchat"
database_url: "/var/lib/matrix-rocketchat/database.sqlite3"
log_level: "info"
log_to_console: false
log_to_file: true
log_file_path: "/var/log/matrix-rocketchat/application-service.log"
accept_remote_invites: false
use_https: false
pkcs12_path: ""
pkcs12_password: ""
```

### Application Service Registration

Download the sample registration file:

```
wget https://raw.githubusercontent.com/exul/matrix-rocketchat/${MRC_VERSION}/config/matrix_rocketchat_registration.yaml.sample
```

Install the file (adjust the directory to your homeservers config directory and the `matrix-synapse` to the user who runs the homeserver process):

```
sudo install -D -m 600 -o matrix-synapse -g nogroup matrix_rocketchat_registration.yaml.sample /etc/matrix-synapse/matrix_rocketchat_registration.yaml
```

Update the values:

```
sudo $EDITOR /etc/matrix-synapse/matrix_rocketchat_registration.yaml
```

If you haven't set `$EDITOR`, replace it with your favorite editor.

Now go through **each** of the settings and adjust them if needed:

#### `id`

An identifier that has to be unique across all application services that are registred. It should never change once an application service is registered.

#### `hs_token`

The token that is used to authenticate the homeserver when sending requests to the application service.

This has to match the parameter `hs_token` in the appliation service config file.

*Keep this private, anybody with this token can send valid requests to the application servce!*

#### `as_token`

The token that is used to authenticate the application service when sending requests to the homeserver.

This has to match the parameter `as_token` in the appliation service config file.

*Keep this private, anybody with this token can send valid requests to the homeserver (within the scope of the application service)!*

#### `namespaces`

An application service can register certain namespaces. There are namespaces for users, aliases and rooms.

See https://matrix.org/docs/spec/application_service/unstable.html#registration for more details.

#### `url`

The base url of the application service.

This has to match the parameter `as_url` in the application service config file.

#### `sender_localpart`

The local part of your bot, the full name will be something like @_rocketchat:example.com This will also be the namespace that the application service uses on the homeserver so no other application service/user will be able to use username that starts with this string. This has to match the parameter sender_localpart in the application service config.

#### Example Registration File (matrix-rocket.chat uses those setting):

*Do NOT just copy and paste this!*

```
id: rocketchat
hs_token: "hs-secret-use-your-own-random-string!!!"
as_token: "as-secret-use-your-own-random-string!!!"
namespaces:
  users:
    - exclusive: true
      regex: '@_rocketchat.*'
  aliases:
    - exclusive: true
      regex: '#_rocketchat.*'
  rooms: []
url: "http://127.0.0.1:8822"
sender_localpart: _rocketchat
```

### Homeserver

Now that the registration file is ready, the homeserver has to know about it.

Add to registration file to the list of application service config files (`app_service_config_files` in `homeserver.yaml`).

The homeserver has to be restarted to apply the change (for example via `sudo systemctl restart matrix-synapse`).

#### Example Config

```
app_service_config_files: ["/etc/matrix-synapse/matrix_rocketchat_registration.yaml"]
```

### Startup

Now you can try to see the bridge manually to test if the setup works:

```
sudo -s /bin/sh -c '/usr/local/bin/matrix-rocketchat -c /usr/local/etc/matrix-rocketchat/config.yaml'
```

When executing

```
curl http://localhost:8822
```

in another shell on the server, that should result in

```
Your Rocket.Chat <-> Matrix application service is running
```

Check the log file if the application service doesn't start or if there is no response.

## Reverse Proxy

With the above setup the application service is running, but isn't expose to the internet. One way to do that is a reverse proxy.