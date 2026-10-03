//! Calendar arithmetic and timestamp recognition, with no dependencies.
//!
//! All times inside the tool are "local seconds": seconds since
//! 1970-01-01 00:00 in the station's local wall-clock time, with no zone.

pub const DAY: i64 = 86_400;

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// 0 = Sunday ... 6 = Saturday.
pub fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

pub fn fmt_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn fmt_datetime(secs: i64) -> String {
    let days = secs.div_euclid(DAY);
    let rem = secs.rem_euclid(DAY);
    format!(
        "{} {:02}:{:02}",
        fmt_date(days),
        rem / 3600,
        (rem % 3600) / 60
    )
}

pub fn local_secs(y: i64, m: u32, d: u32, h: u32, min: u32, s: u32) -> i64 {
    days_from_civil(y, m, d) * DAY + i64::from(h * 3600 + min * 60 + s)
}

fn month_from_name(tok: &str) -> Option<u32> {
    let t = tok.trim_end_matches('.').to_ascii_lowercase();
    let names = [
        ("jan", "january"),
        ("feb", "february"),
        ("mar", "march"),
        ("apr", "april"),
        ("may", "may"),
        ("jun", "june"),
        ("jul", "july"),
        ("aug", "august"),
        ("sep", "september"),
        ("oct", "october"),
        ("nov", "november"),
        ("dec", "december"),
    ];
    for (i, (short, long)) in names.iter().enumerate() {
        if t == *short || t == *long || (i == 8 && t == "sept") {
            return Some(i as u32 + 1);
        }
    }
    None
}

fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn num(s: &str, min_len: usize, max_len: usize) -> Option<u32> {
    if all_digits(s) && s.len() >= min_len && s.len() <= max_len {
        s.parse().ok()
    } else {
        None
    }
}

fn valid_date(y: i64, m: u32, d: u32) -> Option<(i64, u32, u32)> {
    if (1990..=2200).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) {
        Some((y, m, d))
    } else {
        None
    }
}

fn year_from(s: &str) -> Option<i64> {
    match s.len() {
        4 => num(s, 4, 4).map(i64::from),
        2 => num(s, 2, 2).map(|v| 2000 + i64::from(v)),
        _ => None,
    }
}

/// A date written as a single token: 2026-03-02, 3/2/2026, 03/02/26, 03-02-2026.
fn date_token(tok: &str) -> Option<(i64, u32, u32)> {
    if let Some(sep) = b"-/.".iter().copied().find(|c| tok.as_bytes().contains(c)) {
        let parts: Vec<&str> = tok.split(sep as char).collect();
        if parts.len() != 3 {
            return None;
        }
        if parts[0].len() == 4 && sep != b'.' {
            let y = year_from(parts[0])?;
            return valid_date(y, num(parts[1], 1, 2)?, num(parts[2], 1, 2)?);
        }
        if sep != b'.' && parts[2].len() >= 2 {
            // US order: month/day/year
            let y = year_from(parts[2])?;
            return valid_date(y, num(parts[0], 1, 2)?, num(parts[1], 1, 2)?);
        }
    }
    None
}

/// A time token: 11:00, 11:00:05, 11:00:05.123, 11:00AM, 11:00:05Z.
/// Returns seconds into the day and whether an AM/PM marker is still expected.
fn time_token(tok: &str) -> Option<(u32, u32, u32, Option<bool>)> {
    let lower = tok.to_ascii_lowercase();
    let mut t = lower.trim_end_matches('z');
    // Drop an ISO 8601 zone offset such as "-06:00" or "+0530" (times are
    // taken as local); a time itself never contains '+' or '-'.
    if let Some(p) = t.find(['+', '-']).filter(|&p| p >= 4) {
        t = &t[..p];
    }
    let mut pm = None;
    if let Some(stripped) = t.strip_suffix("pm") {
        pm = Some(true);
        t = stripped;
    } else if let Some(stripped) = t.strip_suffix("am") {
        pm = Some(false);
        t = stripped;
    }
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let h = num(parts[0], 1, 2)?;
    let m = num(parts[1], 2, 2)?;
    let s = if parts.len() == 3 {
        let sec = parts[2].split('.').next()?;
        num(sec, 2, 2)?
    } else {
        0
    };
    if h > 23 || m > 59 || s > 60 {
        return None;
    }
    Some((h, m, s.min(59), pm))
}

fn apply_ampm(h: u32, pm: bool) -> Option<u32> {
    if !(1..=12).contains(&h) {
        return None;
    }
    Some(match (h, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    })
}

/// A date written with a month name over three tokens starting at `i`:
/// "Mar 2 2026", "March 2nd, 2026" or "2 Mar 2026" (RFC 2822).
fn name_date(toks: &[&str], i: usize) -> Option<(i64, u32, u32)> {
    let (a, b, c) = (toks[i], *toks.get(i + 1)?, *toks.get(i + 2)?);
    let y = i64::from(num(c, 4, 4)?);
    if let Some(m) = month_from_name(a) {
        let d = num(b.trim_end_matches(['s', 't', 'n', 'd', 'r', 'h']), 1, 2)?;
        return valid_date(y, m, d);
    }
    let d = num(a, 1, 2)?;
    valid_date(y, month_from_name(b)?, d)
}

/// Find a date and a time on one line of text, in either order, in the
/// common styles of EAS boxes, e-mail and spreadsheets. Returns local
/// seconds. Zone suffixes are ignored: times are taken as local.
pub fn find_timestamp(line: &str) -> Option<i64> {
    let cleaned: String = line
        .chars()
        .map(|c| match c {
            ',' | '[' | ']' | '(' | ')' | '|' | ';' | '"' | '\'' | '<' | '>' => ' ',
            _ => c,
        })
        .collect();
    let mut toks: Vec<&str> = Vec::new();
    for raw in cleaned.split_whitespace() {
        // Split ISO "2026-03-02T11:00:05" into date and time.
        if let Some(pos) = raw.find('T') {
            let (a, b) = raw.split_at(pos);
            if date_token(a).is_some() {
                toks.push(a);
                toks.push(&b[1..]);
                continue;
            }
        }
        toks.push(raw);
    }

    let mut date = None;
    let mut time = None;
    let mut i = 0;
    while i < toks.len() {
        let tok = toks[i];
        if date.is_none() {
            if let Some(d) = date_token(tok) {
                date = Some(d);
                i += 1;
                continue;
            }
            if let Some(v) = name_date(&toks, i) {
                date = Some(v);
                i += 3;
                continue;
            }
        }
        if time.is_none()
            && let Some((h, m, s, pm)) = time_token(tok)
        {
            let mut hour = Some(h);
            let marker = pm.or_else(|| {
                toks.get(i + 1)
                    .and_then(|n| match n.to_ascii_lowercase().trim_end_matches('.') {
                        "am" | "a.m" => Some(false),
                        "pm" | "p.m" => Some(true),
                        _ => None,
                    })
            });
            if let Some(p) = marker {
                hour = apply_ampm(h, p);
            }
            if let Some(hh) = hour {
                time = Some((hh, m, s));
            }
        }
        i += 1;
    }
    match (date, time) {
        (Some((y, mo, d)), Some((h, mi, s))) => Some(local_secs(y, mo, d, h, mi, s)),
        _ => None,
    }
}

/// Parse a UTC offset such as "-5", "-05:00", "+0530", "0".
pub fn parse_offset(s: &str) -> Option<i64> {
    let (sign, rest) = match s.as_bytes().first()? {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, s),
    };
    let (h, m) = if let Some((h, m)) = rest.split_once(':') {
        (num(h, 1, 2)?, num(m, 2, 2)?)
    } else if rest.len() == 4 {
        (num(&rest[..2], 2, 2)?, num(&rest[2..], 2, 2)?)
    } else {
        (num(rest, 1, 2)?, 0)
    };
    if h > 14 || m > 59 {
        return None;
    }
    Some(sign * i64::from(h * 3600 + m * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for days in [-1000, 0, 1, 365, 10_957, 20_513, 30_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(fmt_date(days_from_civil(2024, 2, 29)), "2024-02-29");
        assert_eq!(weekday(0), 4); // Thursday
        assert_eq!(weekday(days_from_civil(2026, 3, 1)), 0); // Sunday
    }

    #[test]
    fn timestamps_in_common_styles() {
        let want = local_secs(2026, 3, 2, 11, 0, 5);
        for line in [
            "2026-03-02 11:00:05 Received",
            "2026-03-02T11:00:05Z",
            "2026-03-02T11:00:05-06:00",
            "2026-03-02 11:00:05.250+05:30",
            "03/02/2026 11:00:05",
            "3/2/26 11:00:05 AM",
            "Date: Mon, 2 Mar 2026 11:00:05 -0600",
            "Alert Received 11:00:05 Mar 02, 2026",
            "March 2, 2026 at 11:00:05am",
            "[11:00:05] 2026/03/02",
        ] {
            assert_eq!(find_timestamp(line), Some(want), "{line}");
        }
        assert_eq!(
            find_timestamp("3/2/2026 11:00 PM"),
            Some(local_secs(2026, 3, 2, 23, 0, 0))
        );
        assert_eq!(
            find_timestamp("3/2/2026 12:10 AM"),
            Some(local_secs(2026, 3, 2, 0, 10, 0))
        );
    }

    #[test]
    fn no_timestamp_in_headers_or_time_only_lines() {
        assert_eq!(
            find_timestamp("ZCZC-EAS-RWT-012345+0015-0611500-WXYZ/FM -"),
            None
        );
        assert_eq!(find_timestamp("Alert Received 11:00:05"), None);
        assert_eq!(find_timestamp("2026-02-30 11:00"), None);
    }

    #[test]
    fn offsets() {
        assert_eq!(parse_offset("-5"), Some(-5 * 3600));
        assert_eq!(parse_offset("-05:00"), Some(-5 * 3600));
        assert_eq!(parse_offset("+0530"), Some(5 * 3600 + 1800));
        assert_eq!(parse_offset("0"), Some(0));
        assert_eq!(parse_offset("x"), None);
    }
}
