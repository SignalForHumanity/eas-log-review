//! FCC Part 11 log checks and the printable monthly review.

use crate::extract::Event;
use crate::same;
use crate::time::{self, DAY};
use std::fmt::Write as _;

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The station's own sender IDs (headers it transmits).
    pub stations: Vec<String>,
    /// Monitoring sources: (label, sender pattern). Pattern ending in `*`
    /// matches by prefix.
    pub sources: Vec<(String, String)>,
    /// LPFM-style decoder only: no headers are transmitted.
    pub decoder_only: bool,
    /// 0 = weeks start Sunday, 1 = Monday.
    pub week_start: u32,
    /// First and last day (days since epoch) the logs cover, if known.
    /// Defaults to the first and last logged event.
    pub from: Option<i64>,
    pub to: Option<i64>,
}

fn matches(pattern: &str, sender: &str) -> bool {
    let p = pattern.trim().to_ascii_uppercase();
    match p.strip_suffix('*') {
        Some(prefix) => sender.starts_with(prefix),
        None => sender == p,
    }
}

impl Config {
    pub fn is_sent(&self, e: &Event) -> bool {
        self.stations.iter().any(|s| matches(s, &e.header.sender))
    }

    pub fn source_of(&self, e: &Event) -> Option<&str> {
        self.sources
            .iter()
            .find(|(_, p)| matches(p, &e.header.sender))
            .map(|(n, _)| n.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeekStatus {
    Ok,
    /// Not judged: the log does not cover the whole week.
    Partial(String),
    Problem,
}

#[derive(Debug, Clone)]
pub struct WeekRow {
    pub start: i64,
    /// One cell per column: event codes seen.
    pub cells: Vec<Vec<String>>,
    pub rmt_week: bool,
    pub status: WeekStatus,
}

#[derive(Debug, Clone)]
pub struct RelayCheck {
    pub received: Event,
    pub source: String,
    pub sent: Option<Event>,
    /// Minutes between reception and retransmission, when both are logged.
    pub delay_min: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MonthReview {
    pub year: i64,
    pub month: u32,
    pub columns: Vec<String>,
    pub weeks: Vec<WeekRow>,
    pub monthly: Vec<RelayCheck>,
    pub activations: Vec<(Event, String)>,
    pub problems: Vec<String>,
    pub notes: Vec<String>,
}

fn day_of(e: &Event) -> i64 {
    e.local.div_euclid(DAY)
}

fn label(cfg: &Config, e: &Event) -> String {
    match cfg.source_of(e) {
        Some(n) => format!("{n} ({})", e.header.sender),
        None => e.header.sender.clone(),
    }
}

pub fn months_in(events: &[Event]) -> Vec<(i64, u32)> {
    let mut out: Vec<(i64, u32)> = Vec::new();
    for e in events {
        let (y, m, _) = time::civil_from_days(day_of(e));
        if !out.contains(&(y, m)) {
            out.push((y, m));
        }
    }
    out.sort();
    out
}

/// Review one calendar month. `events` must be all loaded events (sorted),
/// so that weeks crossing month boundaries are judged on full data.
pub fn review_month(cfg: &Config, events: &[Event], year: i64, month: u32) -> MonthReview {
    let first = time::days_from_civil(year, month, 1);
    let last = first + i64::from(time::days_in_month(year, month)) - 1;
    let data_min = cfg
        .from
        .unwrap_or_else(|| events.iter().map(day_of).min().unwrap_or(first));
    let data_max = cfg
        .to
        .unwrap_or_else(|| events.iter().map(day_of).max().unwrap_or(first));
    let mut problems = Vec::new();
    let mut notes = Vec::new();

    let per_source = !cfg.sources.is_empty();
    let mut columns: Vec<String> = if per_source {
        cfg.sources.iter().map(|(n, _)| n.clone()).collect()
    } else {
        vec!["Received".to_string()]
    };
    let show_sent = !cfg.stations.is_empty();
    if show_sent {
        columns.push("Sent".to_string());
    }
    if !per_source {
        notes.push(
            "No --source given, so each monitoring assignment is not checked separately. \
             Run `eas-log-review senders` to see the sender IDs in your logs."
                .to_string(),
        );
    }
    if data_min > first {
        notes.push(format!(
            "The log starts {}; earlier days of the month are not covered. \
             If the log does cover them, pass --from {}.",
            time::fmt_date(data_min),
            time::fmt_date(first)
        ));
    }
    if data_max < last {
        notes.push(format!(
            "The log ends {}; later days of the month are not covered. \
             If the log does cover them, pass --to.",
            time::fmt_date(data_max)
        ));
    }

    // Weeks that intersect the month.
    let offset = (i64::from(time::weekday(first)) - i64::from(cfg.week_start)).rem_euclid(7);
    let mut start = first - offset;
    let mut weeks = Vec::new();
    while start <= last {
        let end = start + 6;
        let in_week: Vec<&Event> = events
            .iter()
            .filter(|e| (start..=end).contains(&day_of(e)))
            .collect();
        let received: Vec<&&Event> = in_week.iter().filter(|e| !cfg.is_sent(e)).collect();
        let sent: Vec<&&Event> = in_week.iter().filter(|e| cfg.is_sent(e)).collect();
        // An RMT or a national test (NPT) received this week may be the only test
        // (47 CFR 11.61(a)(3)(ii): a national test replaces the weekly and monthly tests).
        let rmt_week = received
            .iter()
            .any(|e| matches!(e.header.event.as_str(), "RMT" | "NPT"));

        let codes = |list: &[&&Event]| -> Vec<String> {
            let mut v: Vec<String> = Vec::new();
            for e in list {
                if !v.contains(&e.header.event) {
                    v.push(e.header.event.clone());
                }
            }
            v
        };
        let mut cells = Vec::new();
        if per_source {
            for (name, _) in &cfg.sources {
                let from: Vec<&&Event> = received
                    .iter()
                    .copied()
                    .filter(|e| cfg.source_of(e) == Some(name.as_str()))
                    .collect();
                cells.push(codes(&from));
            }
        } else {
            cells.push(codes(&received));
        }
        if show_sent {
            cells.push(codes(&sent));
        }

        let wk = format!("Week of {}", time::fmt_date(start));
        let status = if start < data_min {
            WeekStatus::Partial(format!("log starts {}", time::fmt_date(data_min)))
        } else if end > data_max {
            WeekStatus::Partial(format!("log ends {}", time::fmt_date(data_max)))
        } else {
            let mut bad = false;
            if per_source {
                for (i, (name, pat)) in cfg.sources.iter().enumerate() {
                    if cells[i].is_empty() {
                        if rmt_week {
                            notes.push(format!(
                                "{wk}: nothing from {name} ({pat}), but an RMT or NPT was \
                                 received that week, which may be the only test."
                            ));
                        } else {
                            bad = true;
                            problems.push(format!(
                                "{wk}: no test or alert received from {name} ({pat}). \
                                 Enter the reason in the station log (47 CFR 11.35(a))."
                            ));
                        }
                    }
                }
            } else if cells[0].is_empty() {
                bad = true;
                problems.push(format!(
                    "{wk}: no test or alert received from any source. \
                     Enter the reason in the station log (47 CFR 11.35(a))."
                ));
            }
            if show_sent && !cfg.decoder_only {
                let is_test =
                    |e: &&&Event| matches!(e.header.event.as_str(), "RWT" | "RMT" | "NPT");
                let sent_test = sent.iter().any(is_test);
                let sent_activation = sent.iter().find(|e| !is_test(e));
                match (sent_test, sent_activation) {
                    (true, _) => {}
                    (false, Some(a)) => notes.push(format!(
                        "{wk}: no RWT sent, but the station relayed {}. It counts as the weekly \
                         test if it carried the EAS header and EOM codes (47 CFR 11.61(a)(4)).",
                        a.header.event
                    )),
                    (false, None) => {
                        bad = true;
                        problems.push(format!(
                            "{wk}: no RWT, RMT or NPT transmitted by the station (47 CFR 11.61(a)(2))."
                        ));
                    }
                }
            }
            if bad {
                WeekStatus::Problem
            } else {
                WeekStatus::Ok
            }
        };
        weeks.push(WeekRow {
            start,
            cells,
            rmt_week,
            status,
        });
        start += 7;
    }

    // Monthly and national tests, and other activations, in this month.
    let in_month: Vec<&Event> = events
        .iter()
        .filter(|e| (first..=last).contains(&day_of(e)))
        .collect();
    let mut monthly: Vec<RelayCheck> = Vec::new();
    let mut activations: Vec<(Event, String)> = Vec::new();
    let mut seen_keys = Vec::new();
    for e in in_month.iter().filter(|e| !cfg.is_sent(e)) {
        let key = e.header.alert_key();
        if seen_keys.contains(&key) {
            continue;
        }
        seen_keys.push(key.clone());
        if same::must_relay(&e.header.event) {
            let sent = events
                .iter()
                .find(|s| cfg.is_sent(s) && s.header.alert_key() == key && s.local >= e.local - 300)
                .cloned();
            // Round up so that 60 min 40 s reads as 61 and fails the limit.
            let delay_min = sent
                .as_ref()
                .filter(|s| s.logged_time && e.logged_time)
                .map(|s| ((s.local - e.local).max(0) + 59) / 60);
            monthly.push(RelayCheck {
                received: (*e).clone(),
                source: label(cfg, e),
                sent,
                delay_min,
            });
        } else if e.header.event != "RWT" {
            activations.push(((*e).clone(), label(cfg, e)));
        }
    }

    let rmts = monthly
        .iter()
        .filter(|m| matches!(m.received.header.event.as_str(), "RMT" | "NPT"))
        .count();
    if rmts == 0 {
        if data_min <= first && data_max >= last {
            problems.push(format!(
                "No Required Monthly Test (or national test) received in {year:04}-{month:02}. \
                 Enter the reason in the station log (47 CFR 11.35(a), 11.61(a)(1))."
            ));
        } else {
            notes.push("No RMT found in the part of the month the log covers.".to_string());
        }
    }
    for m in &monthly {
        let what = format!(
            "{} received {} from {}",
            m.received.header.event,
            time::fmt_datetime(m.received.local),
            m.source
        );
        if m.received.header.event == "EAN" {
            // A national activation, not a test: it must be relayed at once, so the
            // 60-minute test rule does not apply. Report what the log shows.
            notes.push(match (&m.sent, m.delay_min) {
                (None, _) if !cfg.decoder_only && show_sent => {
                    format!("{what}: no retransmission found. An EAN must be relayed immediately.")
                }
                (Some(_), Some(d)) => format!("{what}: relayed after {d} minute(s)."),
                _ => format!("{what}: confirm it was relayed immediately and that this is logged."),
            });
        } else if cfg.decoder_only || !show_sent {
            notes.push(format!(
                "{what}: confirm the test was aired within 60 minutes and that this is logged."
            ));
        } else {
            match (&m.sent, m.delay_min) {
                (None, _) => problems.push(format!(
                    "{what} was not retransmitted (no sent header with the same issue time). \
                     It must be relayed within 60 minutes (47 CFR 11.61(a)(1))."
                )),
                (Some(_), Some(d)) if d > 60 => problems.push(format!(
                    "{what} was retransmitted after {d} minutes; the limit is 60 (47 CFR 11.61(a)(1))."
                )),
                (Some(_), None) => notes.push(format!(
                    "{what} was retransmitted, but the log has no reception or send time to check the 60-minute limit."
                )),
                _ => {}
            }
        }
    }

    MonthReview {
        year,
        month,
        columns,
        weeks,
        monthly,
        activations,
        problems,
        notes,
    }
}

fn cell_text(codes: &[String]) -> String {
    if codes.is_empty() {
        "--".to_string()
    } else if codes.len() <= 2 {
        codes.join("+")
    } else {
        format!("{}+{}", codes[0], codes.len() - 1)
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub fn render(cfg: &Config, r: &MonthReview) -> String {
    let mut o = String::new();
    let _ = writeln!(
        o,
        "EAS LOG REVIEW: {} {}",
        MONTHS[(r.month - 1) as usize],
        r.year
    );
    let _ = writeln!(o, "{}", "=".repeat(60));
    if !cfg.stations.is_empty() {
        let _ = writeln!(o, "Station sender ID: {}", cfg.stations.join(", "));
    }
    if cfg.decoder_only {
        let _ = writeln!(o, "Mode: decoder only (no headers transmitted)");
    }
    for (n, p) in &cfg.sources {
        let _ = writeln!(o, "Source {n}: {p}");
    }
    let _ = writeln!(o);

    let wk_label = if cfg.week_start == 1 {
        "Week (Mon-Sun)"
    } else {
        "Week (Sun-Sat)"
    };
    let width = r.columns.iter().map(|c| c.len()).max().unwrap_or(4).max(8) + 2;
    let _ = write!(o, "{wk_label:<26}");
    for c in &r.columns {
        let _ = write!(o, "{c:<width$}");
    }
    let _ = writeln!(o, "Result");
    for w in &r.weeks {
        let range = format!(
            "{} .. {}",
            time::fmt_date(w.start),
            &time::fmt_date(w.start + 6)[5..]
        );
        let _ = write!(o, "{range:<26}");
        for c in &w.cells {
            let _ = write!(o, "{:<width$}", cell_text(c));
        }
        let result = match &w.status {
            WeekStatus::Ok if w.rmt_week => "ok (RMT week)".to_string(),
            WeekStatus::Ok => "ok".to_string(),
            WeekStatus::Problem => "PROBLEM".to_string(),
            WeekStatus::Partial(why) => format!("not judged: {why}"),
        };
        let _ = writeln!(o, "{result}");
    }
    let _ = writeln!(o);

    let _ = writeln!(o, "Monthly and national tests");
    if r.monthly.is_empty() {
        let _ = writeln!(o, "  none");
    }
    for m in &r.monthly {
        let h = &m.received.header;
        let _ = write!(
            o,
            "  {}  {} {} from {}",
            time::fmt_datetime(m.received.local),
            h.event,
            same::event_name(&h.event),
            m.source
        );
        match (&m.sent, m.delay_min) {
            (Some(s), Some(d)) => {
                let _ = write!(o, "; relayed {} ({d} min)", time::fmt_datetime(s.local));
            }
            (Some(_), None) => {
                let _ = write!(o, "; relayed (time not logged)");
            }
            (None, _) if !cfg.decoder_only && !cfg.stations.is_empty() => {
                let _ = write!(o, "; NOT RELAYED");
            }
            _ => {}
        }
        if !m.received.logged_time {
            let _ = write!(o, " [time from header]");
        }
        let _ = writeln!(o);
    }
    let _ = writeln!(o);

    let _ = writeln!(o, "Other alerts received");
    if r.activations.is_empty() {
        let _ = writeln!(o, "  none");
    }
    for (e, src) in &r.activations {
        let h = &e.header;
        let _ = writeln!(
            o,
            "  {}  {} {} from {}; {}; areas {}",
            time::fmt_datetime(e.local),
            h.event,
            same::event_name(&h.event),
            src,
            same::org_name(&h.org),
            h.locations.join(" ")
        );
    }
    let _ = writeln!(o);

    if r.problems.is_empty() {
        let _ = writeln!(o, "Problems: none found");
    } else {
        let _ = writeln!(o, "Problems ({}):", r.problems.len());
        for p in &r.problems {
            let _ = writeln!(o, "  - {p}");
        }
    }
    if !r.notes.is_empty() {
        let _ = writeln!(o, "Notes:");
        for n in &r.notes {
            let _ = writeln!(o, "  - {n}");
        }
    }
    let _ = writeln!(o);
    let _ = writeln!(o, "Weekly review");
    for w in &r.weeks {
        let _ = writeln!(
            o,
            "  Week of {}  Reviewed by: ______________________  Date: __________",
            time::fmt_date(w.start)
        );
    }
    o
}

/// One CSV row per event.
pub fn csv(cfg: &Config, events: &[Event]) -> String {
    let mut o = String::from(
        "local_time,time_source,direction,source,sender,originator,event,event_name,locations,issued,purge,header,file,line\n",
    );
    for e in events {
        let h = &e.header;
        let dir = if cfg.is_sent(e) { "sent" } else { "received" };
        let src = cfg.source_of(e).unwrap_or("");
        let _ = writeln!(
            o,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            time::fmt_datetime(e.local),
            if e.logged_time { "log" } else { "header" },
            dir,
            csv_field(src),
            csv_field(&h.sender),
            h.org,
            h.event,
            csv_field(same::event_name(&h.event)),
            h.locations.join(" "),
            csv_field(&h.issue_text()),
            h.purge,
            csv_field(&h.canonical()),
            csv_field(&e.file),
            e.line
        );
    }
    o
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
