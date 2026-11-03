// The page is a template literal, and a template literal escapes nothing. So
// every interpolation goes through `html`, which escapes it unless it was
// built by `html` (or marked `raw`) itself: forgetting to escape is the
// failure that can't happen, not the one a reviewer has to spot.

const RAW = Symbol('raw');

/** Trusted markup, interpolated as it is. */
export const raw = (markup) => ({ [RAW]: String(markup) });

const ENTITIES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

/** Text made safe for an element body or a quoted attribute. */
export const esc = (text) => String(text).replace(/[&<>"']/g, (c) => ENTITIES[c]);

const render = (value) => {
	if (value === null || value === undefined || value === false) return '';
	if (Array.isArray(value)) return value.map(render).join('');
	if (typeof value === 'object' && RAW in value) return value[RAW];
	return esc(value);
};

/** Tagged template: escapes each value, keeps nested `html` as markup. */
export const html = (strings, ...values) =>
	raw(strings.reduce((out, s, i) => out + s + (i < values.length ? render(values[i]) : ''), ''));

/** The markup a `html` result holds, as a string. */
export const toString = (value) => render(value);
