// Static file server for the frontend, used by the UI smoke test.
//
// The desktop frontend is zero-build vanilla HTML/CSS/JS, so it can be served
// straight off disk. Tauri injects window.__TAURI__ at runtime; here the smoke
// test injects a stub (see tests/tauri-stub.js) so the app boots with no Rust
// behind it.
//
// Usage: node scripts/serve.mjs <root-dir> [port]
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize, resolve, sep } from "node:path";

const root = resolve(process.argv[2] ?? "src");
const port = Number(process.argv[3] ?? 0);

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", "http://localhost");
  const rel = normalize(decodeURIComponent(url.pathname)).replace(/^([/\\])+/, "");
  // Refuse anything that climbs out of the served root.
  const target = join(root, rel === "" ? "index.html" : rel);
  if (target !== root && !target.startsWith(root + sep)) {
    res.writeHead(403).end("forbidden");
    return;
  }
  try {
    const body = await readFile(target);
    res.writeHead(200, {
      "content-type": TYPES[extname(target)] ?? "application/octet-stream",
      "cache-control": "no-store",
    });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found");
  }
});

server.listen(port, "127.0.0.1", () => {
  // Port 0 asks the OS for a free one; report what we actually got.
  process.stdout.write(String(server.address().port));
});
