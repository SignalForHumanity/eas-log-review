//! eas-log-review: weekly and monthly FCC EAS log review from decoder logs.

mod extract;
mod review;
mod same;
mod time;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
eas-log-review: check EAS decoder logs for missed tests, per source and week

USAGE:
  eas-log-review report  [OPTIONS] <FILE|DIR>...
  eas-log-review senders [OPTIONS] <FILE|DIR>...
  eas-log-review decode  <SAME HEADER>

Files may be log exports, serial captures, saved e-mails (.eml, mbox) or CSV.
Any text that contains SAME headers (ZCZC-...) works.

OPTIONS:
  --station ID         Your own sender ID as it appears in headers you send
                       (e.g. WXYZ/LP). Repeatable.
  --source NAME=ID     A monitoring assignment, e.g. LP-1=WAAA/FM. ID may end
                       in * to match by prefix. Repeatable.
  --decoder-only       Station only decodes (typical LPFM); do not expect
                       transmitted headers.
  --month YYYY-MM      Review only this month (default: every month found).
  --year YYYY          Year for logs that have no dates at all.
  --utc-offset H[:MM]  Station's offset from UTC, used when only the header's
                       issue time is known (default 0), e.g. -5.
  --from YYYY-MM-DD    First day the logs cover (default: first entry).
  --to YYYY-MM-DD      Last day the logs cover (default: last entry).
  --week-start DAY     sunday (default) or monday.
  --csv FILE           Also write every event to a CSV file.
  -h, --help           Show this help.

EXIT STATUS: 0 no problems, 2 problems found, 1 error.";

struct Args {
    cmd: String,
    paths: Vec<PathBuf>,
    cfg: review::Config,
    month: Option<(i64, u32)>,
    opts: extract::Options,
    csv: Option<PathBuf>,
    offset_given: bool,
}

fn parse_month(s: &str) -> Option<(i64, u32)> {
    let (y, m) = s.split_once('-')?;
    let y: i64 = y.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    if (1..=12).contains(&m) && (1990..=2200).contains(&y) {
        Some((y, m))
    } else {
        None
    }
}

fn parse_day(s: &str) -> Option<i64> {
    let mut it = s.splitn(3, '-');
    let (y, m, d) = (it.next()?, it.next()?, it.next()?);
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if (1990..=2200).contains(&y)
        && (1..=12).contains(&m)
        && d >= 1
        && d <= time::days_in_month(y, m)
    {
        Some(time::days_from_civil(y, m, d))
    } else {
        None
    }
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut it = argv.iter();
    let cmd = it.next().ok_or("missing command")?.clone();
    let mut a = Args {
        cmd,
        paths: Vec::new(),
        cfg: review::Config::default(),
        month: None,
        opts: extract::Options {
            utc_offset: 0,
            year: None,
        },
        csv: None,
        offset_given: false,
    };
    while let Some(arg) = it.next() {
        let mut val = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--station" => a.cfg.stations.push(val("--station")?.trim().to_string()),
            "--source" => {
                let v = val("--source")?;
                let (n, id) = v
                    .split_once('=')
                    .ok_or("--source must look like NAME=SENDERID")?;
                if n.trim().is_empty() || id.trim().is_empty() {
                    return Err("--source must look like NAME=SENDERID".into());
                }
                a.cfg
                    .sources
                    .push((n.trim().to_string(), id.trim().to_string()));
            }
            "--decoder-only" => a.cfg.decoder_only = true,
            "--month" => {
                let v = val("--month")?;
                a.month = Some(parse_month(&v).ok_or("--month must be YYYY-MM")?);
            }
            "--year" => {
                let v = val("--year")?;
                a.opts.year = Some(
                    v.parse::<i64>()
                        .ok()
                        .filter(|y| (1990..=2200).contains(y))
                        .ok_or("--year must be a 4-digit year")?,
                );
            }
            "--utc-offset" => {
                let v = val("--utc-offset")?;
                a.opts.utc_offset =
                    time::parse_offset(&v).ok_or("--utc-offset must look like -5 or -05:00")?;
                a.offset_given = true;
            }
            "--week-start" => {
                a.cfg.week_start = match val("--week-start")?.to_ascii_lowercase().as_str() {
                    "sunday" | "sun" => 0,
                    "monday" | "mon" => 1,
                    _ => return Err("--week-start must be sunday or monday".into()),
                }
            }
            "--from" | "--to" => {
                let name = arg.clone();
                let v = val(&name)?;
                let d = parse_day(&v).ok_or_else(|| format!("{name} must be YYYY-MM-DD"))?;
                if name == "--from" {
                    a.cfg.from = Some(d);
                } else {
                    a.cfg.to = Some(d);
                }
            }
            "--csv" => a.csv = Some(PathBuf::from(val("--csv")?)),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => a.paths.push(PathBuf::from(s)),
        }
    }
    Ok(a)
}

fn collect_files(p: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if p.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(p)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                !p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
            })
            .collect();
        entries.sort();
        for e in entries {
            collect_files(&e, out)?;
        }
    } else {
        out.push(p.to_path_buf());
    }
    Ok(())
}

fn load(a: &Args) -> Result<Vec<extract::Event>, String> {
    let mut files = Vec::new();
    for p in &a.paths {
        collect_files(p, &mut files).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let mut all = Vec::new();
    for f in &files {
        let bytes = fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let (ev, warn) = extract::extract(&text, &f.display().to_string(), a.opts);
        for w in warn {
            eprintln!("warning: {w}");
        }
        all.extend(ev);
    }
    let from_header = all.iter().filter(|e| !e.logged_time).count();
    if from_header > 0 && !a.offset_given {
        eprintln!(
            "warning: {from_header} entries have no usable log time and were placed by the \
             header's UTC issue time; pass --utc-offset (e.g. -6) or they may land on the \
             wrong day or week"
        );
    }
    Ok(extract::dedup(all))
}

fn run(argv: &[String]) -> Result<bool, String> {
    let a = parse_args(argv)?;
    match a.cmd.as_str() {
        "decode" => {
            let text = a
                .paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let found = same::find_all(&text);
            if found.is_empty() {
                return Err("no valid SAME header found".into());
            }
            for (_, _, h) in found {
                println!("Originator: {} ({})", h.org, same::org_name(&h.org));
                println!("Event:      {} ({})", h.event, same::event_name(&h.event));
                println!("Areas:      {}", h.locations.join(" "));
                println!("Valid for:  {}h{}m", &h.purge[..2], &h.purge[2..]);
                println!("Issued:     {}", h.issue_text());
                println!("Sender:     {}", h.sender);
            }
            Ok(true)
        }
        "senders" | "report" => {
            if a.paths.is_empty() {
                return Err("no input files given".into());
            }
            let events = load(&a)?;
            if events.is_empty() {
                return Err("no SAME headers found in the input".into());
            }
            if a.cmd == "senders" {
                let mut counts: Vec<(String, usize, Vec<String>, i64, i64)> = Vec::new();
                for e in &events {
                    let s = &e.header.sender;
                    match counts.iter_mut().find(|c| &c.0 == s) {
                        Some(c) => {
                            c.1 += 1;
                            if !c.2.contains(&e.header.event) {
                                c.2.push(e.header.event.clone());
                            }
                            c.4 = e.local;
                        }
                        None => counts.push((
                            s.clone(),
                            1,
                            vec![e.header.event.clone()],
                            e.local,
                            e.local,
                        )),
                    }
                }
                counts.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
                println!(
                    "{:<10} {:>6}  {:<10}  {:<10}  events",
                    "sender", "count", "first", "last"
                );
                for (s, n, ev, f, l) in counts {
                    println!(
                        "{:<10} {:>6}  {}  {}  {}",
                        s,
                        n,
                        time::fmt_date(f.div_euclid(time::DAY)),
                        time::fmt_date(l.div_euclid(time::DAY)),
                        ev.join(" ")
                    );
                }
                return Ok(true);
            }
            if let Some(path) = &a.csv {
                fs::write(path, review::csv(&a.cfg, &events))
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            }
            let months = match a.month {
                Some(m) => vec![m],
                None => review::months_in(&events),
            };
            let mut clean = true;
            for (i, (y, m)) in months.into_iter().enumerate() {
                let r = review::review_month(&a.cfg, &events, y, m);
                if i > 0 {
                    println!("\n");
                }
                print!("{}", review::render(&a.cfg, &r));
                clean &= r.problems.is_empty();
            }
            Ok(clean)
        }
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(true)
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match run(&argv) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(e) => {
            eprintln!("error: {e}\n\nRun `eas-log-review --help` for usage.");
            ExitCode::from(1)
        }
    }
}
