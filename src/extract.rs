//! Turn arbitrary log text (serial captures, exports, e-mail, CSV) into
//! timestamped EAS events.

use crate::same::{self, Header};
use crate::time::{self, DAY};

#[derive(Debug, Clone)]
pub struct Event {
    pub header: Header,
    /// Local seconds (see `time`).
    pub local: i64,
    /// True when the time came from the log itself, false when it was
    /// derived from the header's UTC issue time.
    pub logged_time: bool,
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Station's offset from UTC in seconds, used for header-time fallback.
    pub utc_offset: i64,
    /// Year to use when the log has no dates at all.
    pub year: Option<i64>,
}

/// How far a log timestamp may be from the header's issue time before we
/// distrust it (e.g. the date of a weekly digest e-mail).
const MAX_SKEW: i64 = 36 * 3600;

fn header_local(h: &Header, year: i64, offset: i64) -> i64 {
    let days = time::days_from_civil(year, 1, 1) + i64::from(h.issue_doy) - 1;
    days * DAY + i64::from(h.issue_hour * 3600 + h.issue_min * 60) + offset
}

/// Header time placed in the year that brings it closest to `near`.
fn header_local_near(h: &Header, near: i64, offset: i64) -> i64 {
    let (y, _, _) = time::civil_from_days(near.div_euclid(DAY));
    [y - 1, y, y + 1]
        .into_iter()
        .map(|yy| header_local(h, yy, offset))
        .min_by_key(|t| (t - near).abs())
        .unwrap_or(near)
}

fn decode_quoted_printable(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    let mut bytes = Vec::with_capacity(s.len());
    while i < b.len() {
        if b[i] == b'=' {
            if b.get(i + 1) == Some(&b'\n') {
                i += 2;
                continue;
            }
            if b.get(i + 1) == Some(&b'\r') && b.get(i + 2) == Some(&b'\n') {
                i += 3;
                continue;
            }
            let hex = b
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hex {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    out.push_str(&String::from_utf8_lossy(&bytes));
    out
}

/// Split a file into records: mbox messages, or the whole file.
fn records(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    if text.starts_with("From ") {
        let mut start = 0;
        let mut line_no = 0;
        let mut start_line = 1;
        let mut pos = 0;
        for line in text.split_inclusive('\n') {
            line_no += 1;
            if line.starts_with("From ") && pos > 0 {
                out.push((start_line, text[start..pos].to_string()));
                start = pos;
                start_line = line_no;
            }
            pos += line.len();
        }
        out.push((start_line, text[start..].to_string()));
    } else {
        out.push((1, text.to_string()));
    }
    out
}

/// The e-mail "Date:" header of a record, if it is an e-mail.
fn email_date(rec: &str) -> Option<i64> {
    for line in rec.lines() {
        if line.trim().is_empty() {
            break;
        }
        if line.len() > 5 && line[..5].eq_ignore_ascii_case("date:") {
            return time::find_timestamp(&line[5..]);
        }
    }
    None
}

fn is_quoted_printable(rec: &str) -> bool {
    // Look at every MIME header, not just the top-level one: in multipart
    // messages the encoding is declared on the part.
    rec.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("content-transfer-encoding:") && l.contains("quoted-printable")
    })
}

/// Extract events from one file's text. Returns events and warnings.
pub fn extract(text: &str, file: &str, opts: Options) -> (Vec<Event>, Vec<String>) {
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut last_year: Option<i64> = None;
    for (rec_line, raw) in records(text) {
        let rec = if is_quoted_printable(&raw) {
            decode_quoted_printable(&raw)
        } else {
            raw
        };
        let mail_date = email_date(&rec);
        let mut prev_end = 0;
        for (start, end, header) in same::find_all(&rec) {
            let line_end = rec[end..].find('\n').map_or(rec.len(), |p| end + p);
            let window = &rec[prev_end..line_end];
            let line = rec_line + rec[..start].matches('\n').count();
            prev_end = end;

            let found = window
                .lines()
                .rev()
                .find_map(time::find_timestamp)
                .or(mail_date);
            if let Some(t) = found {
                last_year = Some(time::civil_from_days(t.div_euclid(DAY)).0);
            }
            let (local, logged) = match found {
                Some(t) => {
                    let h = header_local_near(&header, t, opts.utc_offset);
                    if (t - h).abs() <= MAX_SKEW {
                        (t, true)
                    } else {
                        (h, false)
                    }
                }
                None => match opts.year.or(last_year) {
                    Some(y) => (header_local(&header, y, opts.utc_offset), false),
                    None => {
                        warnings.push(format!(
                            "{file}:{line}: no date found for {}; pass --year",
                            header.canonical()
                        ));
                        continue;
                    }
                },
            };
            events.push(Event {
                header,
                local,
                logged_time: logged,
                file: file.to_string(),
                line,
            });
        }
    }
    (events, warnings)
}

/// Sort and drop duplicates: the same header logged twice (e.g. once in an
/// e-mail and once in an export) within 15 minutes counts once.
pub fn dedup(mut events: Vec<Event>) -> Vec<Event> {
    events.sort_by_key(|e| (e.local, !e.logged_time));
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    for e in events {
        let canon = e.header.canonical();
        let dup = out
            .iter()
            .rev()
            .take_while(|o| e.local - o.local <= 15 * 60)
            .any(|o| o.header.canonical() == canon);
        if !dup {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTS: Options = Options {
        utc_offset: -6 * 3600,
        year: None,
    };

    #[test]
    fn timestamp_before_header_is_used() {
        let t = "2026-03-02 09:05:10 Alert Received on monitor 1\n\
                 ZCZC-EAS-RWT-017031+0015-0611500-WAAA/FM -\n\
                 2026-03-03 10:00:00 Alert Received\n\
                 ZCZC-EAS-RWT-017031+0015-0621600-WBBB/AM -\n";
        let (ev, warn) = extract(t, "f", OPTS);
        assert!(warn.is_empty());
        assert_eq!(ev.len(), 2);
        assert_eq!(time::fmt_datetime(ev[0].local), "2026-03-02 09:05");
        assert!(ev[0].logged_time);
        assert_eq!(ev[1].header.sender, "WBBB/AM");
        assert_eq!(ev[1].line, 4);
    }

    #[test]
    fn falls_back_to_header_time_with_year() {
        let t = "Alert Received 09:05\nZCZC-EAS-RWT-017031+0015-0611500-WAAA/FM -\n";
        let (ev, warn) = extract(t, "f", OPTS);
        assert!(ev.is_empty());
        assert_eq!(warn.len(), 1);
        let (ev, _) = extract(
            t,
            "f",
            Options {
                year: Some(2026),
                ..OPTS
            },
        );
        // Day 061 of 2026 is March 2; 15:00 UTC is 09:00 at UTC-6.
        assert_eq!(time::fmt_datetime(ev[0].local), "2026-03-02 09:00");
        assert!(!ev[0].logged_time);
    }

    #[test]
    fn distrusts_far_away_timestamp() {
        let t = "Weekly report generated 2026-03-09 08:00\n\
                 ZCZC-EAS-RWT-017031+0015-0611500-WAAA/FM -\n";
        let (ev, _) = extract(t, "f", OPTS);
        assert_eq!(time::fmt_datetime(ev[0].local), "2026-03-02 09:00");
        assert!(!ev[0].logged_time);
    }

    #[test]
    fn mbox_with_quoted_printable() {
        let t = "From eas@example Mon Mar  2 09:06:00 2026\n\
                 Date: Mon, 2 Mar 2026 09:06:00 -0600\n\
                 Content-Transfer-Encoding: quoted-printable\n\
                 \n\
                 EAS alert received: ZCZC-EAS-RWT-017031+0015-06115=\n00-WAAA/FM -\n\
                 \n\
                 From eas@example Tue Mar  3 10:01:00 2026\n\
                 Date: Tue, 3 Mar 2026 10:01:00 -0600\n\
                 \n\
                 ZCZC-EAS-RWT-017031+0015-0621600-WBBB/AM -\n";
        let (ev, warn) = extract(t, "box", OPTS);
        assert!(warn.is_empty(), "{warn:?}");
        assert_eq!(ev.len(), 2);
        assert_eq!(time::fmt_datetime(ev[0].local), "2026-03-02 09:06");
        assert_eq!(time::fmt_datetime(ev[1].local), "2026-03-03 10:01");
        assert_eq!(ev[1].line, 11);
    }

    #[test]
    fn multipart_quoted_printable_part_is_decoded() {
        let t = "Date: Mon, 02 Mar 2026 10:00:05 -0600\n\
                 MIME-Version: 1.0\n\
                 Content-Type: multipart/alternative; boundary=\"XX\"\n\
                 \n\
                 --XX\n\
                 Content-Type: text/plain; charset=utf-8\n\
                 Content-Transfer-Encoding: quoted-printable\n\
                 \n\
                 EAS Header: ZCZC-EAS-RWT-019153+0015-0611600-WAAA/F=\nM -\n\
                 --XX--\n";
        let (ev, _) = extract(t, "b.eml", OPTS);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].header.sender, "WAAA/FM");
    }

    #[test]
    fn year_wrap_uses_nearest_year() {
        // Logged 2026-01-01 00:10 local, header issued day 365 of 2025 at 23:59 local.
        let t = "2026-01-01 00:10 ZCZC-EAS-RWT-017031+0015-3650559-WAAA/FM -";
        let (ev, _) = extract(t, "f", OPTS);
        assert!(ev[0].logged_time);
    }

    #[test]
    fn dedups_repeats() {
        let t = "2026-03-02 09:05 ZCZC-EAS-RWT-017031+0015-0611500-WAAA/FM -\n\
                 2026-03-02 09:06 ZCZC-EAS-RWT-017031+0015-0611500-WAAA/FM -\n\
                 2026-03-02 09:06 ZCZC-EAS-RWT-017031+0015-0611500-WBBB/AM -\n";
        let (ev, _) = extract(t, "f", OPTS);
        assert_eq!(dedup(ev).len(), 2);
    }
}
