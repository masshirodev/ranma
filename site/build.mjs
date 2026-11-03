// Builds ranma.masshiro.pro into site/build/: one index.html and its assets,
// each asset under a content-hashed name so it can be cached for a year while
// the page itself is never cached. No dependencies: what this reads is the
// repository (the release tag, the example config, the committed captures in
// static/media) and what it writes is plain files. doc/WEBSITE.md.
//
//   node build.mjs [--out DIR]

import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, extname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chapters, excerpt, latestRelease } from './lib/data.mjs';
import { lua, swatchCss, toml } from './lib/highlight.mjs';
import { toString } from './lib/html.mjs';
import { page } from './lib/page.mjs';

const site = dirname(fileURLToPath(import.meta.url));
const repo = join(site, '..');
const STATIC = join(site, 'static');

// The code panels the theme swatches sit on: dark and light (site.css).
const CODE_PANELS = ['#18171a', '#f8f5ee'];

/** name.ext → name.<hash>.ext, for a file's bytes. */
export const hashed = (name, bytes) => {
	const h = createHash('sha256').update(bytes).digest('hex').slice(0, 10);
	const ext = extname(name);
	return `${name.slice(0, -ext.length)}.${h}${ext}`;
};

const walk = (dir) =>
	readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
		e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]
	);

/** What the page states, read from the repository. */
export function gather(repoRoot = repo, staticDir = STATIC) {
	const release = latestRelease(repoRoot);
	const read = (p) => readFileSync(join(repoRoot, p), 'utf8');
	const init = excerpt(
		read('doc/examples/screenshot/init.lua'),
		[
			[10, 18],
			[33, 46]
		],
		['ranma.set { theme = "sumi" }', 'ranma.module("cpu"']
	).map((l) => (l.gap ? l : { ...l, html: lua(l.text) }));
	const theme = excerpt(
		read('doc/examples/screenshot/themes/sumi.toml'),
		[
			[1, 16],
			[46, 51]
		],
		['# sumi', '[border]']
	).map((l) => (l.gap ? l : { ...l, html: toml(l.text, CODE_PANELS) }));
	const vtt = join(staticDir, 'media/demo.vtt');
	return {
		...release,
		init,
		theme,
		chapters: existsSync(vtt) ? chapters(readFileSync(vtt, 'utf8')) : []
	};
}

// The captures the page can show, by the names scripts/captures.sh writes.
// Each is optional except the screenshot, which every page needs.
const CAPTURES = {
	poster: 'media/screenshot.webp',
	og: 'media/og.jpg',
	demo: {
		webm: 'media/demo.webm',
		mp4: 'media/demo.mp4',
		vtt: 'media/demo.vtt',
		poster: 'media/demo-poster.webp'
	},
	sessions: { webm: 'media/sessions.webm', mp4: 'media/sessions.mp4', poster: 'media/sessions-poster.webp' },
	copy: { webm: 'media/copy.webm', mp4: 'media/copy.mp4', poster: 'media/copy-poster.webp' },
	images: 'media/images.webp',
	mobile: 'media/mobile.webp'
};

/** Resolve the captures through the hashed-name map; report what's missing. */
export function media(names) {
	const missing = [];
	const pick = (key, spec) => {
		if (typeof spec === 'string') {
			if (names[spec]) return names[spec];
			missing.push(key);
			return null;
		}
		const parts = Object.entries(spec).map(([k, p]) => [k, names[p]]);
		if (parts.every(([, v]) => v)) return Object.fromEntries(parts);
		missing.push(key);
		return null;
	};
	const out = Object.fromEntries(Object.entries(CAPTURES).map(([k, s]) => [k, pick(k, s)]));
	if (!out.poster) throw new Error('static/media/screenshot.webp is missing: run scripts/captures.sh stills');
	out.og ??= out.poster;
	return { media: out, missing: missing.filter((k) => k !== 'og') };
}

export function build(
	out = join(site, 'build'),
	{ repoRoot = repo, staticDir = STATIC, log = console.log } = {}
) {
	const d = gather(repoRoot, staticDir);
	rmSync(out, { recursive: true, force: true });
	mkdirSync(out, { recursive: true });

	// Every static file but the stylesheet first, so the stylesheet can point
	// at the fonts' hashed names; then the stylesheet with the theme's swatches.
	const names = {};
	const write = (rel, bytes) => {
		const name = rel === 'robots.txt' ? rel : hashed(rel, bytes);
		mkdirSync(join(out, dirname(name)), { recursive: true });
		writeFileSync(join(out, name), bytes);
		names[rel] = name;
	};
	for (const file of walk(staticDir)) {
		const rel = relative(staticDir, file);
		if (rel === 'site.css' || basename(rel).startsWith('.')) continue;
		write(rel, readFileSync(file));
	}
	const markup = [...d.init, ...d.theme].map((l) => l.html ?? '').join('\n');
	let css = readFileSync(join(staticDir, 'site.css'), 'utf8').replace(/url\('(fonts\/[^']+)'\)/g, (_, p) => {
		if (!names[p]) throw new Error(`site.css names ${p}, which static/ doesn't have`);
		return `url('${names[p]}')`;
	});
	css += `\n/* The theme excerpt's swatches (build.mjs) */\n${swatchCss(markup)}\n`;
	write('site.css', Buffer.from(css));

	const { media: m, missing } = media(names);
	const fonts = Object.keys(names)
		.filter((n) => n.startsWith('fonts/') && !n.includes('italic'))
		.map((n) => names[n]);
	const index = toString(
		page({
			...d,
			media: m,
			fonts,
			assets: { css: names['site.css'], js: names['site.js'], icon: names['icon.svg'] }
		})
	);
	writeFileSync(join(out, 'index.html'), index);
	for (const key of missing)
		log(`build: no ${key} capture yet; its panel shows no picture (scripts/captures.sh)`);
	log(`build: v${d.version} (${d.date}) → ${relative(process.cwd(), out) || '.'}`);
	return { out, version: d.version, missing };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const i = process.argv.indexOf('--out');
	build(i > 0 ? process.argv[i + 1] : undefined);
}
