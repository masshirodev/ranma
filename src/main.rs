use std::process::ExitCode;

use clap::Parser;
use ranma::{config, theme};

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

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
