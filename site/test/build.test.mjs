import assert from 'node:assert/strict';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync, cpSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { build, gather, hashed, media } from '../build.mjs';

const site = fileURLToPath(new URL('..', import.meta.url));
const scratch = mkdtempSync(join(tmpdir(), 'ranma-site-test-'));
after(() => rmSync(scratch, { recursive: true, force: true }));

test('hashed names a file by its bytes', () => {
	const a = hashed('fonts/x.woff2', Buffer.from('a'));
	assert.match(a, /^fonts\/x\.[0-9a-f]{10}\.woff2$/);
	assert.equal(a, hashed('fonts/x.woff2', Buffer.from('a')));
	assert.notEqual(a, hashed('fonts/x.woff2', Buffer.from('b')));
});

test('gather reads the release and both excerpts from the repository', () => {
	const d = gather();
	assert.match(d.version, /^\d+\.\d+\.\d+/);
	assert.equal(d.init[0].n, 10);
	assert.match(d.init[0].html, /t-api/);
	assert.ok(d.init.some((l) => l.gap));
	assert.equal(d.theme.at(-1).n, 51);
	assert.ok(Array.isArray(d.chapters));
});

test('media resolves captures, reports the missing ones, and needs the screenshot', () => {
	const names = {
		'media/screenshot.webp': 'media/screenshot.1.webp',
		'media/sessions.webm': 'a',
		'media/sessions.mp4': 'b',
		'media/sessions-poster.webp': 'c',
		'media/copy.webm': 'd'
	};
	const { media: m, missing } = media(names);
	assert.equal(m.poster, 'media/screenshot.1.webp');
	assert.equal(m.og, m.poster, 'the link preview falls back to the screenshot');
	assert.deepEqual(m.sessions, { webm: 'a', mp4: 'b', poster: 'c' });
	assert.equal(m.copy, null, 'half a capture is no capture');
	assert.deepEqual(missing.sort(), ['copy', 'demo', 'images', 'mobile']);
	assert.throws(() => media({}), /screenshot\.webp is missing/);
});

test('build writes a page whose every asset exists, hashed', () => {
	const out = join(scratch, 'out');
	const lines = [];
	const r = build(out, { log: (l) => lines.push(l) });
	const index = readFileSync(join(out, 'index.html'), 'utf8');
	assert.ok(lines.at(-1).includes(`v${r.version}`));
	const refs = [...index.matchAll(/(?:src|href)="((?:fonts|media)\/[^"#]+|site\.[^"]+|icon\.[^"]+)"/g)].map(
		(m) => m[1]
	);
	assert.ok(refs.length >= 5);
	for (const ref of refs) assert.doesNotThrow(() => readFileSync(join(out, ref)), ref);
	const css = readdirSync(out).find((f) => /^site\.[0-9a-f]{10}\.css$/.test(f));
	const sheet = readFileSync(join(out, css), 'utf8');
	for (const [, font] of sheet.matchAll(/url\('([^']+)'\)/g)) readFileSync(join(out, font));
	assert.match(sheet, /\.sw-d9a55b\{background:#d9a55b\}/);
	assert.ok(readdirSync(out).includes('robots.txt'), 'robots.txt keeps its name');
	assert.doesNotMatch(index, /style="/, 'no inline style: the CSP forbids it');
});

test('build refuses a stylesheet that names a font it does not have', () => {
	const staticDir = join(scratch, 'static');
	cpSync(join(site, 'static'), staticDir, { recursive: true });
	writeFileSync(join(staticDir, 'site.css'), "@font-face { src: url('fonts/gone.woff2') }");
	mkdirSync(join(staticDir, 'media'), { recursive: true });
	assert.throws(() => build(join(scratch, 'bad'), { staticDir, log: () => {} }), /names fonts\/gone\.woff2/);
});

test('with every capture, the page has the video, its chapters, the loops and the stills', async () => {
	const { page } = await import('../lib/page.mjs');
	const { toString } = await import('../lib/html.mjs');
	const clip = (n) => ({ webm: `media/${n}.webm`, mp4: `media/${n}.mp4`, poster: `media/${n}.webp` });
	const markup = toString(
		page({
			...gather(),
			chapters: [
				{ start: 0, label: 'tiles' },
				{ start: 75, label: 'a toast' }
			],
			fonts: ['fonts/a.woff2'],
			assets: { css: 'site.css', js: 'site.js', icon: 'icon.svg' },
			media: {
				poster: 'media/shot.webp',
				og: 'media/og.jpg',
				demo: { ...clip('demo'), vtt: 'media/demo.vtt' },
				sessions: clip('sessions'),
				copy: clip('copy'),
				images: 'media/images.webp',
				mobile: 'media/mobile.webp'
			}
		})
	);
	assert.match(markup, /<video id="demo-video"[^>]+poster="media\/demo\.webp"/);
	assert.match(markup, /<track kind="chapters" src="media\/demo\.vtt"/);
	assert.match(
		markup,
		/href="media\/demo\.mp4#t=75" data-seek="75"><span class="dim" aria-label="Chapter 2, at 1:15">2<\/span>a toast/
	);
	assert.equal(markup.match(/<video class="shot"/g).length, 2);
	assert.match(markup, /src="media\/images\.webp"/);
	assert.match(markup, /class="mobile-row"/);
	assert.match(markup, /og:image" content="https:\/\/ranma\.masshiro\.pro\/media\/og\.jpg"/);
});
