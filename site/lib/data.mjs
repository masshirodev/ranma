// Everything the page states about ranma that could go stale is read here, at
// build time, from the repository: the release from its tag, the config
// excerpts from doc/examples/screenshot, the demo's chapters from the WebVTT
// the capture script wrote. Nothing on the page is typed by hand twice.

import { execFileSync } from 'node:child_process';

const runGit = (repo, args) => execFileSync('git', ['-C', repo, ...args], { encoding: 'utf8' }).trim();

/**
 * The newest release reachable from HEAD: the last `v*` tag, and the day it
 * was made. The site shows what a visitor can install, which is the tag, not
 * Cargo.toml (that runs ahead of a release).
 */
export function latestRelease(repo, git = runGit) {
	let tag;
	try {
		tag = git(repo, ['describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*']);
	} catch {
		throw new Error(
			'no v* tag is reachable from HEAD; a shallow clone needs fetch-depth: 0 (doc/WEBSITE.md, "Deploying")'
		);
	}
	if (!/^v\d+\.\d+\.\d+/.test(tag)) throw new Error(`unexpected tag ${JSON.stringify(tag)}`);
	const date = git(repo, ['log', '-1', '--format=%cs', tag]);
	if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error(`no date for ${tag}: ${JSON.stringify(date)}`);
	return { tag, version: tag.slice(1), date };
}

/**
 * Lines of a file as the page quotes them: each range [first, last] (1-based,
 * inclusive) with its line numbers, a gap between ranges. `expect` pins the
 * start of each range's first line, so an edit to the example file fails the
 * build instead of quietly quoting the wrong lines.
 */
export function excerpt(text, ranges, expect) {
	const lines = text.split('\n');
	const out = [];
	ranges.forEach(([first, last], i) => {
		if (first < 1 || last < first || last > lines.length)
			throw new Error(`range ${first}-${last} is outside a ${lines.length}-line file`);
		const head = lines[first - 1];
		if (!head.startsWith(expect[i]))
			throw new Error(
				`line ${first} reads ${JSON.stringify(head)}, expected it to start ${JSON.stringify(expect[i])}`
			);
		if (i > 0) out.push({ gap: true });
		for (let n = first; n <= last; n++) out.push({ n, text: lines[n - 1] });
	});
	return out;
}

const toSeconds = (stamp) => {
	const parts = stamp.split(':').map(Number);
	return parts.reduce((total, p) => total * 60 + p, 0);
};

/** The cues of a WebVTT chapters track: start in seconds, and the label. */
export function chapters(vtt) {
	if (!vtt.startsWith('WEBVTT')) throw new Error('not a WebVTT file');
	const cues = [];
	for (const block of vtt.split(/\r?\n\r?\n/).slice(1)) {
		const lines = block.split(/\r?\n/).filter(Boolean);
		const timing = lines.findIndex((l) => l.includes('-->'));
		if (timing < 0) continue;
		const start = toSeconds(lines[timing].split('-->')[0].trim());
		const label = lines
			.slice(timing + 1)
			.join(' ')
			.trim();
		if (label) cues.push({ start, label });
	}
	return cues;
}
