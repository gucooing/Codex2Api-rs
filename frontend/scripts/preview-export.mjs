// Loopback-only static export preview for browser contract tests. Production is Rust-embedded.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, resolve, sep, extname } from "node:path";
import { fileURLToPath } from "node:url";

const roots = {
  admin: resolve(dirname(fileURLToPath(import.meta.url)), "../out"),
  user: resolve(dirname(fileURLToPath(import.meta.url)), "../../frontend-user/out"),
};
const types = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".txt": "text/plain",
  ".woff2": "font/woff2",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};
createServer(async (request, response) => {
  try {
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    const prefix = url.pathname.split("/")[1];
    const root = Object.hasOwn(roots, prefix) ? roots[prefix] : undefined;
    if (!root || !url.pathname.startsWith(`/${prefix}/`)) {
      response.writeHead(404).end();
      return;
    }
    let path = decodeURIComponent(url.pathname.slice(prefix.length + 1));
    if (path.endsWith("/")) path += "index.html";
    const file = resolve(root, `.${path}`);
    if (!file.startsWith(root + sep)) {
      response.writeHead(403).end();
      return;
    }
    const content = await readFile(file);
    response.writeHead(200, {
      "Content-Type": types[extname(file)] ?? "application/octet-stream",
      "Cache-Control": "no-store",
    });
    response.end(request.method === "HEAD" ? undefined : content);
  } catch {
    response.writeHead(404).end();
  }
}).listen(Number(process.env.CODEX2API_PREVIEW_PORT ?? 8793), "127.0.0.1");
