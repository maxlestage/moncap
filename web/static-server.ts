// Petit serveur statique pour le dossier dist/ (test/local).
// Usage : bun static-server.ts [port] [dir]
const port = Number(Bun.argv[2] ?? 8080);
const dir = Bun.argv[3] ?? "dist";

Bun.serve({
  port,
  async fetch(req) {
    const url = new URL(req.url);
    let path = decodeURIComponent(url.pathname);
    if (path === "/" || path.endsWith("/")) path += "index.html";
    const file = Bun.file(`${dir}${path}`);
    if (await file.exists()) return new Response(file);
    // SPA fallback
    return new Response(Bun.file(`${dir}/index.html`));
  },
});

console.log(`static server: http://localhost:${port} (dir=${dir})`);
