// Rodar: node --experimental-strip-types --test apps/client/tests/savedSearches.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { parseSavedSearches, serializeSavedSearches, suggestName } from "../src/savedSearches.ts";

const one = { id: "a", name: "Anexos do João", query: "de:joao tem:anexo", folder: "box:x|Clientes", scope: "folder", account: "x" };

test("ida e volta preserva a pesquisa", () => {
  assert.deepEqual(parseSavedSearches(serializeSavedSearches([one])), [one]);
});

test("valores ausentes ou inválidos viram o padrão", () => {
  const [s] = parseSavedSearches(JSON.stringify([{ id: "b", name: "n", query: "é:não-lida", scope: "x", account: 3 }]));
  assert.deepEqual(s, { id: "b", name: "n", query: "é:não-lida", folder: "inbox", scope: "folder", account: null });
});

test("lixo é ignorado", () => {
  assert.deepEqual(parseSavedSearches(undefined), []);
  assert.deepEqual(parseSavedSearches("{"), []);
  assert.deepEqual(parseSavedSearches('{"a":1}'), []);
  assert.deepEqual(parseSavedSearches(JSON.stringify([null, 1, { id: "c", name: "x", query: "  " }, { name: "sem id", query: "q" }, one])), [one]);
});

test("nome sugerido corta a consulta", () => {
  assert.equal(suggestName("  de:ana  "), "de:ana");
  assert.equal(suggestName("x".repeat(80)).length, 40);
});
