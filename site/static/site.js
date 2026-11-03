// ranma.masshiro.pro: the few things that need a script. The page is whole
// without it: the theme follows the system, every command is selectable text,
// and the demo has its own controls. This only adds to that.
//
// Loaded in <head> on purpose: the saved theme is applied before the first
// paint, so a visitor who chose light never sees a flash of dark.
(() => {
	const root = document.documentElement;
	const KEY = 'ranma-theme';
	const stored = () => {
		try {
			return localStorage.getItem(KEY);
		} catch {
			return null; // private window, blocked storage: follow the system
		}
	};
	const saved = stored();
	if (saved === 'light' || saved === 'dark') root.dataset.theme = saved;

	const current = () =>
		root.dataset.theme || (matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark');

	document.addEventListener('DOMContentLoaded', () => {
		// The theme toggle exists only here, so without a script there is no
		// control that does nothing.
		const toggle = document.querySelector('.theme-toggle');
		if (toggle) {
			const label = toggle.querySelector('.theme-label');
			const show = () => {
				const mode = current();
				label.textContent = mode;
				toggle.setAttribute('aria-label', `Switch to ${mode === 'dark' ? 'light' : 'dark'} theme`);
			};
			toggle.addEventListener('click', () => {
				const next = current() === 'dark' ? 'light' : 'dark';
				root.dataset.theme = next;
				try {
					localStorage.setItem(KEY, next);
				} catch {
					// the choice lasts this page only
				}
				show();
			});
			matchMedia('(prefers-color-scheme: light)').addEventListener('change', show);
			show();
			toggle.hidden = false;
		}

		// Copy buttons, likewise only shown once they can work.
		if (navigator.clipboard) {
			for (const button of document.querySelectorAll('[data-copy]')) {
				const name = button.textContent;
				let timer;
				button.addEventListener('click', async () => {
					try {
						await navigator.clipboard.writeText(button.dataset.copy);
						button.textContent = 'Copied';
					} catch {
						button.textContent = 'Select it';
					}
					clearTimeout(timer);
					timer = setTimeout(() => (button.textContent = name), 1600);
				});
				button.hidden = false;
			}
		}

		// Videos play while they're on screen, unless the visitor asked for less
		// motion; then they wait for a press of play.
		const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
		const videos = document.querySelectorAll('video[data-autoplay]');
		if (!still && 'IntersectionObserver' in window) {
			const seen = new IntersectionObserver(
				(entries) => {
					for (const e of entries) {
						if (e.isIntersecting) e.target.play().catch(() => {});
						else e.target.pause();
					}
				},
				{ threshold: 0.25 }
			);
			videos.forEach((v) => seen.observe(v));
		}

		// Chapter chips seek the demo instead of opening the file.
		const demo = document.getElementById('demo-video');
		for (const chip of document.querySelectorAll('[data-seek]')) {
			chip.addEventListener('click', (event) => {
				if (!demo) return;
				event.preventDefault();
				demo.currentTime = Number(chip.dataset.seek);
				demo.play().catch(() => {});
			});
		}

		// The bar's workspace pills mark the section on screen, the way ranma's
		// bar marks the focused workspace.
		const pills = new Map([...document.querySelectorAll('.workspaces a')].map((a) => [a.hash.slice(1), a]));
		if ('IntersectionObserver' in window) {
			const visible = new Set();
			const mark = () => {
				const first = [...pills.keys()].find((id) => visible.has(id));
				for (const [id, a] of pills) {
					if (id === first) a.setAttribute('aria-current', 'true');
					else a.removeAttribute('aria-current');
				}
			};
			const spy = new IntersectionObserver(
				(entries) => {
					for (const e of entries) {
						if (e.isIntersecting) visible.add(e.target.id);
						else visible.delete(e.target.id);
					}
					mark();
				},
				{ rootMargin: '-80px 0px -55% 0px' }
			);
			for (const id of pills.keys()) {
				const section = document.getElementById(id);
				if (section) spy.observe(section);
			}
		}
	});
})();
