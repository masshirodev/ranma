use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ranma::{config, ipc, theme};

/// A tiling window manager for the terminal: i3's tree, Hyprland's dwindle, in a PTY.
///
/// Without a command, ranma attaches this terminal to a server: the most
/// recently used one no terminal shows, or a new one. Closing the terminal
/// only detaches; `ranma ls` lists the servers and `ranma attach NAME`
/// returns to one.
///
/// Press the leader (ctrl+b) for WM mode, where single keys split, focus,
/// float and move panes; pause there and a hint lists them. ranma-keys(7)
/// lists the default keys and ranma(5) the configuration, read from
/// ~/.config/ranma/init.lua.
#[derive(Parser)]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("RANMA_GIT_SHA"), ")"))]
struct Cli {
    /// Print the built-in default init.lua and exit. It is also the reference for every option.
    #[arg(long, conflicts_with_all = ["check_config", "dump_theme"])]
    dump_config: bool,

    /// With --dump-config: comment out every line, for pasting into your own
    /// init.lua as a reference that changes nothing until you uncomment it.
    #[arg(long, requires = "dump_config")]
    commented: bool,

    /// Load the config and theme, report any error, and exit.
    #[arg(long, conflicts_with = "dump_theme")]
    check_config: bool,

    /// Print the built-in default theme and exit.
    #[arg(long)]
    dump_theme: bool,

    /// Print the Lua API as LuaLS annotations, for an editor to complete and
    /// check `ranma.*` in init.lua and plugins.
    #[arg(long)]
    dump_types: bool,

    /// Run in this terminal only, without a server: closing the terminal ends
    /// it. For tests, and for when a server is not wanted.
    #[arg(long)]
    standalone: bool,

    /// Read a server's handover and check it, adopting nothing: what a server
    /// taking this build asks of it before the exec (not for use by hand).
    #[arg(long, hide = true, value_name = "FILE")]
    check_handover: Option<std::path::PathBuf>,

    /// Write the man pages under DIR (man1/, man5/, man7/): what `doc/man/`
    /// holds, regenerated with `cargo run -- --dump-man doc/man`.
    #[arg(long, hide = true, value_name = "DIR")]
    dump_man: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

/// Commands. Without one, ranma attaches this terminal to a server: the most
/// recently used one no terminal shows, or a new one.
#[derive(Subcommand)]
enum Command {
    /// List the ranma servers, and which ones a terminal is showing.
    Ls,
    /// Attach this terminal to the server NAME, sharing it with any terminal
    /// already showing it, or start one by that name.
    Attach {
        /// Send every other terminal showing it away instead of sharing.
        #[arg(long)]
        steal: bool,
        /// This terminal is a phone or a tablet, as `RANMA_MOBILE=1` says.
        #[arg(long)]
        mobile: bool,
        /// The server's name, as `ranma ls` lists it.
        name: String,
    },
    /// Quit the server NAME and everything in it, without asking.
    Kill {
        /// The server's name, as `ranma ls` lists it.
        name: String,
    },
    /// Run a server (what `ranma` starts; not for use by hand).
    #[command(hide = true)]
    Server {
        #[arg(long)]
        name: String,
        /// Take over from a server that exec'd this build (see `upgrade`).
        #[arg(long)]
        resume: Option<std::path::PathBuf>,
        /// The old build, to go back to if taking over fails.
        #[arg(long, requires = "resume")]
        fallback: Option<std::path::PathBuf>,
    },
    /// Move running servers to the installed build without closing anything:
    /// every shell, pane and scrollback stays. `install.sh` runs `--all`.
    Upgrade {
        /// The server (as `ranma ls` names it); the one this runs in when left out.
        name: Option<String>,
        /// Every server.
        #[arg(long, conflicts_with = "name")]
        all: bool,
    },
    /// Show a toast, e.g. `make && ranma notify "build done"`.
    Notify {
        /// Draw it in the urgent style.
        #[arg(long, short)]
        urgent: bool,
        /// Seconds before it goes (default 5).
        #[arg(long, short)]
        timeout: Option<f64>,
        /// The message.
        #[arg(required = true)]
        text: Vec<String>,
    },
    /// Pull ranma's source and install it; with --check, only say whether there
    /// is anything new.
    Update {
        /// Only say whether the source has anything new; install nothing.
        #[arg(long)]
        check: bool,
    },
    /// Run an action, spelled as in a bind: `ranma action "workspace 3"`.
    Action {
        /// The action and its arguments (joined by spaces), as a bind spells it.
        #[arg(required = true)]
        action: Vec<String>,
    },
    /// Run a command in a float over this pane and print what it prints:
    /// `cd "$(ranma popup -- 'ls ~/projects | fzf')"`. Waits for it, exits with
    /// its status, and gives focus back when it closes.
    Popup {
        /// Width in percent of the workspace.
        #[arg(long, short = 'W', default_value_t = 60)]
        width: u8,
        /// Height in percent of the workspace.
        #[arg(long, short = 'H', default_value_t = 60)]
        height: u8,
        /// Name shown on its border.
        #[arg(long, short)]
        title: Option<String>,
        /// The command. Its stdout is what `ranma popup` prints, so the program
        /// must draw on the terminal itself (as fzf does), not on stdout.
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Run COMMAND (your shell when left out) with a `tmux` on its PATH that
    /// answers in this ranma: `ranma tmux-shim -- claude`. See CONFIG.md.
    TmuxShim {
        /// The command and its arguments, after `--`.
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// The tmux shim asked for by name: `ranma tmux list-panes -F '#{pane_id}'`.
    Tmux {
        /// What tmux would be given: a command and its flags.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// List every pane in every session: id, where it is, what runs in it.
    Panes {
        /// As JSON, one object per pane, for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Type into a pane as if at its keyboard: `ranma send -p 3 -e 'make test'`.
    Send {
        /// The pane (from `ranma panes`); the one this runs in when left out.
        #[arg(long, short)]
        pane: Option<u64>,
        /// Each argument is a key as a bind spells it: `ranma send --keys ctrl+c`.
        #[arg(long, conflicts_with_all = ["paste", "enter"])]
        keys: bool,
        /// Send the text as a paste, bracketed if the program asked for that.
        #[arg(long)]
        paste: bool,
        /// Press Enter after the text, as a key of its own a moment later, so a
        /// program that takes a fast burst for a paste still submits the line.
        #[arg(long, short)]
        enter: bool,
        /// The text (arguments joined by spaces), or the keys.
        #[arg(required = true)]
        input: Vec<String>,
    },
    /// Evaluate Lua in the running ranma's configuration and print what it
    /// returns: `ranma lua 'ranma.state()'`. The code is the arguments joined
    /// by spaces, or stdin when there are none. It runs as a bind does.
    Lua {
        /// The Lua code; stdin when there is none.
        code: Vec<String>,
    },
    /// Say what loaded and what runs: the config, each plugin and whether it
    /// failed, the hooks, timers and jobs.
    Health,
    /// Print a pane's text: its screen, and with --history that many lines of
    /// scrollback above it.
    Capture {
        /// The pane (from `ranma panes`); the one this runs in when left out.
        #[arg(long, short)]
        pane: Option<u64>,
        /// Lines of scrollback to print above the screen.
        #[arg(long, short = 'H', default_value_t = 0)]
        history: usize,
    },
    /// Wait for a pane to end, and exit with its program's status.
    Wait {
        /// The pane (from `ranma panes`); the one this runs in when left out.
        #[arg(long, short)]
        pane: Option<u64>,
    },
    /// Open a pane somewhere, e.g. `ranma open --session ai --workspace empty
    /// --cwd ~/projects/x --name x -- 'ai; exec zsh'`. One argument after `--`
    /// is a command line for the shell; several are a command and its arguments.
    Open {
        /// Switch to this session, creating it if there is none.
        #[arg(long)]
        session: Option<String>,
        /// Then to this workspace: a number, next, prev or empty.
        #[arg(long)]
        workspace: Option<String>,
        /// Name the pane.
        #[arg(long)]
        name: Option<String>,
        /// Name the workspace it lands in.
        #[arg(long)]
        workspace_name: Option<String>,
        /// Start in this directory instead of the focused pane's.
        #[arg(long)]
        cwd: Option<std::path::PathBuf>,
        /// Colour the session it lands in: #rrggbb, 0-255 or an ANSI name.
        #[arg(long)]
        accent: Option<String>,
        /// Open it beside this pane (an id from `ranma panes`), in that pane's
        /// session and workspace, on the --side given.
        #[arg(long, conflicts_with_all = ["session", "workspace"])]
        beside: Option<u64>,
        /// With --beside: left, right, up or down (default right).
        #[arg(long, requires = "beside")]
        side: Option<String>,
        /// Leave focus, the session and the workspace as they are.
        #[arg(long, short = 'd')]
        background: bool,
        /// Float it, centred: width and height in percent of the workspace.
        #[arg(long, num_args = 2, value_names = ["WIDTH", "HEIGHT"])]
        float: Option<Vec<u8>>,
        /// Print the new pane's id.
        #[arg(long, short = 'P')]
        print: bool,
        /// What to run; the shell when left out.
        #[arg(last = true)]
        command: Vec<String>,
    },
}

/// The default config with every line of code commented out. Comments stay as
/// they are, so the result reads like the original but does nothing.
fn commented(src: &str) -> String {
    let mut out = String::from(
        "-- Every option ranma has, at its default, commented out: uncomment a line\n\
         -- (or a whole block) to change it. Generated by `ranma --dump-config\n\
         -- --commented`; rerun it after upgrading to see options added since.\n\n",
    );
    for line in src.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with("--") {
            out.push_str(line);
        } else {
            out.push_str("-- ");
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// `ranma update`: report how far behind the source is, and (without --check)
/// run the same pull-and-install the in-ranma update does.
fn update(check: bool) -> ExitCode {
    use ranma::update::{BUILD_SHA, Source, behind, install_command};
    let source = match Source::current() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ranma: cannot update: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    let shown = source.dir().display().to_string();
    match source.ensure().and_then(|dir| behind(dir, BUILD_SHA, true)) {
        Ok(b) if b.commits() == 0 => {
            println!("ranma is up to date ({BUILD_SHA}, source {shown})");
            if check {
                return ExitCode::SUCCESS;
            }
        }
        Ok(b) => {
            println!(
                "ranma is {} commit(s) behind its source: {} upstream, {} pulled but not installed ({shown})",
                b.commits(),
                b.upstream,
                b.local
            );
            if check {
                return ExitCode::SUCCESS;
            }
        }
        Err(e) => {
            eprintln!("ranma: cannot check for updates: {e:#}");
            if check {
                return ExitCode::FAILURE;
            }
        }
    }
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(install_command(&source))
        .status()
    {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// `ranma upgrade`: ask servers to take the installed build in place. A server
/// from before this existed does not know the request, and is named: it needs
/// one last restart.
fn upgrade(name: Option<&str>, all: bool) -> ExitCode {
    let names: Vec<String> = if all {
        let dir = ipc::server_dir();
        let mut n: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension()? != "sock" {
                    return None;
                }
                Some(p.file_stem()?.to_string_lossy().into_owned())
            })
            .collect();
        n.sort();
        n
    } else if let Some(n) = name {
        vec![n.to_string()]
    } else {
        match std::env::var(ipc::ENV).ok().and_then(|s| {
            std::path::Path::new(&s)
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
        }) {
            Some(n) => vec![n],
            None => {
                eprintln!("ranma: name a server (ranma ls lists them), or --all");
                return ExitCode::FAILURE;
            }
        }
    };
    let mut ok = true;
    for n in names {
        let sock = ipc::server_socket(&n);
        match ipc::send_to(&sock, "upgrade\n") {
            Ok(msg) => print!("{msg}"),
            Err(e) => {
                let e = e.to_string();
                if e.contains("unknown request") {
                    eprintln!(
                        "ranma: server {n} is from before in-place upgrades: restart it once"
                    );
                } else {
                    eprintln!("ranma: server {n}: {e}");
                }
                ok = false;
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A command that is a request to the ranma this runs in, and what it prints.
fn request(cmd: Command) -> anyhow::Result<ExitCode> {
    use anyhow::Context;
    let out = match cmd {
        Command::Notify {
            urgent,
            timeout,
            text,
        } => ipc::send(&ipc::toast_request(&text.join(" "), urgent, timeout))?,
        Command::Action { action } => ipc::send(&format!("action\n{}", action.join(" ")))?,
        Command::Panes { json } => {
            let body = ipc::send("panes\n")?;
            if json {
                body
            } else {
                let panes: Vec<ipc::PaneInfo> =
                    serde_json::from_str(body.trim()).context("reading the pane list")?;
                pane_table(&panes)
            }
        }
        Command::Send {
            pane,
            keys,
            paste,
            enter,
            input,
        } => {
            let pane = ipc::pane_or_own(pane)?;
            let inputs = if keys {
                vec![ipc::SendInput::Keys(
                    input
                        .iter()
                        .map(|k| k.parse().map_err(|e| anyhow::anyhow!("key `{k}`: {e}")))
                        .collect::<anyhow::Result<_>>()?,
                )]
            } else {
                ipc::typed_inputs(input.join(" "), paste, enter)
            };
            let mut reply = String::new();
            for (i, input) in inputs.iter().enumerate() {
                if i > 0 {
                    std::thread::sleep(ipc::ENTER_GAP);
                }
                reply = ipc::send(&ipc::send_request(pane, input))?;
            }
            reply
        }
        Command::Lua { code } => {
            let code = if code.is_empty() {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                    .context("reading Lua from stdin")?;
                s
            } else {
                code.join(" ")
            };
            ipc::send(&format!("lua\n{code}"))?
        }
        Command::Health => ipc::send("health\n")?,
        Command::Capture { pane, history } => {
            let pane = ipc::pane_or_own(pane)?;
            ipc::send(&format!("capture\n{pane}\n{history}\n"))?
        }
        Command::Wait { pane } => {
            let pane = ipc::pane_or_own(pane)?;
            let body = ipc::send(&format!("wait\n{pane}\n"))?;
            // Unknown (killed by a signal, or closed by ranma) counts as failure.
            let code = body.trim().parse::<i32>().unwrap_or(1);
            return Ok(ExitCode::from(code.clamp(0, 255) as u8));
        }
        Command::Open {
            session,
            workspace,
            name,
            workspace_name,
            cwd,
            accent,
            beside,
            side,
            background,
            float,
            print,
            command,
        } => {
            let float = match float.as_deref() {
                None => None,
                Some([w, h]) if [w, h].iter().all(|n| (10..=100).contains(*n)) => Some((*w, *h)),
                Some(_) => anyhow::bail!("--float: width and height are 10-100 percent"),
            };
            let workspace = workspace
                .map(|w| ranma::action::parse_workspace(&w))
                .transpose()
                .map_err(|e| anyhow::anyhow!("--workspace: {e}"))?;
            let accent = accent
                .map(|a| a.parse::<ranma::theme::Color>())
                .transpose()
                .map_err(|e| anyhow::anyhow!("--accent: {e}"))?;
            let beside = match beside {
                None => None,
                Some(p) => {
                    let side = side.as_deref().unwrap_or("right");
                    match format!("new_pane {side}").parse() {
                        Ok(ranma::action::Action::NewPaneAt(d)) => Some((p, d)),
                        _ => anyhow::bail!("--side: `{side}` is not left, right, up or down"),
                    }
                }
            };
            // The pane runs this with the shell, relative paths from here.
            let cwd = cwd.map(|c| std::path::absolute(&c).unwrap_or(c));
            let id = ipc::send(&ipc::open_request(&ipc::OpenSpec {
                session,
                workspace,
                name,
                workspace_name,
                cwd,
                command: ipc::command_line(&command),
                accent,
                beside,
                background,
                float,
                return_focus: false,
                env: Vec::new(),
            }))?;
            if print { id } else { String::new() }
        }
        Command::Tmux { args } => return Ok(ranma::tmux::run(&args)),
        Command::TmuxShim { command } => return ranma::tmux::launch(&command),
        Command::Popup {
            width,
            height,
            title,
            command,
        } => return popup(width, height, title, &command),
        Command::Update { .. }
        | Command::Upgrade { .. }
        | Command::Ls
        | Command::Kill { .. }
        | Command::Attach { .. }
        | Command::Server { .. } => unreachable!("not a request"),
    };
    print!("{out}");
    Ok(ExitCode::SUCCESS)
}

/// `ranma popup`: the command runs in a float with its stdout sent to a file
/// only this user can read; once its pane ends, the file is what we print.
/// The float draws on its own PTY, so fzf and the like show up there while
/// their answer comes back here.
fn popup(
    width: u8,
    height: u8,
    title: Option<String>,
    command: &[String],
) -> anyhow::Result<ExitCode> {
    use std::os::unix::fs::OpenOptionsExt;
    if !(10..=100).contains(&width) || !(10..=100).contains(&height) {
        anyhow::bail!("--width and --height are 10-100 percent");
    }
    let dir = ipc::server_dir();
    std::fs::create_dir_all(&dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let out = dir.join(format!("popup-{}-{nanos}.out", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&out)?;
    let quoted = out.display().to_string().replace('\'', "'\\''");
    let cmd = format!(
        "( {} ) > '{quoted}'",
        ipc::command_line(command).unwrap_or_default()
    );
    let result = (|| {
        let id = ipc::send(&ipc::open_request(&ipc::OpenSpec {
            name: title,
            cwd: std::env::current_dir().ok(),
            command: Some(cmd),
            float: Some((width, height)),
            return_focus: true,
            ..Default::default()
        }))?;
        let id = id.trim();
        let status = ipc::send(&format!("wait\n{id}\n"))?;
        let text = std::fs::read_to_string(&out)?;
        anyhow::Ok((status, text))
    })();
    let _ = std::fs::remove_file(&out);
    let (status, text) = result?;
    print!("{text}");
    let code = status.trim().parse::<i32>().unwrap_or(1);
    Ok(ExitCode::from(code.clamp(0, 255) as u8))
}

/// `ranma panes` for people: one row per pane, `*` on the focused one of each
/// workspace, where it is as session:workspace (`S` for the scratchpad).
fn pane_table(panes: &[ipc::PaneInfo]) -> String {
    let rows: Vec<[String; 4]> = panes
        .iter()
        .map(|p| {
            let ws = if p.workspace == 0 {
                "S".to_string()
            } else {
                p.workspace.to_string()
            };
            [
                format!("{}{}", p.id, if p.focused { "*" } else { "" }),
                format!("{}:{ws}", p.session),
                p.program.clone().unwrap_or_default(),
                p.title.clone(),
            ]
        })
        .collect();
    let head = ["ID", "WHERE", "PROGRAM", "TITLE"].map(String::from);
    let width = |i: usize| {
        std::iter::once(&head)
            .chain(&rows)
            .map(|r| r[i].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (w0, w1, w2) = (width(0), width(1), width(2));
    std::iter::once(&head)
        .chain(&rows)
        .map(|r| {
            format!("{:w0$}  {:w1$}  {:w2$}  {}\n", r[0], r[1], r[2], r[3])
                .trim_end()
                .to_string()
                + "\n"
        })
        .collect()
}

fn dump_man(dir: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    for (path, roff) in ranma::man::pages(<Cli as clap::CommandFactory>::command())? {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&file, roff).with_context(|| format!("writing {}", file.display()))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    // Called as `tmux` (the shim's link): answer, or hand the call on.
    let mut argv = std::env::args();
    let argv0 = argv.next().unwrap_or_default();
    if std::path::Path::new(&argv0).file_name() == Some(std::ffi::OsStr::new("tmux")) {
        return ranma::tmux::main_as_tmux(argv.collect());
    }
    let mut cli = Cli::parse();

    if let Some(dir) = &cli.dump_man {
        return match dump_man(dir) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ranma: {e:#}");
                ExitCode::FAILURE
            }
        };
    }

    if let Some(file) = &cli.check_handover {
        return match ranma::app::check_handover(file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e:#}");
                ExitCode::FAILURE
            }
        };
    }

    match &cli.command {
        Some(Command::Update { check }) => return update(*check),
        Some(Command::Upgrade { name, all }) => return upgrade(name.as_deref(), *all),
        Some(Command::Ls) => return ranma::client::list(),
        Some(Command::Kill { name }) => {
            return match ranma::client::kill(name) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("ranma: {e:#}");
                    ExitCode::FAILURE
                }
            };
        }
        _ => {}
    }

    if let Some(cmd) = cli
        .command
        .take_if(|c| !matches!(c, Command::Attach { .. } | Command::Server { .. }))
    {
        return match request(cmd) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("ranma: {e:#}");
                ExitCode::FAILURE
            }
        };
    }

    if cli.dump_config {
        if cli.commented {
            print!("{}", commented(config::DEFAULT_INIT_LUA));
        } else {
            print!("{}", config::DEFAULT_INIT_LUA);
        }
        return ExitCode::SUCCESS;
    }
    if cli.dump_theme {
        print!("{}", theme::DEFAULT_THEME_SRC);
        return ExitCode::SUCCESS;
    }
    if cli.dump_types {
        print!("{}", ranma::devtools::TYPES);
        return ExitCode::SUCCESS;
    }

    let dir = config::config_dir();
    let cfg = match config::load(dir.as_deref()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("ranma: config error: {e:#}");
            return ExitCode::FAILURE;
        }
    };

    if cli.check_config {
        match &cfg.source {
            Some(p) => println!("config: {}", p.display()),
            None => println!(
                "config: built-in defaults only (no init.lua in {})",
                dir.as_deref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "any config directory".into())
            ),
        }
        println!("leader: {}", cfg.settings.leader);
        println!("theme:  {}", cfg.theme.name);
        println!("binds:  {}", cfg.binds.len());
        println!(
            "hooks:  {}",
            cfg.hooks.values().map(Vec::len).sum::<usize>()
        );
        // A failing plugin is dropped, not fatal: ranma runs without it, so
        // the check still passes (install.sh must not refuse a binary over
        // one plugin), but says so on stderr.
        for p in &cfg.plugins {
            match &p.error {
                None => println!("plugin: {} ({} ms)", p.path.display(), p.took.as_millis()),
                Some(e) => eprintln!("plugin: {} not loaded: {e}", p.path.display()),
            }
        }
        for w in &cfg.warnings {
            eprintln!("warning: {w}");
        }
        println!("ok");
        return ExitCode::SUCCESS;
    }

    // The config was loaded (and so checked) above: an error there exits 1
    // before a server is involved, and a shell startup that runs ranma falls
    // back to a plain shell with the message on screen.
    let result = match (&cli.command, cli.standalone) {
        (
            Some(Command::Server {
                name,
                resume: Some(file),
                fallback,
            }),
            _,
        ) => ranma::app::run_server_resume(cfg, name, file, fallback.as_deref()),
        (Some(Command::Server { name, .. }), _) => ranma::app::run_server(cfg, name),
        (
            Some(Command::Attach {
                name,
                steal,
                mobile,
            }),
            _,
        ) => ranma::client::run(Some(name), *steal, *mobile).map(|_| ()),
        (_, true) => ranma::app::run(cfg),
        (_, false) => {
            return match ranma::client::run(None, false, false) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("ranma: {e:#}");
                    ExitCode::FAILURE
                }
            };
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ranma: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {

    /// Every argument a user can see says what it is: `--help` and the man
    /// pages both show the text, and an empty one reads as an omission.
    #[test]
    fn every_argument_has_help() {
        fn walk(cmd: &clap::Command, path: &str, missing: &mut Vec<String>) {
            for a in cmd.get_arguments() {
                if a.is_hide_set()
                    || a.get_help().is_none() && ["help", "version"].contains(&a.get_id().as_str())
                {
                    continue;
                }
                if a.get_help().is_none() {
                    missing.push(format!("{path} {}", a.get_id()));
                }
            }
            for s in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
                walk(s, &format!("{path} {}", s.get_name()), missing);
            }
        }
        let mut cmd = <super::Cli as clap::CommandFactory>::command();
        cmd.build();
        let mut missing = Vec::new();
        walk(&cmd, "ranma", &mut missing);
        assert!(missing.is_empty(), "no help: {missing:?}");
    }

    /// `doc/man/` is the generated pages, committed: this fails when the CLI,
    /// CONFIG.md or the default binds changed without them.
    #[test]
    fn the_committed_man_pages_are_current() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/man");
        let pages = ranma::man::pages(<super::Cli as clap::CommandFactory>::command()).unwrap();
        let mut stale = Vec::new();
        for (path, roff) in &pages {
            if std::fs::read_to_string(root.join(path)).ok().as_deref() != Some(roff.as_str()) {
                stale.push(path.clone());
            }
        }
        // And nothing left over from a command that is gone.
        for sec in ["man1", "man5", "man7"] {
            for e in std::fs::read_dir(root.join(sec))
                .into_iter()
                .flatten()
                .flatten()
            {
                let rel = format!("{sec}/{}", e.file_name().to_string_lossy());
                if !pages.iter().any(|(p, _)| *p == rel) {
                    stale.push(format!("{rel} (not generated any more)"));
                }
            }
        }
        assert!(
            stale.is_empty(),
            "stale man pages, regenerate with `rm -r doc/man && cargo run -- --dump-man doc/man`: {stale:?}"
        );
    }

    #[test]
    fn commented_defaults_run_as_nothing() {
        let out = super::commented(ranma::config::DEFAULT_INIT_LUA);
        // Every line is a comment or blank: loading it changes nothing.
        assert!(
            out.lines()
                .all(|l| l.trim().is_empty() || l.trim_start().starts_with("--"))
        );
        let cfg = ranma::config::load_from(None, None, Some(&out)).unwrap();
        let defaults = ranma::config::load_from(None, None, None).unwrap();
        assert_eq!(cfg.settings, defaults.settings);
        assert_eq!(cfg.binds.len(), defaults.binds.len());
        // And every setting is in there to uncomment.
        for key in [
            "leader",
            "theme",
            "layout",
            "master_ratio",
            "preserve_split",
            "shell",
            "scrollback_lines",
            "sticky",
            "mouse",
            "updates",
            "update_check_hours",
        ] {
            assert!(
                out.contains(&format!("--   {key} =")) || out.contains(&format!("--     {key} =")),
                "{key}"
            );
        }
    }
}
