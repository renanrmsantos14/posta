#!/usr/bin/env sh
# Sobe IMAP + SMTP de teste (Greenmail) e roda os testes de integração do mail-core.
set -e
NAME=posta-greenmail
# Container antigo só com IMAP: recria com SMTP.
if docker ps --format '{{.Names}} {{.Ports}}' | grep "^$NAME " | grep -vq 3025; then
  docker stop "$NAME" >/dev/null
  while docker ps -a --format '{{.Names}}' | grep -qx "$NAME"; do sleep 1; done
fi
if ! docker ps --format '{{.Names}}' | grep -qx "$NAME"; then
  docker run -d --rm --name "$NAME" -p 3143:3143 -p 3025:3025 \
    -e GREENMAIL_OPTS="-Dgreenmail.setup.test.imap -Dgreenmail.setup.test.smtp -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.auth.disabled" \
    greenmail/standalone:2.1.3 >/dev/null
  until docker logs "$NAME" 2>&1 | grep -q "Starting GreenMail API"; do sleep 1; done
fi
POSTA_IMAP_TEST=127.0.0.1:3143 POSTA_SMTP_TEST=3025 cargo test -p mail-core --release --test imap_greenmail -- --nocapture
