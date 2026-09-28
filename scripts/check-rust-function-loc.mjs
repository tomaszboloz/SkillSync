import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import process from "node:process";

const MAX_LINES = 150;
const ROOT = join(process.cwd(), "src-tauri", "src");
const FUNCTION = /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)/;

async function rustFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await rustFiles(path)));
    else if (entry.isFile() && entry.name.endsWith(".rs")) files.push(path);
  }
  return files;
}

const violations = [];
for (const path of await rustFiles(ROOT)) {
  const lines = (await readFile(path, "utf8")).split("\n");
  const starts = lines.flatMap((line, index) => {
    const match = line.match(FUNCTION);
    return match ? [{ index, name: match[1] }] : [];
  });
  starts.forEach((functionStart, index) => {
    const end = starts[index + 1]?.index ?? lines.length;
    const size = end - functionStart.index;
    if (size > MAX_LINES) {
      violations.push(`${path}:${functionStart.index + 1} ${functionStart.name}=${size}`);
    }
  });
}

if (violations.length > 0) {
  process.stderr.write(`Rust function LOC limit exceeded (${MAX_LINES}):\n`);
  for (const violation of violations) process.stderr.write(`- ${violation}\n`);
  process.exit(1);
}
process.stdout.write(`Rust function LOC check passed (max ${MAX_LINES}).\n`);
