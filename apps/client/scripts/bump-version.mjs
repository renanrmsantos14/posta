// Sobe a versão de publicação (PATCH, ou a versão passada como argumento) e grava data/hora de São Paulo.
import { readFileSync, writeFileSync } from "node:fs";

const pkgUrl = new URL("../package.json", import.meta.url);
const pkg = JSON.parse(readFileSync(pkgUrl, "utf8"));
const [major, minor, patch] = pkg.version.split(".").map(Number);
const wanted = process.argv[2]?.replace(/^v/, "");
if (wanted && !/^\d+\.\d+\.\d+$/.test(wanted)) throw new Error(`versão inválida: ${wanted}`);
pkg.version = wanted ?? `${major}.${minor}.${patch + 1}`;
writeFileSync(pkgUrl, JSON.stringify(pkg, null, 2) + "\n");

const builtAt = new Intl.DateTimeFormat("pt-BR", {
  timeZone: "America/Sao_Paulo",
  day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
}).format(new Date()).replace(",", "");
writeFileSync(new URL("../src/version.json", import.meta.url),
  JSON.stringify({ version: pkg.version, builtAt }, null, 2) + "\n");

// Mantém o app empacotado com a mesma versão.
const confUrl = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const conf = JSON.parse(readFileSync(confUrl, "utf8"));
conf.version = pkg.version;
writeFileSync(confUrl, JSON.stringify(conf, null, 2) + "\n");
console.log(`Posta v${pkg.version} · ${builtAt}`);
