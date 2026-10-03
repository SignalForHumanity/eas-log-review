//! SAME (Specific Area Message Encoding) header parsing, 47 CFR 11.31.
//!
//! `ZCZC-ORG-EEE-PSSCCC-PSSCCC+TTTT-JJJHHMM-LLLLLLLL-`

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub org: String,
    pub event: String,
    pub locations: Vec<String>,
    /// Valid period, as written (HHMM).
    pub purge: String,
    /// Day of year of issue, 1-366 (UTC).
    pub issue_doy: u32,
    pub issue_hour: u32,
    pub issue_min: u32,
    /// Sender ID (station call sign or NWS office), trimmed.
    pub sender: String,
}

impl Header {
    /// Canonical text form, used for display and de-duplication.
    pub fn canonical(&self) -> String {
        format!(
            "ZCZC-{}-{}-{}+{}-{:03}{:02}{:02}-{}-",
            self.org,
            self.event,
            self.locations.join("-"),
            self.purge,
            self.issue_doy,
            self.issue_hour,
            self.issue_min,
            self.sender
        )
    }

    /// Key that identifies one alert across relays: the issue time and event
    /// code stay the same when a station retransmits, while the sender changes.
    pub fn alert_key(&self) -> (String, u32, u32, u32) {
        (
            self.event.clone(),
            self.issue_doy,
            self.issue_hour,
            self.issue_min,
        )
    }

    pub fn issue_text(&self) -> String {
        format!(
            "day {:03} {:02}:{:02} UTC",
            self.issue_doy, self.issue_hour, self.issue_min
        )
    }
}

fn digits(b: &[u8], at: usize, n: usize) -> Option<&str> {
    let s = b.get(at..at + n)?;
    if s.iter().all(u8::is_ascii_digit) {
        std::str::from_utf8(s).ok()
    } else {
        None
    }
}

/// Try to parse a header starting exactly at `start` (where "ZCZC-" begins).
/// Returns the header and the byte index just after it.
pub fn parse_at(text: &str, start: usize) -> Option<(Header, usize)> {
    let b = text.as_bytes();
    if b.get(start..start + 5)? != b"ZCZC-" {
        return None;
    }
    let mut i = start + 5;
    let org = b.get(i..i + 3)?;
    if !org.iter().all(u8::is_ascii_uppercase) || *b.get(i + 3)? != b'-' {
        return None;
    }
    i += 4;
    let ev = b.get(i..i + 3)?;
    if !ev
        .iter()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        || *b.get(i + 3)? != b'-'
    {
        return None;
    }
    i += 4;
    let mut locations = Vec::new();
    loop {
        locations.push(digits(b, i, 6)?.to_string());
        i += 6;
        match *b.get(i)? {
            b'-' => i += 1,
            b'+' => {
                i += 1;
                break;
            }
            _ => return None,
        }
        if locations.len() > 31 {
            return None;
        }
    }
    let purge = digits(b, i, 4)?.to_string();
    i += 4;
    if *b.get(i)? != b'-' {
        return None;
    }
    i += 1;
    let issue = digits(b, i, 7)?;
    let doy: u32 = issue[..3].parse().ok()?;
    let hour: u32 = issue[3..5].parse().ok()?;
    let min: u32 = issue[5..7].parse().ok()?;
    if !(1..=366).contains(&doy) || hour > 23 || min > 59 {
        return None;
    }
    i += 7;
    if *b.get(i)? != b'-' {
        return None;
    }
    i += 1;
    let s_start = i;
    while i < b.len() && i - s_start < 8 {
        let c = b[i];
        // Sender IDs are call signs or office IDs (letters, digits, '/'),
        // space padded. Anything else ends the field: '-', line ends, and
        // the delimiters of CSV, HTML or quoted exports.
        if !(c.is_ascii_alphanumeric() || matches!(c, b'/' | b' ' | b'.' | b'_')) {
            break;
        }
        i += 1;
    }
    let sender = text[s_start..i].trim().to_ascii_uppercase();
    if sender.is_empty() {
        return None;
    }
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    Some((
        Header {
            org: String::from_utf8_lossy(org).into_owned(),
            event: String::from_utf8_lossy(ev).into_owned(),
            locations,
            purge,
            issue_doy: doy,
            issue_hour: hour,
            issue_min: min,
            sender,
        },
        i,
    ))
}

/// Find every valid header in a text, with its byte range.
pub fn find_all(text: &str) -> Vec<(usize, usize, Header)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = text[from..].find("ZCZC-") {
        let start = from + pos;
        if let Some((h, end)) = parse_at(text, start) {
            out.push((start, end, h));
            from = end;
        } else {
            from = start + 5;
        }
    }
    out
}

pub fn org_name(code: &str) -> &'static str {
    match code {
        "EAS" => "Broadcast station or cable system",
        "CIV" => "Civil authorities",
        "WXR" => "National Weather Service",
        "PEP" => "Primary Entry Point System",
        _ => "Unknown originator",
    }
}

/// Monthly and national tests (relayed within 60 minutes) and the national
/// activation (EAN, relayed immediately): all are checked for retransmission.
pub fn must_relay(code: &str) -> bool {
    matches!(code, "RMT" | "NPT" | "EAN")
}

pub fn event_name(code: &str) -> &'static str {
    match code {
        "EAN" => "Emergency Action Notification",
        "NIC" => "National Information Center",
        "NPT" => "National Periodic Test",
        "RMT" => "Required Monthly Test",
        "RWT" => "Required Weekly Test",
        "DMO" => "Practice/Demo Warning",
        "NMN" => "Network Message Notification",
        "ADR" => "Administrative Message",
        "AVA" => "Avalanche Watch",
        "AVW" => "Avalanche Warning",
        "BHW" => "Biological Hazard Warning",
        "BLU" => "Blue Alert",
        "BWW" => "Boil Water Warning",
        "BZW" => "Blizzard Warning",
        "CAE" => "Child Abduction Emergency",
        "CDW" => "Civil Danger Warning",
        "CEM" => "Civil Emergency Message",
        "CFA" => "Coastal Flood Watch",
        "CFW" => "Coastal Flood Warning",
        "CHW" => "Chemical Hazard Warning",
        "CWW" => "Contaminated Water Warning",
        "DBA" => "Dam Watch",
        "DBW" => "Dam Break Warning",
        "DEW" => "Contagious Disease Warning",
        "DSW" => "Dust Storm Warning",
        "EQW" => "Earthquake Warning",
        "EVA" => "Evacuation Watch",
        "EVI" => "Evacuation Immediate",
        "EWW" => "Extreme Wind Warning",
        "FCW" => "Food Contamination Warning",
        "FFA" => "Flash Flood Watch",
        "FFS" => "Flash Flood Statement",
        "FFW" => "Flash Flood Warning",
        "FLA" => "Flood Watch",
        "FLS" => "Flood Statement",
        "FLW" => "Flood Warning",
        "FRW" => "Fire Warning",
        "FSW" => "Flash Freeze Warning",
        "FZW" => "Freeze Warning",
        "HLS" => "Hurricane Local Statement",
        "HMW" => "Hazardous Materials Warning",
        "HUA" => "Hurricane Watch",
        "HUW" => "Hurricane Warning",
        "HWA" => "High Wind Watch",
        "HWW" => "High Wind Warning",
        "IBW" => "Iceberg Warning",
        "IFW" => "Industrial Fire Warning",
        "LAE" => "Local Area Emergency",
        "LEW" => "Law Enforcement Warning",
        "LSW" => "Land Slide Warning",
        "MEP" => "Missing and Endangered Persons",
        "NUW" => "Nuclear Power Plant Warning",
        "POS" => "Power Outage Statement",
        "RHW" => "Radiological Hazard Warning",
        "SMW" => "Special Marine Warning",
        "SPS" => "Special Weather Statement",
        "SPW" => "Shelter in Place Warning",
        "SQW" => "Snow Squall Warning",
        "SSA" => "Storm Surge Watch",
        "SSW" => "Storm Surge Warning",
        "SVA" => "Severe Thunderstorm Watch",
        "SVR" => "Severe Thunderstorm Warning",
        "SVS" => "Severe Weather Statement",
        "TOA" => "Tornado Watch",
        "TOE" => "911 Telephone Outage Emergency",
        "TOR" => "Tornado Warning",
        "TRA" => "Tropical Storm Watch",
        "TRW" => "Tropical Storm Warning",
        "TSA" => "Tsunami Watch",
        "TSW" => "Tsunami Warning",
        "VOW" => "Volcano Warning",
        "WFA" => "Wild Fire Watch",
        "WFW" => "Wild Fire Warning",
        "WSA" => "Winter Storm Watch",
        "WSW" => "Winter Storm Warning",
        _ => "Unknown event code",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_header() {
        let t = "xx ZCZC-WXR-TOR-031109-031111+0030-0611523-KOKX/NWS- yy";
        let all = find_all(t);
        assert_eq!(all.len(), 1);
        let (s, e, h) = &all[0];
        assert_eq!(
            &t[*s..*e],
            "ZCZC-WXR-TOR-031109-031111+0030-0611523-KOKX/NWS-"
        );
        assert_eq!(h.org, "WXR");
        assert_eq!(h.event, "TOR");
        assert_eq!(h.locations, vec!["031109", "031111"]);
        assert_eq!(h.purge, "0030");
        assert_eq!((h.issue_doy, h.issue_hour, h.issue_min), (61, 15, 23));
        assert_eq!(h.sender, "KOKX/NWS");
        assert_eq!(
            h.canonical(),
            "ZCZC-WXR-TOR-031109-031111+0030-0611523-KOKX/NWS-"
        );
        assert_eq!(event_name(&h.event), "Tornado Warning");
    }

    #[test]
    fn sender_with_spaces_and_no_trailing_dash() {
        let t = "ZCZC-EAS-RWT-012345+0015-0611500-WXYZ FM\nnext";
        let h = &find_all(t)[0].2;
        assert_eq!(h.sender, "WXYZ FM");
    }

    #[test]
    fn rejects_malformed() {
        for t in [
            "ZCZC-eas-RWT-012345+0015-0611500-WXYZ-",
            "ZCZC-EAS-RWT-01234+0015-0611500-WXYZ-",
            "ZCZC-EAS-RWT-012345+0015-3671500-WXYZ-",
            "ZCZC-EAS-RWT-012345+0015-0612500-WXYZ-",
            "ZCZC-EAS-RWT-012345-0015-0611500-WXYZ-",
            "ZCZC-EAS-RWT-012345+0015-0611500--",
            "ZCZC-EAS-RWT",
        ] {
            assert!(find_all(t).is_empty(), "{t}");
        }
    }

    #[test]
    fn sender_stops_at_csv_or_quote_delimiters() {
        for t in [
            "2026-03-02,\"ZCZC-EAS-RWT-012345+0015-0611500-WAAA/FM\",ok",
            "ZCZC-EAS-RWT-012345+0015-0611500-WAAA/FM,2026-03-02",
            "<td>ZCZC-EAS-RWT-012345+0015-0611500-WAAA/FM</td>",
        ] {
            assert_eq!(find_all(t)[0].2.sender, "WAAA/FM", "{t}");
        }
    }

    #[test]
    fn finds_several() {
        let t = "ZCZC-EAS-RWT-012345+0015-0611500-AAAA-ZCZC-EAS-RWT-012345+0015-0611500-BBBB-";
        let all = find_all(t);
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].2.sender, "BBBB");
    }
}
