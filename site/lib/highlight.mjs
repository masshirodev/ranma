// Just enough highlighting for the two files the page quotes, done here so the
// page ships no highlighter. It colours what the design colours (ranma's API
// calls, strings, numbers, keywords, comments, TOML tables) and leaves the
// rest plain; it is not a Lua or TOML parser and does not need to be.

import { esc } from './html.mjs';

const span = (cls, text) => `<span class="${cls}">${esc(text)}</span>`;

const LUA_KEYWORDS = new Set(
	'and break do else elseif end false for function goto if in local nil not or repeat return then true until while'.split(
		' '
	)
);

/** One line of Lua, as markup. */
export function lua(line) {
	let out = '';
	let i = 0;
	while (i < line.length) {
		const rest = line.slice(i);
		let m;
		if (rest.startsWith('--')) {
			out += span('t-comment', rest);
			break;
		} else if ((m = rest.match(/^"(?:\\.|[^"\\])*"?/))) {
			out += span('t-str', m[0]);
		} else if ((m = rest.match(/^ranma(?:\.[A-Za-z_]\w*)+/))) {
			out += span('t-api', m[0]);
		} else if ((m = rest.match(/^[A-Za-z_]\w*/))) {
			out += LUA_KEYWORDS.has(m[0]) ? span('t-kw', m[0]) : esc(m[0]);
		} else if ((m = rest.match(/^\d+(?:\.\d+)?/))) {
			out += span('t-num', m[0]);
		} else {
			m = [rest[0]];
			out += esc(m[0]);
		}
		i += m[0].length;
	}
	return out;
}

// sRGB relative luminance and contrast (WCAG 2), to decide which swatches need
// an outline: a colour that nearly vanishes into either theme's code panel.
const luminance = (hex) => {
	const [r, g, b] = [1, 3, 5].map((k) => {
		const c = parseInt(hex.slice(k, k + 2), 16) / 255;
		return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
export const contrast = (a, b) => {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
};

/**
 * One line of TOML, as markup. `panels` are the code backgrounds the swatch
 * may sit on (dark and light); a colour within 1.6:1 of either is outlined.
 */
export function toml(line, panels) {
	if (/^\s*#/.test(line)) return span('t-comment', line);
	if (/^\s*\[[^\]]+\]\s*$/.test(line)) return span('t-table', line);
	const m = line.match(/^(\s*[A-Za-z0-9_.-]+\s*=\s*)("(?:\\.|[^"\\])*"|\d+(?:\.\d+)?|true|false)(.*)$/);
	if (!m) return esc(line);
	const [, key, value, tail] = m;
	const cls = value.startsWith('"') ? 't-str' : 't-num';
	let out = esc(key) + span(cls, value);
	const hex = value.match(/^"(#[0-9a-fA-F]{6})"$/);
	if (hex) {
		const faint = panels.some((p) => contrast(hex[1], p) < 1.6);
		// A class, not a style attribute: the site's CSP allows no inline style.
		// swatchCss() writes the rule for each one.
		out += ` <span class="swatch sw-${hex[1].slice(1).toLowerCase()}${faint ? ' swatch-edge' : ''}" aria-hidden="true"></span>`;
	}
	return out + esc(tail);
}

/** The CSS rules behind the swatches in some highlighted markup. */
export function swatchCss(markup) {
	const hexes = [...new Set([...markup.matchAll(/\bsw-([0-9a-f]{6})\b/g)].map((m) => m[1]))].sort();
	return hexes.map((h) => `.sw-${h}{background:#${h}}`).join('\n');
}
