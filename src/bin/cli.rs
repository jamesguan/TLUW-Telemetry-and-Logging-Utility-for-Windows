//! Command-line interface for Telemetry and Logging Utility for Windows.
//!
//! Thin wrapper around [`telemetry_logging_utility::telemetry`].

#![cfg(windows)]

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use telemetry_logging_utility::cleanup_history;
use telemetry_logging_utility::disclaimer;
use telemetry_logging_utility::identity;
use telemetry_logging_utility::log_cleanup;
use telemetry_logging_utility::maintenance;
use telemetry_logging_utility::system_links;
use telemetry_logging_utility::telemetry::{
    self, apply, apply_all, ensure_elevated, read_all, read_one, SettingId,
};
use telemetry_logging_utility::temp_cleanup;

#[derive(Parser, Debug)]
#[command(
    name = "tluw",
    about = "Telemetry and Logging Utility for Windows (CLI)",
    long_about = "Inspect and toggle Windows diagnostic / telemetry settings, and clear logs/temp.\n\
                  The GUI (tluw-gui.exe) is optional and calls the same library.\n\n\
                  USE AT YOUR OWN RISK — AS IS, NO WARRANTY, NO LIABILITY.\n\
                  See `tluw disclaimer` and DISCLAIMER.md."
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

    /// List Windows logging / diagnostics tools this app can open
    #[command(name = "logs", alias = "links")]
    Logs,

    /// Open a Windows logging tool by id (see `logs`)
    Open {
        /// Link id, e.g. event-viewer, privacy-feedback
        id: String,
    },

    /// List clearable log targets (Diagnosis folder, event logs, WER, …)
    #[command(name = "clear-list")]
    ClearList,

    /// Open the folder / Event Viewer for a clear target (see `clear-list`)
    #[command(name = "open-log")]
    OpenLog {
        id: String,
    },

    /// Clear one log target by id (see `clear-list`). Destructive; needs admin for most.
    Clear {
        /// Target id, e.g. diagnosis, event-application, wer
        id: String,
        /// Required for dangerous targets (diagnosis*, event-security)
        #[arg(long)]
        confirm: bool,
    },

    /// Clear all available log targets (add `--dangerous` for Diagnosis / Security)
    #[command(name = "clear-all")]
    ClearAll {
        #[arg(long)]
        dangerous: bool,
        #[arg(long)]
        confirm: bool,
    },

    /// List temp-folder cleanup targets
    #[command(name = "temp-list")]
    TempList,

    /// Open a temp location (see `temp-list`)
    #[command(name = "open-temp")]
    OpenTemp {
        id: String,
    },

    /// Clear one temp target (see `temp-list`)
    #[command(name = "clear-temp")]
    ClearTemp {
        id: String,
        #[arg(long)]
        confirm: bool,
    },

    /// Clear all available temp targets
    #[command(name = "clear-temp-all")]
    ClearTempAll {
        #[arg(long)]
        confirm: bool,
    },

    /// Print the full no-warranty / liability disclaimer
    Disclaimer,

    /// Show daily GB freed by log/temp clears (dashboard history)
    History {
        /// How many recent days to list (default 14)
        #[arg(long, default_value_t = 14)]
        days: usize,
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
    telemetry_logging_utility::bootstrap();

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
                identity::TASK_NAME
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
        Commands::Logs => {
            println!("{:<22} {}", "ID", "TITLE");
            println!("{}", "-".repeat(72));
            for link in system_links::ALL {
                println!("{:<22} {}", link.id, link.title);
                println!("  {}", link.description);
            }
            println!();
            println!("Open one: tluw open <id>");
            Ok(())
        }
        Commands::Open { id } => match system_links::open_id(&id) {
            Ok(msg) => {
                println!("{msg}");
                Ok(())
            }
            Err(e) => Err(e),
        },
        Commands::ClearList => {
            println!("{:<24} {:<6} {}", "ID", "AVAIL", "STATUS");
            println!("{}", "-".repeat(78));
            for a in log_cleanup::ALL {
                let avail = if a.is_available() { "yes" } else { "no" };
                let danger = if a.dangerous { " [dangerous]" } else { "" };
                println!("{:<24} {:<6} {}{danger}", a.id, avail, a.title);
                if a.is_available() {
                    let st = log_cleanup::inspect(a);
                    println!("  {}", st.summary_line());
                } else {
                    println!("  {}", a.description);
                }
            }
            println!();
            println!("Open loc:   tluw open-log <id>");
            println!("Clear one:  tluw clear <id> --confirm");
            println!("Clear safe: tluw clear-all --confirm");
            Ok(())
        }
        Commands::OpenLog { id } => {
            let action = log_cleanup::ClearAction::find(&id)
                .ok_or_else(|| format!("unknown clear target '{id}' (see clear-list)"))?;
            let status = log_cleanup::inspect(action);
            println!("{}", status.summary_line());
            match log_cleanup::open_location(action) {
                Ok(msg) => {
                    println!("{msg}");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::Clear { id, confirm } => {
            let action = log_cleanup::ClearAction::find(&id)
                .ok_or_else(|| format!("unknown clear target '{id}' (see clear-list)"))?;
            if action.dangerous && !confirm {
                return Err(format!(
                    "'{}' is dangerous — re-run with --confirm",
                    action.id
                ));
            }
            if !confirm {
                return Err("refusing to clear without --confirm".into());
            }
            require_admin(no_elevate)?;
            match log_cleanup::clear(action) {
                Ok(result) => {
                    println!("{}", result.summary_line());
                    println!("  before: {}", result.before.summary_line());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::ClearAll {
            dangerous,
            confirm,
        } => {
            if !confirm {
                return Err("refusing to clear-all without --confirm".into());
            }
            require_admin(no_elevate)?;
            let results = log_cleanup::clear_all(dangerous);
            let mut failed = false;
            let mut total_files = 0u64;
            let mut total_bytes = 0u64;
            for (id, r) in results {
                match r {
                    Ok(result) => {
                        total_files += result.removed_files;
                        total_bytes += result.freed_bytes;
                        println!("  OK  {id}: {}", result.summary_line());
                    }
                    Err(e) => {
                        eprintln!("  FAIL {id}: {e}");
                        failed = true;
                    }
                }
            }
            println!(
                "\nTotal: {} item(s), {}",
                total_files,
                log_cleanup::format_bytes(total_bytes)
            );
            if failed {
                Err("one or more clear operations failed".into())
            } else {
                Ok(())
            }
        }
        Commands::TempList => {
            println!("{:<18} {:<6} {}", "ID", "AVAIL", "STATUS");
            println!("{}", "-".repeat(78));
            for t in temp_cleanup::ALL {
                if !t.is_available() {
                    continue; // e.g. TMP / LocalAppData\Temp same as TEMP
                }
                let admin = if t.needs_admin { " [admin]" } else { "" };
                println!("{:<18} yes    {}{admin}", t.id, t.title);
                println!("  {}", temp_cleanup::inspect(t).summary_line());
            }
            println!();
            println!("Open:  tluw open-temp <id>");
            println!("Clear: tluw clear-temp <id> --confirm");
            println!("All:   tluw clear-temp-all --confirm");
            Ok(())
        }
        Commands::OpenTemp { id } => {
            let target = temp_cleanup::TempTarget::find(&id)
                .ok_or_else(|| format!("unknown temp target '{id}' (see temp-list)"))?;
            let st = temp_cleanup::inspect(target);
            println!("{}", st.summary_line());
            match temp_cleanup::open_location(target) {
                Ok(msg) => {
                    println!("{msg}");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::ClearTemp { id, confirm } => {
            if !confirm {
                return Err("refusing to clear-temp without --confirm".into());
            }
            let target = temp_cleanup::TempTarget::find(&id)
                .ok_or_else(|| format!("unknown temp target '{id}' (see temp-list)"))?;
            if target.needs_admin {
                require_admin(no_elevate)?;
            }
            match temp_cleanup::clear(target) {
                Ok(result) => {
                    println!("{}", result.summary_line());
                    println!("  before: {}", result.before.summary_line());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::ClearTempAll { confirm } => {
            if !confirm {
                return Err("refusing to clear-temp-all without --confirm".into());
            }
            // Elevate if any admin target is available
            if temp_cleanup::ALL
                .iter()
                .any(|t| t.is_available() && t.needs_admin)
            {
                require_admin(no_elevate)?;
            }
            let results = temp_cleanup::clear_all();
            let mut failed = false;
            let mut total_files = 0u64;
            let mut total_bytes = 0u64;
            for (id, r) in results {
                match r {
                    Ok(result) => {
                        total_files += result.removed_files;
                        total_bytes += result.freed_bytes;
                        println!("  OK  {id}: {}", result.summary_line());
                    }
                    Err(e) => {
                        eprintln!("  FAIL {id}: {e}");
                        failed = true;
                    }
                }
            }
            println!(
                "\nTotal: {} item(s), {}",
                total_files,
                log_cleanup::format_bytes(total_bytes)
            );
            if failed {
                Err("one or more temp clear operations failed".into())
            } else {
                Ok(())
            }
        }
        Commands::Disclaimer => {
            println!("{}", disclaimer::FULL);
            match disclaimer::accept("cli") {
                Ok(rec) => {
                    println!();
                    println!(
                        "Recorded local acceptance at {} (user={}, computer={}, v{}).",
                        rec.accepted_at, rec.user, rec.computer, rec.version
                    );
                    println!(
                        "Stored in HKCU\\Software\\TelemetryLoggingUtility\\Disclaimer \
                         (+ %APPDATA%\\TelemetryLoggingUtility backup). Not wiped by TEMP/log clears. Not uploaded."
                    );
                }
                Err(e) => eprintln!("Could not save acceptance record: {e}"),
            }
            Ok(())
        }
        Commands::History { days } => {
            let days = days.clamp(1, 90);
            let rows = cleanup_history::daily_totals(days);
            let (life_logs, life_temp) = cleanup_history::lifetime_totals();
            println!(
                "Lifetime freed — logs {} · temp {} · total {}",
                cleanup_history::format_size(life_logs),
                cleanup_history::format_size(life_temp),
                cleanup_history::format_size(life_logs.saturating_add(life_temp))
            );
            println!();
            println!("{:<12} {:>10} {:>10} {:>10}", "DATE", "LOGS", "TEMP", "TOTAL");
            println!("{}", "-".repeat(46));
            for d in rows {
                println!(
                    "{:<12} {:>10} {:>10} {:>10}",
                    d.date,
                    cleanup_history::format_size(d.logs_bytes),
                    cleanup_history::format_size(d.temp_bytes),
                    cleanup_history::format_size(d.total_bytes())
                );
            }
            Ok(())
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
    println!("Tip: tluw disable | enable | set <id> on|off");
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
