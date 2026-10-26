#!/bin/sh
# Copies run-generated material into root-owned private paths that satisfy sshd StrictModes,
# sets the generated password from the environment and runs sshd in the foreground.
set -eu
adduser -D -s /bin/sh stoneql
printf 'stoneql:%s\n' "$STONEQL_SSH_PASSWORD" | chpasswd >/dev/null
unset STONEQL_SSH_PASSWORD
install -d -m 755 /etc/ssh/stoneql-keys
install -m 644 /stoneql-ssh/authorized_keys /etc/ssh/stoneql-keys/stoneql
install -m 600 /stoneql-ssh/host_key /etc/ssh/stoneql_host_key
install -m 644 /stoneql-ssh/sshd_config /etc/ssh/stoneql_sshd_config
exec /usr/sbin/sshd -D -e -f /etc/ssh/stoneql_sshd_config
