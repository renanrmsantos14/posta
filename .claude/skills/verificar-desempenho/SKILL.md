---
name: verificar-desempenho
description: Verifica o desempenho do backend do Posta (store SQLite, sync IMAP incremental, sessão persistente do Hub). Use ao mudar crates/mail-core (store.rs, imap_sync.rs, session.rs, outbox.rs) ou os comandos de lista/sync em apps/client/src-tauri/src/lib.rs.
---

# Verificar desempenho (backend)

1. Corretude primeiro: `cargo test -q -p mail-core` (store: fios incrementais x recálculo, paginação por cursor, plano de índice, migração de banco grande do master, delta; `imap_fake`: QRESYNC/VANISHED, reconexão, UIDVALIDITY, concorrência no Hub, corpo sob demanda offline, fila offline pela sessão do Hub).
2. Bench local (sem rede): `PERF_LOCAL=50000 cargo test --release -p mail-core --test perf_bench -- --nocapture bench_local`. Use `CARGO_TARGET_DIR` fora do repo se o disco estiver apertado.
3. Bench completo com servidor (opcional): `scripts/test-imap.sh` sobe o GreenMail; depois `PERF_N=50000 cargo test --release -p mail-core --test perf_bench -- --nocapture`. `PERF_FAKE` exige servidor na porta 3145.
4. Se mexeu em `apps/client/src-tauri`: cargo check do crate `posta` no Docker (ver `verificar-mail-core`, passo 3) e `git checkout -- apps/client/src-tauri/gen/schemas`.
5. Metas (50 mil mensagens, build release):
   - inserção de 50k: ~10 s (hoje ~10,2 s; FTS com `prefix='2'`);
   - primeira página da lista: < 1 ms;
   - `unread_counts`: < 5 ms;
   - busca: < 40 ms;
   - `suggest_contacts`: ~0,1 ms;
   - IDLE até o banco: < 1 s;
   - reabrir banco migrado: < 500 ms.
6. Passou quando: testes verdes, nenhuma meta estourada em mais de ~20% (o bench varia entre execuções; repita antes de concluir regressão) e o cargo check termina com `Finished`.
