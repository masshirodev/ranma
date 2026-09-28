use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ranma::{config, ipc, theme};

/// A tiling window manager for the terminal: i3's tree, Hyprland's dwindle, in a PTY.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Print the built-in default init.lua and exit. It is also the reference for every option.
    #[arg(long, conflicts_with_all = ["check_config", "dump_theme"])]
    dump_config: bool,

    /// Load the config and theme, report any error, and exit.
    #[arg(long, conflicts_with = "dump_theme")]
    check_config: bool,

    /// Print the built-in default theme and exit.
    #[arg(long)]
    dump_theme: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

/// Commands for the ranma this shell runs in (found through RANMA_SOCKET).
#[derive(Subcommand)]
enum Command {
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
    /// Run an action, spelled as in a bind: `ranma action "workspace 3"`.
    Action {
        #[arg(required = true)]
        action: Vec<String>,
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
        /// What to run; the shell when left out.
        #[arg(last = true)]
        command: Vec<String>,
    },
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        let request = match cmd {
            Command::Notify {
                urgent,
                timeout,
                text,
            } => ipc::toast_request(&text.join(" "), urgent, timeout),
            Command::Action { action } => format!("action\n{}", action.join(" ")),
            Command::Open {
                session,
                workspace,
                name,
                workspace_name,
                cwd,
                command,
            } => {
                let workspace = match workspace.map(|w| ranma::action::parse_workspace(&w)) {
                    None => None,
                    Some(Ok(w)) => Some(w),
                    Some(Err(e)) => {
                        eprintln!("ranma: --workspace: {e}");
                        return ExitCode::FAILURE;
                    }
                };
                // The pane runs this with the shell, relative paths from here.
                let cwd = cwd.map(|c| std::path::absolute(&c).unwrap_or(c));
                ipc::open_request(&ipc::OpenSpec {
                    session,
                    workspace,
                    name,
                    workspace_name,
                    cwd,
                    command: command_line(&command),
                })
            }
        };
        return match ipc::send(&request) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ranma: {e:#}");
                ExitCode::FAILURE
            }
        };
    }

    if cli.dump_config {
        print!("{}", config::DEFAULT_INIT_LUA);
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

    match ranma::app::run(cfg) {
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
