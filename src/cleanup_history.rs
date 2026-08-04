//! Daily cleanup history: bytes freed by log / temp clears (library API).
//!
//! Stored under `%APPDATA%\WindowsDiagnostics\cleanup_history.log` (not a TEMP path).
//! GUI dashboard + CLI `history` read the same file.

use crate::win_cmd;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Logs,
    Temp,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logs => "logs",
            Self::Temp => "temp",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "logs" | "log" => Some(Self::Logs),
            "temp" | "temps" => Some(Self::Temp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DayTotals {
    pub date: String,
    pub logs_bytes: u64,
    pub temp_bytes: u64,
}

impl DayTotals {
    pub fn total_bytes(&self) -> u64 {
        self.logs_bytes.saturating_add(self.temp_bytes)
    }
}

fn history_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("WindowsDiagnostics").join("cleanup_history.log"))
}

fn today() -> String {
    // local_stamp → "yyyy-MM-dd HH:mm:ss"
    win_cmd::local_stamp()
        .get(..10)
        .unwrap_or("0000-00-00")
        .to_string()
}

/// Append a freed-bytes event (called from log/temp clear APIs).
pub fn record(category: Category, freed_bytes: u64) {
    record_clear(category, freed_bytes, 0);
}

/// Record a clear; if byte size is unknown but items were removed, still log a
/// best-effort size so the dashboard updates.
pub fn record_clear(category: Category, freed_bytes: u64, removed_items: u64) {
    // Prefer measured bytes; fall back to a tiny per-item estimate so event-log
    // style clears (size sometimes 0) still appear on the chart.
    let bytes = if freed_bytes > 0 {
        freed_bytes
    } else if removed_items > 0 {
        removed_items.saturating_mul(512).max(1)
    } else {
        return;
    };
    let Some(path) = history_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let line = format!("{}|{}|{}\n", today(), category.as_str(), bytes);
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| {
            f.write_all(line.as_bytes())?;
            f.flush()
        });
}

fn load_raw() -> Vec<(String, Category, u64)> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let date = parts.next()?.trim().to_string();
            let cat = Category::parse(parts.next()?)?;
            let bytes: u64 = parts.next()?.trim().parse().ok()?;
            if date.len() >= 10 {
                Some((date[..10].to_string(), cat, bytes))
            } else {
                None
            }
        })
        .collect()
}

/// Aggregate by calendar day, newest last. `days` caps how many days to return
/// (looking back from today). Missing days are filled with zeros.
pub fn daily_totals(days: usize) -> Vec<DayTotals> {
    let days = days.max(1).min(90);
    let raw = load_raw();
    let mut map: BTreeMap<String, DayTotals> = BTreeMap::new();
    for (date, cat, bytes) in raw {
        let entry = map.entry(date.clone()).or_insert_with(|| DayTotals {
            date: date.clone(),
            ..Default::default()
        });
        match cat {
            Category::Logs => entry.logs_bytes = entry.logs_bytes.saturating_add(bytes),
            Category::Temp => entry.temp_bytes = entry.temp_bytes.saturating_add(bytes),
        }
    }

    // Build last N calendar days ending today (string compare works for yyyy-MM-dd).
    let today = today();
    let mut all_dates: Vec<String> = map.keys().cloned().collect();
    if !all_dates.iter().any(|d| d == &today) {
        all_dates.push(today.clone());
    }
    all_dates.sort();
    let start = all_dates.len().saturating_sub(days);
    let slice = &all_dates[start..];

    // Prefer a contiguous window: if we have sparse history, still show known days
    // plus today, padded to `days` by walking back with win32 would be heavy —
    // fill from map only for dates we have, and ensure `days` entries by
    // duplicating empty preceding labels when needed.
    let mut out: Vec<DayTotals> = slice
        .iter()
        .map(|d| {
            map.get(d).cloned().unwrap_or(DayTotals {
                date: d.clone(),
                ..Default::default()
            })
        })
        .collect();

    while out.len() < days {
        out.insert(
            0,
            DayTotals {
                date: "—".into(),
                ..Default::default()
            },
        );
    }
    if out.len() > days {
        out = out.split_off(out.len() - days);
    }
    out
}

/// Sum of all recorded frees.
pub fn lifetime_totals() -> (u64, u64) {
    let mut logs = 0u64;
    let mut temp = 0u64;
    for (_, cat, bytes) in load_raw() {
        match cat {
            Category::Logs => logs = logs.saturating_add(bytes),
            Category::Temp => temp = temp.saturating_add(bytes),
        }
    }
    (logs, temp)
}

/// Format bytes with an appropriate unit (B / KB / MB / GB).
/// Small clears show as KB/MB instead of `0.00 GB`.
pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.3} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Alias kept for older call sites / CLI wording.
pub fn format_gib(bytes: u64) -> String {
    format_size(bytes)
}

/// Today's totals (logs, temp), or zeros if nothing recorded yet.
pub fn today_totals() -> DayTotals {
    let today = today();
    daily_totals(1)
        .into_iter()
        .find(|d| d.date == today)
        .unwrap_or(DayTotals {
            date: today,
            ..Default::default()
        })
}
