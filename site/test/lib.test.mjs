import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chapters, excerpt, latestRelease } from '../lib/data.mjs';
import { contrast, lua, swatchCss, toml } from '../lib/highlight.mjs';
import { esc, html, raw, toString } from '../lib/html.mjs';
import { seconds, tapeChapters, vtt } from '../lib/tape.mjs';

test('html escapes what it interpolates, and only that', () => {
	const evil = '<script>"&\'';
	assert.equal(esc(evil), '&lt;script&gt;&quot;&amp;&#39;');
	assert.equal(toString(html`<p title="${evil}">${evil}</p>`), `<p title="${esc(evil)}">${esc(evil)}</p>`);
	assert.equal(toString(html`<b>${raw('<i>x</i>')}</b>`), '<b><i>x</i></b>');
	assert.equal(toString(html`${[html`<a>`, 'b&', null, false, undefined, 0]}`), '<a>b&amp;0');
	assert.equal(
		toString(html`<ul>${['a', 'b'].map((x) => html`<li>${x}</li>`)}</ul>`),
		'<ul><li>a</li><li>b</li></ul>'
	);
});

test('latestRelease reads the last v* tag and its day', () => {
	const calls = [];
	const git = (_repo, args) => {
		calls.push(args.join(' '));
		return args[0] === 'describe' ? 'v0.4.0' : '2026-10-16';
	};
	assert.deepEqual(latestRelease('/r', git), { tag: 'v0.4.0', version: '0.4.0', date: '2026-10-16' });
	assert.match(calls[0], /--match v\[0-9\]\*/);
	assert.equal(calls[1], 'log -1 --format=%cs v0.4.0');
});

test('latestRelease refuses a missing tag, an odd one, or no date', () => {
	assert.throws(
		() =>
			latestRelease('/r', () => {
				throw new Error('no names');
			}),
		/fetch-depth: 0/
	);
	assert.throws(() => latestRelease('/r', () => 'nightly'), /unexpected tag/);
	assert.throws(() => latestRelease('/r', (_r, a) => (a[0] === 'describe' ? 'v1.0.0' : '')), /no date/);
});

test('the real repository has a release', () => {
	const r = latestRelease(new URL('../..', import.meta.url).pathname);
	assert.match(r.version, /^\d+\.\d+\.\d+/);
	assert.match(r.date, /^\d{4}-\d{2}-\d{2}$/);
});

test('excerpt quotes numbered ranges with a gap, and refuses lines that moved', () => {
	const text = ['a1', 'b2', 'c3', 'd4', 'e5'].join('\n');
	assert.deepEqual(
		excerpt(
			text,
			[
				[1, 2],
				[4, 5]
			],
			['a', 'd']
		),
		[{ n: 1, text: 'a1' }, { n: 2, text: 'b2' }, { gap: true }, { n: 4, text: 'd4' }, { n: 5, text: 'e5' }]
	);
	assert.throws(() => excerpt(text, [[2, 3]], ['a']), /line 2 reads "b2"/);
	assert.throws(() => excerpt(text, [[4, 9]], ['d']), /outside a 5-line file/);
	assert.throws(() => excerpt(text, [[3, 2]], ['c']), /outside/);
	assert.throws(() => excerpt(text, [[0, 1]], ['a']), /outside/);
});

test('chapters reads a WebVTT chapters track', () => {
	const track =
		'WEBVTT\n\n1\n00:00:00.000 --> 00:00:13.300\nfirst one\n\n2\n00:00:13.300 --> 00:01:02.500\nsecond\n\nNOTE no timing here\n\n3\n00:01:02.500 --> 00:01:03.000\n\n';
	assert.deepEqual(chapters(track), [
		{ start: 0, label: 'first one' },
		{ start: 13.3, label: 'second' }
	]);
	assert.deepEqual(chapters('WEBVTT\n\n00:05.000 --> 00:06.000\nshort stamps'), [
		{ start: 5, label: 'short stamps' }
	]);
	assert.throws(() => chapters('1\n00:00 --> 00:01\nx'), /not a WebVTT/);
});

test('lua colours the API, strings, numbers, keywords and comments', () => {
	assert.equal(
		lua('ranma.set { theme = "sumi" } -- a <note>'),
		'<span class="t-api">ranma.set</span> { theme = <span class="t-str">&quot;sumi&quot;</span> } <span class="t-comment">-- a &lt;note&gt;</span>'
	);
	assert.equal(lua('  interval = 60,'), '  interval = <span class="t-num">60</span>,');
	assert.equal(
		lua('render = function() return x end'),
		'render = <span class="t-kw">function</span>() <span class="t-kw">return</span> x <span class="t-kw">end</span>'
	);
	assert.equal(lua('"a \\" b"'), '<span class="t-str">&quot;a \\&quot; b&quot;</span>');
	assert.equal(lua('"unterminated'), '<span class="t-str">&quot;unterminated</span>');
});

test('toml colours tables, values and comments, and swatches hex colours', () => {
	const panels = ['#18171a', '#f8f5ee'];
	assert.equal(toml('# sumi', panels), '<span class="t-comment"># sumi</span>');
	assert.equal(toml('[colors]', panels), '<span class="t-table">[colors]</span>');
	assert.equal(toml('style = "rounded"', panels), 'style = <span class="t-str">&quot;rounded&quot;</span>');
	assert.equal(toml('width = 2', panels), 'width = <span class="t-num">2</span>');
	assert.equal(toml('not toml at all', panels), 'not toml at all');
	assert.match(toml('a = "#d9a55b"', panels), /class="swatch sw-d9a55b" aria-hidden/);
	// Near the dark panel, and near the light one: both outlined.
	assert.match(toml('a = "#3a3631"', panels), /sw-3a3631 swatch-edge/);
	assert.match(toml('a = "#e8e1d3"', panels), /sw-e8e1d3 swatch-edge/);
});

test('contrast is WCAG 2 contrast', () => {
	assert.equal(contrast('#000000', '#ffffff'), 21);
	assert.equal(contrast('#777777', '#777777'), 1);
});

test('swatchCss writes one rule per colour, once', () => {
	const css = swatchCss(
		'<span class="swatch sw-d9a55b"></span><span class="sw-d9a55b"></span><span class="sw-3a3631"></span>'
	);
	assert.equal(css, '.sw-3a3631{background:#3a3631}\n.sw-d9a55b{background:#d9a55b}');
	assert.equal(swatchCss('none here'), '');
});

test('seconds reads vhs durations', () => {
	assert.equal(seconds('500ms'), 0.5);
	assert.equal(seconds('1.5s'), 1.5);
	assert.equal(seconds('2'), 2);
	assert.equal(seconds('1m'), 60);
	assert.throws(() => seconds('soon'), /not a duration/);
});

test('tapeChapters times each chapter from Show, and scales to the recording', () => {
	const tape = [
		'Set TypingSpeed 100ms',
		'Hide',
		'Type "ignored"',
		'Sleep 9s',
		'Show',
		'# chapter: start',
		'Type "ab"', // 0.2
		'Enter', // 0.1
		'Sleep 1s', // 1
		'# a plain comment',
		'Left@50ms 4', // 0.2
		'Type@10ms "xyz"', // 0.03
		'Ctrl+B', // 0.1
		'# chapter: later',
		'Sleep 500ms',
		'Output somewhere.mp4'
	].join('\n');
	const est = tapeChapters(tape);
	assert.deepEqual(est.chapters, [
		{ start: 0, label: 'start' },
		{ start: 1.6, label: 'later' }
	]);
	assert.ok(Math.abs(est.length - 2.13) < 1e-9);
	const scaled = tapeChapters(tape, 4.26);
	assert.deepEqual(
		scaled.chapters.map((c) => c.start),
		[0, 3.2]
	);
	assert.equal(scaled.length, 4.26);
	assert.throws(() => tapeChapters(tape, 0), /need a length/);
	assert.throws(() => tapeChapters('# chapter: early\nShow'), /before Show/);
	// Hiding again stops the clock; a default TypingSpeed of 50ms applies.
	assert.equal(tapeChapters('Show\nType "ab"\nHide\nSleep 5s\n# x').length, 0.1);
});

test('vtt writes a chapters track the page can read back', () => {
	const track = vtt(
		[
			{ start: 0, label: 'one' },
			{ start: 61.25, label: 'two' }
		],
		3725.5
	);
	assert.equal(
		track,
		'WEBVTT\n\n1\n00:00:00.000 --> 00:01:01.250\none\n\n2\n00:01:01.250 --> 01:02:05.500\ntwo\n'
	);
	assert.deepEqual(chapters(track), [
		{ start: 0, label: 'one' },
		{ start: 61.25, label: 'two' }
	]);
	assert.throws(() => vtt([{ start: 5, label: 'late' }], 5), /no length/);
});

test('the real demo tape has the six chapters the page shows', () => {
	const { readFileSync } = process.getBuiltinModule('node:fs');
	const tape = readFileSync(new URL('../../scripts/demo/demo.tape', import.meta.url), 'utf8');
	const { chapters: c } = tapeChapters(tape);
	assert.equal(c.length, 6);
	assert.equal(c[0].start, 0);
	assert.ok(c.every((x, i) => i === 0 || x.start > c[i - 1].start));
});
