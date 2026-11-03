// The demo's chapters, from the tape that records it. A line `# chapter: X`
// in a vhs tape marks where chapter X starts; its time is what vhs spends on
// the commands before it once `Show` has run: every Sleep, plus the typing
// (each character, and each key press, at the tape's TypingSpeed unless the
// command gives its own with @). Close to what vhs records rather than exact
// (it adds a few milliseconds per command), which is close enough to seek to:
// scripts/captures.sh writes the WebVTT from this, and the page reads that.

const DURATION = /^(\d+(?:\.\d+)?)(ms|s|m)?$/;

/** "500ms", "1.5s", "2" (seconds, as vhs reads a bare number) → seconds. */
export function seconds(text) {
	const m = text.trim().match(DURATION);
	if (!m) throw new Error(`not a duration: ${JSON.stringify(text)}`);
	const n = Number(m[1]);
	return m[2] === 'ms' ? n / 1000 : m[2] === 'm' ? n * 60 : n;
}

// Commands that press a key, optionally repeated: `Enter`, `Left 3`, `Ctrl+B`.
const KEY =
	/^(Enter|Space|Tab|Backspace|Delete|Escape|Up|Down|Left|Right|PageUp|PageDown|Home|End|Insert|(?:Ctrl|Alt|Shift)\+\S+)(?:@(\S+))?(?:\s+(\d+))?$/;

/**
 * [{ start, label }] for every `# chapter:` line, in seconds from Show, and
 * the estimated length of the whole recording. Pass `length` (the recorded
 * video's real duration) and the starts are scaled to it: the estimate's drift
 * is spread across the tape instead of piling up at its end.
 */
export function tapeChapters(tape, length) {
	let speed = 0.05; // vhs's default TypingSpeed
	let shown = false;
	let t = 0;
	const out = [];
	for (const raw of tape.split('\n')) {
		const line = raw.trim();
		let m;
		if ((m = line.match(/^#\s*chapter:\s*(.+)$/))) {
			if (!shown) throw new Error(`chapter ${JSON.stringify(m[1])} comes before Show`);
			out.push({ start: Math.round(t * 10) / 10, label: m[1].trim() });
		} else if (!line || line.startsWith('#')) {
			continue;
		} else if ((m = line.match(/^Set\s+TypingSpeed\s+(\S+)$/))) {
			speed = seconds(m[1]);
		} else if (line === 'Show') {
			shown = true;
			t = 0;
		} else if (line === 'Hide') {
			shown = false;
		} else if (!shown) {
			continue;
		} else if ((m = line.match(/^Sleep\s+(\S+)$/))) {
			t += seconds(m[1]);
		} else if ((m = line.match(/^Type(?:@(\S+))?\s+(["'`])(.*)\2$/))) {
			t += [...m[3]].length * (m[1] ? seconds(m[1]) : speed);
		} else if ((m = line.match(KEY))) {
			t += (m[3] ? Number(m[3]) : 1) * (m[2] ? seconds(m[2]) : speed);
		}
	}
	if (length === undefined) return { chapters: out, length: t };
	if (!(length > 0) || !(t > 0)) throw new Error('a tape and its recording both need a length');
	const k = length / t;
	return { chapters: out.map((c) => ({ ...c, start: Math.round(c.start * k * 10) / 10 })), length };
}

const stamp = (s) => {
	const ms = Math.round(s * 1000);
	const h = Math.floor(ms / 3600000);
	const m = Math.floor((ms % 3600000) / 60000);
	const sec = Math.floor((ms % 60000) / 1000);
	const pad = (n, w = 2) => String(n).padStart(w, '0');
	return `${pad(h)}:${pad(m)}:${pad(sec)}.${pad(ms % 1000, 3)}`;
};

/** A WebVTT chapters track: each chapter runs to the next, the last to `end`. */
export function vtt(chapters, end) {
	const cues = chapters.map((c, i) => {
		const until = i + 1 < chapters.length ? chapters[i + 1].start : end;
		if (until <= c.start) throw new Error(`chapter ${JSON.stringify(c.label)} has no length`);
		return `${i + 1}\n${stamp(c.start)} --> ${stamp(until)}\n${c.label}`;
	});
	return `WEBVTT\n\n${cues.join('\n\n')}\n`;
}
