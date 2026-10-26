# Disposable OpenSSH server for StoneQL M005 fixture tests only. Keys, users and passwords are
# generated per run by tools/test-ssh-fixture.mjs and mounted read-only; nothing is baked in.
FROM alpine:3.24@sha256:71e9b7bae160ba167fa17414401b94cbcdaf3c32feeab9298bd794a2a0614c35
RUN apk add --no-cache openssh-server
COPY entrypoint.sh /usr/local/bin/stoneql-sshd
ENTRYPOINT ["/bin/sh", "/usr/local/bin/stoneql-sshd"]
