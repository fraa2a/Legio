// Copies the documentation screenshots from a smoke test output directory into _meta/screenshots/app.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DOCUMENTED_SCREENSHOTS = ["home", "library", "game", "store", "settings"];

const [source] = process.argv.slice(2);
if (source === undefined) {
  console.error("Usage: node tools/app-smoke/update-screenshots.mjs <smoke output directory>");
  process.exit(2);
}
const captures = join(resolve(source), "screenshots");
const missing = DOCUMENTED_SCREENSHOTS.filter((name) => !existsSync(join(captures, `${name}.png`)));
if (missing.length > 0) {
  console.error(`Missing captures in ${captures}: ${missing.join(", ")}`);
  process.exit(1);
}
const destination = resolve(dirname(fileURLToPath(import.meta.url)), "../../_meta/screenshots/app");
mkdirSync(destination, { recursive: true });
for (const name of DOCUMENTED_SCREENSHOTS) {
  copyFileSync(join(captures, `${name}.png`), join(destination, `${name}.png`));
  console.log(`Updated _meta/screenshots/app/${name}.png`);
}
