// ranma.masshiro.pro, one page. The structure, order and copy are the design
// handoff's (doc/handoffs/website/Main.dc.html); where this departs from it,
// doc/WEBSITE.md says so and why. Styling lives in static/site.css, keyed by
// the classes below; nothing here carries an inline style (the CSP forbids it).
//
// The markup is hand-formatted and Prettier leaves it alone
// (embeddedLanguageFormatting: off): reflowing an html`` template changes the
// page, whitespace inside <pre> included.

import { html, raw } from './html.mjs';

const GH = 'https://github.com/masshirodev/ranma';
export const ONE_LINER = 'curl -fsSL https://masshirodev.github.io/ranma/install.sh | sh';

export const KEYS = [
	['leader t', 'open a pane next to the focused one'],
	['leader ←↓↑→', 'move focus (Alt+arrows without the leader)'],
	['leader q', 'close the focused pane'],
	['leader 1…0', 'go to workspace 1-10 (Alt+1…0)'],
	['leader w', 'float the pane, or put it back'],
	['leader d', 'detach: ranma keeps running'],
	['leader ?', 'every key, searchable'],
	['leader :', 'run any action by name'],
	['leader p', 'the settings panel']
];

export const CHANNELS = [
	['aur', 'Arch (AUR)', 'x86_64, maintained by Rafael Dominiquini', 'ranma-bin'],
	['brew', 'Homebrew', 'macOS, Linux', 'brew install masshirodev/tap/ranma'],
	[
		'nix',
		'Nix',
		'or the flake’s packages.<system>.default / overlays.default',
		'nix run github:masshirodev/ranma'
	],
	['binstall', 'Rust, the binary', 'cargo-binstall', 'cargo binstall ranma'],
	['cargo', 'Rust, built', 'Rust 1.88+ and a C compiler', 'cargo install ranma'],
	['npm', 'npm', 'Node stays running beside each client', 'npm install -g @masshirodev/ranma'],
	['hand', 'By hand', 'tarballs, .deb and .rpm on each release', `${GH}/releases`]
];

export const REPOS = [
	[
		'apt',
		'apt · Debian 12+, Ubuntu 22.04+',
		'Signed; the key and repository stay on GitHub Pages.',
		[
			'sudo install -d /etc/apt/keyrings',
			'curl -fsSL https://masshirodev.github.io/ranma/ranma.asc | sudo tee /etc/apt/keyrings/ranma.asc >/dev/null',
			'echo "deb [signed-by=/etc/apt/keyrings/ranma.asc] https://masshirodev.github.io/ranma/deb stable main" \\',
			'  | sudo tee /etc/apt/sources.list.d/ranma.list',
			'sudo apt update && sudo apt install ranma'
		].join('\n')
	],
	[
		'dnf',
		'dnf · Fedora, RHEL 9+, openSUSE',
		'The last line is openSUSE’s, with zypper.',
		[
			'sudo curl -fsSL -o /etc/yum.repos.d/ranma.repo https://masshirodev.github.io/ranma/ranma.repo',
			'sudo dnf install ranma',
			'# openSUSE:',
			'sudo zypper addrepo https://masshirodev.github.io/ranma/ranma.repo && sudo zypper install ranma'
		].join('\n')
	]
];

const LATTICE = raw(
	'<defs><pattern id="ranma-kumiko" width="44" height="44" patternUnits="userSpaceOnUse"><path d="M22 0L44 22L22 44L0 22Z M22 0V44 M0 22H44 M22 0L11 22L22 44L33 22Z" fill="none" stroke="currentColor" stroke-width="1.2" opacity="0.55"/></pattern></defs>'
);

// A copy button exists only once site.js has wired it: without JavaScript it
// stays hidden, and the command beside it is plain selectable text.
const copyButton = (text, label, cls, name = 'Copy') =>
	html`<button type="button" class="${cls}" data-copy="${text}" aria-label="${label}" hidden>${name}</button>`;

const oneLiner = (cls) => html`
	<div class="${cls}">
		<span class="prompt" aria-hidden="true">❯</span>
		<code>${ONE_LINER}</code>
		${copyButton(ONE_LINER, 'Copy the install command', 'btn-wood')}
	</div>`;

const lines = (excerpt) => raw(excerpt.map((l) => (l.gap ? ' ⋯' : String(l.n))).join('\n'));
const code = (excerpt) =>
	raw(excerpt.map((l) => (l.gap ? '<span class="t-faint">⋯</span>' : l.html)).join('\n'));

const fmtTime = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

// A capture that hasn't been recorded yet leaves its panel without a picture
// rather than show the design's placeholder caption on a public page; the
// build warns about it (build.mjs, "missing").
const loop = (m, alt) =>
	m
		? html`<video class="shot" muted loop playsinline preload="none" data-autoplay
				poster="${m.poster}" width="1200" height="675" aria-label="${alt}">
				<source src="${m.webm}" type='video/webm; codecs="av01.0.08M.08"'>
				<source src="${m.mp4}" type="video/mp4">
			</video>`
		: '';

const still = (src, alt, cls, w, h) =>
	src
		? html`<img class="${cls}" src="${src}" alt="${alt}" width="${w}" height="${h}" loading="lazy" decoding="async">`
		: '';

/** The whole page. `d` is what build.mjs gathered (see its `data`). */
export function page(d) {
	const m = d.media;
	return html`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ranma: a tiling window manager for the terminal</title>
<meta name="description" content="A tiling window manager for the terminal: i3's container tree and Hyprland's dwindle placement, where every window is a PTY. Configured in Lua, themed in TOML.">
<meta name="color-scheme" content="dark light">
<meta property="og:title" content="ranma: a tiling window manager for the terminal">
<meta property="og:description" content="i3's tree, Hyprland's dwindle, every window a PTY. Configured in Lua, themed in TOML.">
<meta property="og:image" content="https://ranma.masshiro.pro/${m.og}">
<meta property="og:url" content="https://ranma.masshiro.pro/">
<link rel="icon" href="${d.assets.icon}" type="image/svg+xml">
${d.fonts.map((f) => html`<link rel="preload" href="${f}" as="font" type="font/woff2" crossorigin>`)}
<link rel="stylesheet" href="${d.assets.css}">
<script src="${d.assets.js}"></script>
</head>
<body>
<a class="skip" href="#top">Skip to the page</a>
<header class="bar">
<nav class="wrap bar-nav" aria-label="Sections">
	<a class="pill pill-brand" href="#top"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="1" y="1" width="14" height="14" rx="2"/><path d="M1 6h14M5.5 1v5M10.5 1v5M3 1l2.5 5M13 1l-2.5 5"/></svg>ranma</a>
	<div class="workspaces">
		<a href="#demo">1 demo</a>
		<a href="#features">2 features</a>
		<a href="#config">3 config</a>
		<a href="#why">4 why</a>
		<a href="#install">5 install</a>
	</div>
	<div class="spacer"></div>
	<a class="pill" href="${GH}/releases/latest" aria-label="Latest release, version ${d.version}">v${d.version}</a>
	<a class="pill" href="${GH}">GitHub</a>
	<button type="button" class="pill theme-toggle" hidden><svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="8" cy="8" r="6.5"/><path d="M8 1.5a6.5 6.5 0 0 0 0 13z" fill="currentColor"/></svg><span class="theme-label">theme</span></button>
</nav>
</header>

<main id="top">
<section class="wrap hero">
	<div class="hero-lead">
		<p class="eyebrow">Linux · macOS · Android in Termux</p>
		<h1>A tiling window manager for the terminal.</h1>
		<p class="lede">i3's container tree and Hyprland's dwindle placement, where every window is a PTY. Sessions, workspaces, a floating layer and a scratchpad, driven by a leader key and a modal WM mode, configured in Lua and themed in TOML.</p>
	</div>
	<div class="hero-name">
		<span class="kanji" lang="ja">欄間</span>
		<p><strong>ranma</strong>, named for the 欄間: the carved transom panel above sliding doors. A lattice framed in wood, with light coming through it.</p>
	</div>
</section>

<div class="wrap">
	${oneLiner('oneliner')}
	<p class="oneliner-note">Run it again to update. Or apt, dnf, the AUR, Homebrew, Nix, cargo or npm: <a class="wood" href="#install">every channel</a>.</p>
</div>

<section id="demo" class="wrap demo" aria-label="ranma in motion">
	<div class="doorway">
		<div class="transom"><svg width="100%" height="88" aria-hidden="true">${LATTICE}<rect x="0" y="0" width="100%" height="88" fill="url(#ranma-kumiko)"/><line x1="0" y1="1" x2="100%" y2="1" stroke="currentColor" stroke-width="3"/></svg></div>
		${
			m.demo
				? html`<video id="demo-video" class="demo-video" controls muted loop playsinline preload="metadata" data-autoplay
				poster="${m.demo.poster}" width="1600" height="900"
				aria-label="ranma in motion: nvim, btop and a git log tiled by the dwindle layout, the leader stepping through layouts, a float over the tiling, a second workspace and back, a toast from ranma notify, and the which-key hint WM mode shows when it waits">
				<source src="${m.demo.webm}" type='video/webm; codecs="av01.0.08M.08"'>
				<source src="${m.demo.mp4}" type="video/mp4">
				<track kind="chapters" src="${m.demo.vtt}" srclang="en" label="Chapters" default>
			</video>`
				: html`<img class="demo-video" src="${m.poster}" width="2560" height="1520" alt="ranma: nvim in a large pane beside a shell running tests and a tail of pacman.log, the bar on top with workspaces, the focused workspace's panes and system modules">`
		}
	</div>
	${
		d.chapters.length && m.demo
			? html`<ol class="chapters" aria-label="Chapters">${d.chapters.map(
					(c, i) =>
						html`<li><a href="${m.demo.mp4}#t=${c.start}" data-seek="${c.start}"><span class="dim" aria-label="Chapter ${i + 1}, at ${fmtTime(c.start)}">${i + 1}</span>${c.label}</a></li>`
				)}</ol>`
			: ''
	}
</section>

<section class="wrap section-keys" aria-labelledby="keys-h">
	<div class="keys">
		<div class="keys-intro">
			<h2 id="keys-h">The leader is <kbd>Ctrl+b</kbd>, the key tmux uses.</h2>
			<p>Press it and let go, and ranma enters <strong>WM mode</strong>: the keys you press now go to ranma instead of the program in the pane. Wait half a second and a panel lists what each key does. <code>Esc</code> leaves it.</p>
			<p>The leader is a setting, like everything else: <code class="hi">ranma.set { leader = "ctrl+a" }</code>. <code>man ranma-keys</code> lists the rest.</p>
		</div>
		<div class="pane pane-focus keys-pane">
			<span class="pane-title">1 · first steps</span>
			<dl>${KEYS.map(([k, v]) => html`<div><dt>${k}</dt><dd>${v}</dd></div>`)}</dl>
		</div>
	</div>
</section>

<section id="features" class="wrap section" aria-labelledby="features-h">
	<h2 id="features-h">What it does</h2>
	<p class="intro">A daily driver: every milestone of the roadmap is done. Six panels, the way the README groups them.</p>
	<div class="grid-480 features">
		<article class="pane pane-focus">
			<span class="pane-title">1 · tiling</span>
			<h3>Panes place themselves.</h3>
			<p>Dwindle splits the focused pane along its longer side, like Hyprland. Or group panes into tabs, float one over the tiling, keep a scratchpad, or lay a workspace out as niri's strip of scrolling columns. Window rules and saved layouts on top.</p>
			<div class="layouts">
				<figure><div class="mini mini-dwindle" aria-hidden="true"><i></i><i></i><i></i><i></i></div><figcaption>dwindle</figcaption></figure>
				<figure><div class="mini mini-tabbed" aria-hidden="true"><i></i><i></i><i></i><i></i></div><figcaption>tabbed</figcaption></figure>
				<figure><div class="mini mini-floating" aria-hidden="true"><i></i><i></i><i></i></div><figcaption>floating</figcaption></figure>
				<figure><div class="mini mini-scratchpad" aria-hidden="true"><i></i><i></i></div><figcaption>scratchpad</figcaption></figure>
				<figure><div class="mini mini-scrolling" aria-hidden="true"><i></i><i></i><i></i></div><figcaption>scrolling</figcaption></figure>
			</div>
		</article>
		<article class="pane">
			<span class="pane-title">2 · sessions</span>
			<h3>Close the terminal. Nothing closes.</h3>
			<p>ranma runs as a server: closing the terminal, or losing SSH, only detaches, and <code class="hi">ranma</code> again picks up where you were. Workspaces and sessions with fuzzy switchers. An upgrade moves running servers to the new build without closing anything.</p>
			${loop(m.sessions, 'leader d detaches and the terminal is back at its shell; ranma again brings the session back where it was, then the session switcher')}
		</article>
		<article class="pane">
			<span class="pane-title">3 · copy mode</span>
			<h3>Copy from history, over SSH too.</h3>
			<p>Copy mode is alacritty's vi mode, with history search. What you yank reaches the system clipboard by OSC 52, from a pane on the far side of an SSH connection as well. Files paste into a pane that runs ssh.</p>
			${loop(m.copy, 'copy mode searches back through a git log, selects a line and yanks it to the clipboard')}
		</article>
		<article class="pane">
			<span class="pane-title">4 · images and keys</span>
			<h3>The kitty protocols, both of them.</h3>
			<p>Images draw in panes with the kitty graphics protocol. The kitty keyboard protocol works both ways, so a program asking for exact keys gets them. Outside WM mode bytes pass through raw; a paste is trusted to bracketed paste, not guessed.</p>
			${still(m.images, 'An image drawn in a ranma pane with the kitty graphics protocol, tiled beside a shell', 'shot', 1200, 675)}
		</article>
		<article class="pane">
			<span class="pane-title">5 · the bar</span>
			<h3>waybar's shape, without waybar's config.</h3>
			<p>Modules in Lua or shell, left, centre and right. Run ranma inside ranma and the nested ones share the outer bar rather than stack their own. Toasts from <code class="hi">ranma notify</code> when a build finishes.</p>
			<div class="bar-shot">
				<img class="bar-strip" src="${m.poster}" width="2560" height="1520" loading="lazy" decoding="async" alt="The bar from the README screenshot: a ranma label, workspaces 1 nvim, 2 zsh, 3 vps, the focused workspace's panes, and cpu, memory, host and clock modules">
				<img class="bar-detail" src="${m.poster}" width="2560" height="1520" loading="lazy" decoding="async" alt="">
			</div>
			<p class="caption">From the screenshot's configuration, below.</p>
		</article>
		<article class="pane">
			<span class="pane-title">6 · mobile</span>
			<h3>A phone can drive it.</h3>
			<p>While a phone drives the screen, ranma switches to a mobile view built from scriptable pieces. And it runs on the phone itself, in Termux.</p>
			${m.mobile ? html`<div class="mobile-row">${still(m.mobile, "ranma's mobile view on a phone, portrait", 'phone-shot', 390, 844)}</div>` : ''}
		</article>
	</div>
	<div class="plugins">
		<h3>plugins</h3>
		<p>Plain Lua on the same API as <code class="hi">init.lua</code>, in Neovim's layout. <a href="https://github.com/masshirodev/hako.ranma">hako</a> comes with ranma and installs them; <code class="hi">leader L</code>, then <code class="hi">B</code>, browses the <a href="https://github.com/masshirodev/ranma-plugins">official list</a>. A plugin that fails while loading is dropped whole; a watchdog stops one that hangs.</p>
	</div>
</section>

<section id="config" class="wrap section" aria-labelledby="config-h">
	<h2 id="config-h">Configured in Lua, themed in TOML.</h2>
	<p class="intro wide">The configuration behind the screenshot, as it is in <a href="${GH}/tree/main/doc/examples/screenshot">doc/examples/screenshot</a>. Lua for behaviour, TOML for looks. Both are parsed strictly: a name ranma doesn't know is an error that names it.</p>
	<div class="grid-480 code-files">
		<figure class="pane pane-focus code-file">
			<figcaption class="pane-title">1 · init.lua</figcaption>
			<div class="code-scroll"><pre class="gutter" aria-hidden="true">${lines(d.init)}</pre><pre><code>${code(d.init)}</code></pre></div>
		</figure>
		<figure class="pane code-file">
			<figcaption class="pane-title">2 · themes/sumi.toml</figcaption>
			<div class="code-scroll"><pre class="gutter" aria-hidden="true">${lines(d.theme)}</pre><pre><code>${code(d.theme)}</code></pre></div>
		</figure>
	</div>
	<div class="config-notes">
		<p><code class="hi">ranma --dump-config</code> prints the default, with every option documented.</p>
		<p><code class="hi">ranma --check-config</code> checks yours and its theme.</p>
		<p><a class="wood" href="${GH}/blob/main/doc/CONFIG.md">CONFIG.md</a>, also <code>man 5 ranma</code></p>
	</div>
</section>

<section id="why" class="wrap section" aria-labelledby="why-h">
	<h2 id="why-h">Why not tmux, zellij or tuios?</h2>
	<p class="intro wide">They are good, and you may want one of them instead. ranma gets where it is mostly by refusing scope, not by out-engineering anyone.</p>
	<div class="others">
		<div class="other">
			<h3>tmux</h3>
			<p>Use tmux where it is already installed on every machine you touch. ranma keeps its leader, its five preset layouts and the things people style in it, and adds a window manager's tree: dwindle placement, a floating layer, a scratchpad. A tmux shim covers a stated subset for programs that drive tmux; matching tmux feature for feature is a non-goal.</p>
		</div>
		<div class="other">
			<h3>zellij</h3>
			<p>zellij is a whole workspace with its own interface and WebAssembly plugins. ranma takes its shape from tiling window managers instead: i3's tree, Hyprland's placement, niri's columns. Its plugins are plain Lua on the same API as your init.lua.</p>
		</div>
		<div class="other">
			<h3>tuios</h3>
			<p>tuios is the model for the feel: a terminal that behaves like a window manager. ranma makes different trade-offs and keeps its scope small:</p>
			<ul>
				<li>one process, one copy of each grid, queues bounded by bytes</li>
				<li>a server that owns everything, and a client that holds nothing</li>
				<li>bytes pass through raw outside WM mode; bracketed paste is trusted, not guessed</li>
			</ul>
		</div>
	</div>
	<div class="pane pane-shu refuses">
		<span class="pane-title">what ranma refuses to be</span>
		<ul>
			<li><strong>Not reboot-proof.</strong> <span>A snapshot brings back sessions, layouts and commands as new processes; the old ones are gone.</span></li>
			<li><strong>Not remote.</strong> <span>No remote hosts, SSH or web servers, no clients keeping copies of the layout in step.</span></li>
			<li><strong>No AI in the core.</strong> <span>A Lua hook can do that for someone who wants it.</span></li>
			<li><strong>Not native Windows.</strong> <span>WSL2 is the Windows story. Not the BSDs yet either.</span></li>
			<li><strong>Not tmux.</strong> <span>What it took from tmux it took in its own terms.</span></li>
		</ul>
		<p class="refuses-note"><a href="${GH}/blob/main/doc/DESIGN.md">DESIGN.md</a> records every decision and what it rules out.</p>
	</div>
</section>

<section id="install" class="wrap section" aria-labelledby="install-h">
	<div class="install-head">
		<h2 id="install-h">Install</h2>
		<a class="wood mono-14" href="${GH}/releases/tag/${d.tag}">v${d.version} · ${d.date} · changelog</a>
	</div>
	<p class="intro wide">Linux (x86_64, aarch64), macOS (Apple silicon, Intel) and Android in Termux (aarch64), into <code>~/.local</code>. Run it again to update; it checks the new binary against your config and moves running servers onto it.</p>
	${oneLiner('oneliner oneliner-big')}
	<h3 class="channels-h">Or with a package manager</h3>
	<p class="channels-note">Every channel ships the same release binary, except cargo install and Nix, which build it.</p>
	<ul class="channels">${CHANNELS.map(
		([, where, note, cmd]) => html`<li>
			<div class="channel-name"><div>${where}</div><div class="dim">${note}</div></div>
			<div class="channel-cmd"><code>${cmd}</code>${copyButton(cmd, `Copy: ${where}`, 'btn-line')}</div>
		</li>`
	)}</ul>
	<div class="grid-480 repos">${REPOS.map(
		([, title, note, cmd]) => html`<figure class="pane repo">
			<figcaption class="pane-title">${title}</figcaption>
			<pre><code>${cmd}</code></pre>
			<div class="repo-foot"><span class="dim">${note}</span>${copyButton(cmd, `Copy the ${title} commands`, 'btn-line btn-mono', 'Copy all')}</div>
		</figure>`
	)}</div>
	<div class="then">
		<div>
			<h3>Then</h3>
			<p>Start it with <code class="hi">ranma</code>, and press <code class="hi">Ctrl+b ?</code> for every key. The man pages come with every install: <code>man ranma</code>, <code>man 5 ranma</code>, <code>man ranma-keys</code>.</p>
		</div>
		<div>
			<h3>After a package-manager upgrade</h3>
			<p>A package manager replaces the binary but doesn't check it or move servers. Shells and scrollback stay: <code class="hi">ranma --check-config &amp;&amp; ranma upgrade --all</code></p>
		</div>
		<div>
			<h3>Platforms</h3>
			<p>Linux with glibc 2.34+ (musl builds from source), WSL2, macOS arm64 and Intel, Termux on aarch64. Not the BSDs or native Windows. <a href="${GH}#platforms">Details</a></p>
		</div>
	</div>
</section>
</main>

<footer class="foot">
	<div class="foot-lattice" aria-hidden="true"><svg width="100%" height="24"><rect x="0" y="0" width="100%" height="24" fill="url(#ranma-kumiko)"/></svg></div>
	<div class="wrap foot-body">
		<div class="foot-about">
			<p class="foot-name">ranma <span lang="ja">欄間</span></p>
			<p>v${d.version}, released ${d.date}. Licensed under MIT or Apache-2.0, at your option. Made by <a href="https://github.com/masshirodev">masshirodev</a>.</p>
			<p>Plain files: no analytics, no trackers, no third-party anything.</p>
		</div>
		<nav class="foot-links" aria-label="Project links">
			<a href="${GH}">GitHub</a>
			<a href="${GH}/releases">Releases</a>
			<a href="${GH}/blob/main/CHANGELOG.md">Changelog</a>
			<a href="${GH}/tree/main/doc/man">Man pages</a>
			<a href="${GH}/blob/main/doc/CONFIG.md">CONFIG.md</a>
			<a href="${GH}/blob/main/doc/DESIGN.md">DESIGN.md</a>
			<a href="https://github.com/masshirodev/ranma-plugins">Plugins</a>
			<a href="${GH}/blob/main/LICENSE-MIT">Licence</a>
		</nav>
	</div>
</footer>
</body>
</html>
`;
}
