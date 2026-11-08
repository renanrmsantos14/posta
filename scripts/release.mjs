// pnpm release [versão]: sobe a versão, cria o commit e a tag e envia.
// O GitHub Actions (.github/workflows/release.yml) gera os instaladores e publica o release.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

if (git("branch", "--show-current") !== "master") throw new Error("publique a partir do master");
if (git("status", "--porcelain")) throw new Error("há mudanças não commitadas");
git("pull", "--ff-only");

execFileSync(process.execPath, ["apps/client/scripts/bump-version.mjs", ...process.argv.slice(2)], { stdio: "inherit" });
const { version } = JSON.parse(readFileSync("apps/client/package.json", "utf8"));
const tag = `v${version}`;

git("add", "apps/client/package.json", "apps/client/src/version.json", "apps/client/src-tauri/tauri.conf.json");
git("commit", "-m", `Lança ${tag}`);
git("tag", "-a", tag, "-m", tag);
git("push", "--follow-tags");
console.log(`${tag} enviado. Acompanhe: https://github.com/renanrmsantos14/posta/actions`);
