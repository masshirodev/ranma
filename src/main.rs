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
