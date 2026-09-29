use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ranma::{config, ipc, theme};

/// A tiling window manager for the terminal: i3's tree, Hyprland's dwindle, in a PTY.
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

    /// Run in this terminal only, without a server: closing the terminal ends
    /// it. For tests, and for when a server is not wanted.
    #[arg(long)]
    standalone: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

/// Commands. Without one, ranma attaches this terminal to a server: the most
/// recently used one no terminal shows, or a new one.
#[derive(Subcommand)]
enum Command {
    /// List the ranma servers, and which ones a terminal is showing.
    Ls,
    /// Attach this terminal to the server NAME (taking it from another
    /// terminal showing it), or start one by that name.
    Attach { name: String },
    /// Quit the server NAME and everything in it, without asking.
    Kill { name: String },
    /// Run a server (what `ranma` starts; not for use by hand).
    #[command(hide = true)]
    Server {
        #[arg(long)]
        name: String,
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
        #[arg(long)]
        check: bool,
    },
    /// Run an action, spelled as in a bind: `ranma action "workspace 3"`.
    Action {
        #[arg(required = true)]
        action: Vec<String>,
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
        /// Press Enter after the text.
        #[arg(long, short)]
        enter: bool,
        /// The text (arguments joined by spaces), or the keys.
        #[arg(required = true)]
        input: Vec<String>,
    },
    /// Print a pane's text: its screen, and with --history that many lines of
    /// scrollback above it.
    Capture {
        #[arg(long, short)]
        pane: Option<u64>,
        #[arg(long, short = 'H', default_value_t = 0)]
        history: usize,
    },
    /// Wait for a pane to end, and exit with its program's status.
    Wait {
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
    use ranma::update::{BUILD_SHA, SOURCE_DIR, behind, install_command};
    let dir = std::path::Path::new(SOURCE_DIR);
    match behind(dir, BUILD_SHA, true) {
        Ok(b) if b.commits() == 0 => {
            println!("ranma is up to date ({BUILD_SHA}, from {SOURCE_DIR})");
            if check {
                return ExitCode::SUCCESS;
            }
        }
        Ok(b) => {
            println!(
                "ranma is {} commit(s) behind its source: {} upstream, {} in the checkout ({SOURCE_DIR})",
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
        .arg(install_command())
        .status()
    {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// One argument is a command line as written; several are quoted one by one,
/// the way ssh treats what follows the host.
fn command_line(args: &[String]) -> Option<String> {
    match args {
        [] => None,
        [one] => Some(one.clone()),
        many => Some(
            many.iter()
                .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" "),
        ),
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
            let input = if keys {
                ipc::SendInput::Keys(
                    input
                        .iter()
                        .map(|k| k.parse().map_err(|e| anyhow::anyhow!("key `{k}`: {e}")))
                        .collect::<anyhow::Result<_>>()?,
                )
            } else {
                let mut text = input.join(" ");
                if enter {
                    text.push('\n');
                }
                if paste {
                    ipc::SendInput::Paste(text)
                } else {
                    ipc::SendInput::Text(text)
                }
            };
            ipc::send(&ipc::send_request(pane, &input))?
        }
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
            print,
            command,
        } => {
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
                command: command_line(&command),
                accent,
                beside,
                background,
            }))?;
            if print { id } else { String::new() }
        }
        Command::Update { .. }
        | Command::Ls
        | Command::Kill { .. }
        | Command::Attach { .. }
        | Command::Server { .. } => unreachable!("not a request"),
    };
    print!("{out}");
    Ok(ExitCode::SUCCESS)
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

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    match &cli.command {
        Some(Command::Update { check }) => return update(*check),
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
        println!("ok");
        return ExitCode::SUCCESS;
    }

    // The config was loaded (and so checked) above: an error there exits 1
    // before a server is involved, and a shell startup that runs ranma falls
    // back to a plain shell with the message on screen.
    let result = match (&cli.command, cli.standalone) {
        (Some(Command::Server { name }), _) => ranma::app::run_server(cfg, name),
        (Some(Command::Attach { name }), _) => ranma::client::run(Some(name)).map(|_| ()),
        (_, true) => ranma::app::run(cfg),
        (_, false) => {
            return match ranma::client::run(None) {
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
    use super::command_line;

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

    #[test]
    fn one_argument_is_a_command_line_several_are_quoted() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(command_line(&[]), None);
        assert_eq!(
            command_line(&s(&["ai; exec zsh"])).as_deref(),
            Some("ai; exec zsh")
        );
        assert_eq!(
            command_line(&s(&["echo", "it's", "a b"])).as_deref(),
            Some("'echo' 'it'\\''s' 'a b'")
        );
    }
}
