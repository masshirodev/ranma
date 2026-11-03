// Serves site/build/ as Caddy does in production: plain files, index.html for
// a directory, the right Content-Type, and nothing else. For the e2e tests
// and for a look before a push (`node scripts/serve.mjs`, then the URL it prints).

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../build/', import.meta.url));
const port = Number(process.env.PORT ?? 4173);
const TYPES = {
	'.html': 'text/html; charset=utf-8',
	'.css': 'text/css; charset=utf-8',
	'.js': 'text/javascript; charset=utf-8',
	'.woff2': 'font/woff2',
	'.webp': 'image/webp',
	'.png': 'image/png',
	'.svg': 'image/svg+xml',
	'.mp4': 'video/mp4',
	'.webm': 'video/webm',
	'.vtt': 'text/vtt; charset=utf-8',
	'.txt': 'text/plain; charset=utf-8'
};

createServer(async (req, res) => {
	const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
	const file = normalize(join(root, path.endsWith('/') ? `${path}index.html` : path));
	if (!file.startsWith(root)) {
		res.writeHead(403).end();
		return;
	}
	try {
		const body = await readFile(file);
		res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' });
		res.end(body);
	} catch {
		res.writeHead(404).end('not found');
	}
}).listen(port, () => console.log(`http://localhost:${port}/`));
