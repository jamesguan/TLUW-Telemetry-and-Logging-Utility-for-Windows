//! Command-line interface for Windows Diagnostics.
//!
//! Thin wrapper around [`windows_diagnostics::telemetry`].

#![cfg(windows)]

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use windows_diagnostics::maintenance;
use windows_diagnostics::telemetry::{
    self, apply, apply_all, ensure_elevated, read_all, read_one, SettingId,
};

#[derive(Parser, Debug)]
#[command(
    name = "windows-diagnostics",
    about = "Inspect and toggle Windows diagnostic data / telemetry (CLI)",
    long_about = "Core commands to disable or enable Windows diagnostic collection.\n\
                  The GUI (windows-diagnostics-gui.exe) is optional and calls the same library."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Skip UAC re-launch (fail instead if not administrator)
    #[arg(long, global = true)]
    no_elevate: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show current ON/OFF status for each setting (default)
    Status,

    /// List setting ids usable with `set` / `explain`
    List,

    /// Print explanation for one setting, or all if omitted
    Explain {
        /// Setting id (see `list`)
        setting: Option<String>,
    },

    /// Turn all telemetry items OFF (original one-shot lockdown)
    #[command(alias = "off", alias = "lockdown")]
    Disable,

    /// Turn all telemetry items ON (Basic diagnostic level where applicable)
    #[command(alias = "on")]
    Enable,

    /// Set one setting on or off
    Set {
        /// Setting id (see `list`)
        setting: String,
        /// on = collecting allowed, off = blocked
        state: OnOff,
    },

    /// Show startup / post-update integration status
    Integration,

    /// Enable or disable run-at-startup (GUI at logon)
    Startup {
        state: OnOff,
    },

    /// Enable or disable re-apply after Windows Update (scheduled task)
    #[command(name = "post-update")]
    PostUpdate {
        state: OnOff,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OnOff {
    On,
    Off,
}

impl OnOff {
    fn active(self) -> bool {
        matches!(self, OnOff::On)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Commands::Status);

    match run(command, cli.no_elevate) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Commands, no_elevate: bool) -> Result<(), String> {
    match command {
        Commands::Status => cmd_status(),
        Commands::List => {
            cmd_list();
            Ok(())
        }
        Commands::Explain { setting } => cmd_explain(setting.as_deref()),
        Commands::Disable => {
            require_admin(no_elevate)?;
            cmd_apply_all(false)
        }
        Commands::Enable => {
            require_admin(no_elevate)?;
            cmd_apply_all(true)
        }
        Commands::Set { setting, state } => {
            require_admin(no_elevate)?;
            let id = SettingId::parse_cli(&setting)?;
            match apply(id, state.active()) {
                Ok(msg) => {
                    println!("{}: {msg}", id.cli_name());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::Integration => {
            let s = maintenance::read_integration();
            println!(
                "run-at-startup: {}",
                if s.run_at_startup { "ON" } else { "OFF" }
            );
            println!(
                "post-update:    {}  (task: {})",
                if s.post_update { "ON" } else { "OFF" },
                maintenance::TASK_NAME
            );
            Ok(())
        }
        Commands::Startup { state } => {
            // HKCU Run does not need admin
            match maintenance::set_run_at_startup(state.active()) {
                Ok(msg) => {
                    println!("{msg}");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::PostUpdate { state } => {
            require_admin(no_elevate)?;
            match maintenance::set_post_update_task(state.active()) {
                Ok(msg) => {
                    println!("{msg}");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    }
}

fn require_admin(no_elevate: bool) -> Result<(), String> {
    if telemetry::is_elevated() {
        return Ok(());
    }
    if no_elevate {
        return Err(
            "administrator rights required (re-run from an elevated prompt, or omit --no-elevate)"
                .into(),
        );
    }
    match ensure_elevated()? {
        true => Ok(()),
        false => {
            // Elevated child already ran with -Wait; parent exits cleanly.
            std::process::exit(0);
        }
    }
}

fn cmd_status() -> Result<(), String> {
    println!(
        "{:<22} {:<6} {}",
        "SETTING", "STATE", "DETAIL"
    );
    println!("{}", "-".repeat(72));
    for s in read_all() {
        let state = if s.active { "ON" } else { "OFF" };
        println!(
            "{:<22} {:<6} {}",
            s.id.cli_name(),
            state,
            s.note
        );
    }
    println!();
    println!("OFF = blocked/privacy   ON = collecting/allowed");
    println!("Tip: windows-diagnostics disable | enable | set <id> on|off");
    Ok(())
}

fn cmd_list() {
    println!("{:<22} {}", "ID", "TITLE");
    println!("{}", "-".repeat(72));
    for id in SettingId::ALL {
        println!("{:<22} {}", id.cli_name(), id.title());
    }
}

fn cmd_explain(setting: Option<&str>) -> Result<(), String> {
    let ids: Vec<SettingId> = match setting {
        Some(name) => vec![SettingId::parse_cli(name)?],
        None => SettingId::ALL.to_vec(),
    };
    for id in ids {
        let state = read_one(id);
        let state_s = if state.active { "ON" } else { "OFF" };
        println!("== {} ({}) — currently {state_s}", id.title(), id.cli_name());
        println!("{}", id.explanation());
        println!("Where: {}", id.detail());
        println!("Now:   {}", state.note);
        println!();
    }
    Ok(())
}

fn cmd_apply_all(active: bool) -> Result<(), String> {
    let label = if active { "ON" } else { "OFF" };
    println!("Applying all settings → {label}...\n");
    let mut failed = false;
    for (id, result) in apply_all(active) {
        match result {
            Ok(msg) => println!("  OK  {:<22} {msg}", id.cli_name()),
            Err(e) => {
                eprintln!("  FAIL {:<22} {e}", id.cli_name());
                failed = true;
            }
        }
    }
    println!();
    if !active {
        println!("Done. A reboot is recommended so all settings stick.");
    } else {
        println!("Done. Telemetry items re-enabled (Basic level where applicable).");
    }
    if failed {
        Err("one or more settings failed".into())
    } else {
        Ok(())
    }
}
