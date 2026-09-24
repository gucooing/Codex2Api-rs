import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const manifestPath = join(root, "out/.source-manifest.json");

async function sourceHashes() {
  const files = {};
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const absolute = join(directory, entry.name);
      const path = relative(root, absolute).replaceAll("\\", "/");
      if (entry.isDirectory()) {
        if (
          directory !== root ||
          ["src", "app", "components", "lib", "public", "scripts"].includes(entry.name)
        )
          await visit(absolute);
      } else if (
        directory !== root ||
        /^(package.*\.json|components\.json|next\.config\..+|postcss\.config\..+|eslint\.config\..+|tsconfig\.json)$/.test(
          entry.name,
        )
      ) {
        files[path] = createHash("sha256")
          .update(await readFile(absolute))
          .digest("hex");
      }
    }
  }
  await visit(root);
  return Object.fromEntries(Object.entries(files).sort());
}

const before = await sourceHashes();
await rm(manifestPath, { force: true });
const build = spawnSync(
  process.execPath,
  [join(root, "node_modules/next/dist/bin/next"), "build"],
  {
    cwd: root,
    env: { ...process.env, NEXT_TELEMETRY_DISABLED: "1" },
    stdio: "inherit",
  },
);
if (build.status !== 0) process.exit(build.status ?? 1);

// Next's client names prefetch segments with dots. Windows export can retain
// backslashes from the segment path, creating directories instead of that filename.
async function normalizeSegments(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const source = join(directory, entry.name);
    if (entry.isDirectory()) {
      await normalizeSegments(source);
    } else if (entry.name.endsWith(".txt")) {
      const parts = relative(join(root, "out"), source).replaceAll("\\", "/").split("/");
      const segment = parts.findIndex((part) => part.startsWith("__next."));
      if (segment >= 0 && segment < parts.length - 1) {
        await rename(
          source,
          join(root, "out", ...parts.slice(0, segment), parts.slice(segment).join(".")),
        );
      }
    }
  }
}
await normalizeSegments(join(root, "out"));
const files = await sourceHashes();
if (JSON.stringify(before) !== JSON.stringify(files)) {
  throw new Error("Frontend sources changed during export. Run npm run build again before Cargo.");
}
await writeFile(manifestPath, JSON.stringify({ files }, null, 2) + "\n");
