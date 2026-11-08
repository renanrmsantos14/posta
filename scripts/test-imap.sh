#!/usr/bin/env sh
# Sobe um IMAP de teste (Greenmail) e roda os testes de integração do mail-core.
set -e
NAME=posta-greenmail
if ! docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
  docker run -d --rm --name "$NAME" -p 3143:3143 \
    -e GREENMAIL_OPTS="-Dgreenmail.setup.test.imap -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.auth.disabled" \
    greenmail/standalone:2.1.3 >/dev/null
  until docker logs "$NAME" 2>&1 | grep -q "Starting GreenMail API"; do sleep 1; done
fi
POSTA_IMAP_TEST=127.0.0.1:3143 cargo test -p mail-core --release --test imap_greenmail -- --nocapture
