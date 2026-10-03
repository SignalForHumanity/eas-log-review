use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_eas-log-review");

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(BIN).args(args).output().expect("run binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn temp_file(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eas-log-review-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

const FULL: &[&str] = &[
    "report",
    "--station",
    "WXYZ/FM",
    "--source",
    "LP-1=WAAA/FM",
    "--source",
    "LP-2=WBBB/AM",
    "--source",
    "IPAWS=IPAWSOPN",
    "--month",
    "2026-03",
    "--from",
    "2026-03-01",
];

fn week_line<'a>(out: &'a str, start: &str) -> &'a str {
    out.lines()
        .find(|l| l.starts_with(start))
        .unwrap_or_else(|| panic!("no week {start} in\n{out}"))
}

#[test]
fn three_log_styles_give_identical_reviews() {
    let mut outputs = Vec::new();
    for f in ["sage-style.txt", "alerts.mbox", "export.csv"] {
        let mut args = FULL.to_vec();
        let path = fixture(f);
        args.push(&path);
        let (code, out, err) = run(&args);
        assert_eq!(code, 2, "{f}: {err}");
        outputs.push(out);
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);

    let out = &outputs[0];
    let w1 = week_line(out, "2026-03-01");
    assert!(
        w1.contains("RWT       RWT       RWT       RWT") && w1.ends_with(" ok"),
        "{w1}"
    );
    assert!(week_line(out, "2026-03-08").ends_with("ok (RMT week)"));
    let w3 = week_line(out, "2026-03-15");
    assert!(w3.contains("--") && w3.ends_with("PROBLEM"), "{w3}");
    assert!(week_line(out, "2026-03-22").ends_with("PROBLEM"));
    assert!(week_line(out, "2026-03-29").contains("not judged: log ends 2026-04-03"));
    assert!(out.contains("Problems (2):"));
    assert!(out.contains("Week of 2026-03-15: no test or alert received from LP-2 (WBBB/AM)"));
    assert!(out.contains("Week of 2026-03-22: no RWT, RMT or NPT transmitted"));
    assert!(out.contains(
        "RMT Required Monthly Test from LP-1 (WAAA/FM); relayed 2026-03-11 10:45 (15 min)"
    ));
    assert!(out.contains(
        "TOR Tornado Warning from KDMX/NWS; National Weather Service; areas 019153 019169"
    ));
}

#[test]
fn without_from_first_week_is_not_judged() {
    let mut args: Vec<&str> = FULL[..FULL.len() - 2].to_vec();
    let path = fixture("export.csv");
    args.push(&path);
    let (_, out, _) = run(&args);
    assert!(week_line(&out, "2026-03-01").contains("not judged: log starts 2026-03-02"));
    assert!(out.contains("pass --from 2026-03-01"));
}

#[test]
fn merging_files_dedups_repeated_entries() {
    let mut args = FULL.to_vec();
    let a = fixture("export.csv");
    let b = fixture("alerts.mbox");
    args.push(&a);
    args.push(&b);
    let csv_path = temp_file("merged.csv", "");
    let csv_s = csv_path.display().to_string();
    args.push("--csv");
    args.push(&csv_s);
    let (code, _, _) = run(&args);
    assert_eq!(code, 2);
    let csv = std::fs::read_to_string(&csv_path).unwrap();
    // 19 distinct events plus the header row.
    assert_eq!(csv.lines().count(), 20, "{csv}");
    assert!(csv.lines().nth(1).unwrap().starts_with(
        "2026-03-02 10:00,log,received,LP-1,WAAA/FM,EAS,RWT,Required Weekly Test,019153"
    ));
    assert!(csv.contains(",sent,,WXYZ/FM,CIV,RMT,"));
}

const RMT_LATE: &str = "\
2026-05-05 21:00:00 ZCZC-CIV-RMT-019000+0015-1260200-WAAA/FM -
2026-05-05 22:30:00 ZCZC-CIV-RMT-019000+0015-1260200-WXYZ/FM -
2026-06-09 21:00:00 ZCZC-CIV-RMT-019000+0015-1610200-WAAA/FM -
";

#[test]
fn late_and_missing_relays_are_problems() {
    let p = temp_file("late.txt", RMT_LATE);
    let ps = p.display().to_string();
    let (code, out, _) = run(&["report", "--station", "WXYZ/FM", "--month", "2026-05", &ps]);
    assert_eq!(code, 2);
    assert!(
        out.contains("retransmitted after 90 minutes; the limit is 60"),
        "{out}"
    );
    let (_, out, _) = run(&["report", "--station", "WXYZ/FM", "--month", "2026-06", &ps]);
    assert!(out.contains("was not retransmitted"), "{out}");
    assert!(out.contains("NOT RELAYED"));
}

#[test]
fn relay_just_over_60_minutes_is_late() {
    let p = temp_file(
        "late60.txt",
        "2026-05-05 10:00:00 ZCZC-CIV-RMT-019000+0015-1261600-WAAA/FM -\n\
         2026-05-05 11:00:40 ZCZC-CIV-RMT-019000+0015-1261600-WXYZ/FM -\n",
    );
    let ps = p.display().to_string();
    let (code, out, _) = run(&["report", "--station", "WXYZ/FM", "--month", "2026-05", &ps]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("the limit is 60"), "{out}");
}

#[test]
fn header_time_without_utc_offset_warns() {
    let p = temp_file(
        "undated2.txt",
        "Alert Received 10:00:05\nZCZC-EAS-RWT-019153+0015-0611600-WAAA/FM -\n",
    );
    let ps = p.display().to_string();
    let (_, _, err) = run(&["report", "--year", "2026", &ps]);
    assert!(err.contains("--utc-offset"), "{err}");
    let (_, _, err) = run(&["report", "--year", "2026", "--utc-offset", "-6", &ps]);
    assert!(!err.contains("--utc-offset"), "{err}");
}

#[test]
fn decoder_only_does_not_expect_sent_headers() {
    let p = temp_file("lpfm.txt", RMT_LATE);
    let ps = p.display().to_string();
    let (_, out, _) = run(&[
        "report",
        "--station",
        "WXYZ/LP",
        "--decoder-only",
        "--month",
        "2026-06",
        &ps,
    ]);
    assert!(!out.contains("not retransmitted"), "{out}");
    assert!(out.contains("confirm the test was aired within 60 minutes"));
    assert!(out.contains("Mode: decoder only"));
}

#[test]
fn missing_rmt_in_fully_covered_month() {
    let log = "\
2026-07-01 10:00 ZCZC-EAS-RWT-019153+0015-1821500-WAAA/FM -
2026-07-08 10:00 ZCZC-EAS-RWT-019153+0015-1891500-WAAA/FM -
2026-07-15 10:00 ZCZC-EAS-RWT-019153+0015-1961500-WAAA/FM -
2026-07-22 10:00 ZCZC-EAS-RWT-019153+0015-2031500-WAAA/FM -
2026-07-29 10:00 ZCZC-EAS-RWT-019153+0015-2101500-WAAA/FM -
2026-08-01 10:00 ZCZC-EAS-RWT-019153+0015-2131500-WAAA/FM -
";
    let p = temp_file("july.txt", log);
    let ps = p.display().to_string();
    let (code, out, _) = run(&[
        "report",
        "--source",
        "LP-1=WAAA/FM",
        "--month",
        "2026-07",
        "--from",
        "2026-06-28",
        &ps,
    ]);
    assert_eq!(code, 2);
    assert!(
        out.contains("No Required Monthly Test (or national test) received in 2026-07"),
        "{out}"
    );
    assert!(
        !out.contains("no test or alert received from LP-1"),
        "{out}"
    );
}

#[test]
fn monday_weeks_and_prefix_sources() {
    let log = "\
2026-03-02 10:00 ZCZC-EAS-RWT-019153+0015-0611600-WAAA/FM -
2026-03-08 10:00 ZCZC-EAS-RWT-019153+0015-0671600-WAAA/FM -
";
    let p = temp_file("mon.txt", log);
    let ps = p.display().to_string();
    let (_, out, _) = run(&[
        "report",
        "--source",
        "LP-1=WAAA*",
        "--week-start",
        "monday",
        &ps,
    ]);
    assert!(out.contains("Week (Mon-Sun)"));
    let w = week_line(&out, "2026-03-02");
    assert!(w.contains("RWT") && w.ends_with(" ok"), "{out}");
}

#[test]
fn no_sources_checks_any_reception_and_says_so() {
    let p = temp_file("nosrc.txt", RMT_LATE);
    let ps = p.display().to_string();
    let (_, out, _) = run(&["report", "--month", "2026-05", &ps]);
    assert!(out.contains("Received"));
    assert!(out.contains("No --source given"));
}

#[test]
fn year_needed_for_undated_logs() {
    let p = temp_file(
        "undated.txt",
        "Alert Received 10:00:05\nZCZC-EAS-RWT-019153+0015-0611600-WAAA/FM -\n",
    );
    let ps = p.display().to_string();
    let (code, _, err) = run(&["report", &ps]);
    assert_eq!(code, 1);
    assert!(err.contains("pass --year"));
    let (_, out, _) = run(&["report", "--year", "2026", "--utc-offset", "-6", &ps]);
    assert!(out.contains("EAS LOG REVIEW: March 2026"), "{out}");
}

#[test]
fn senders_lists_ids() {
    let path = fixture("export.csv");
    let (code, out, _) = run(&["senders", &path]);
    assert_eq!(code, 0);
    assert!(out.lines().nth(1).unwrap().starts_with("IPAWSOPN"));
    assert!(out.contains("KDMX/NWS"));
}

#[test]
fn decode_prints_plain_english() {
    let (code, out, _) = run(&["decode", "ZCZC-WXR-TOR-019153+0030-0781523-KDMX/NWS-"]);
    assert_eq!(code, 0);
    assert!(out.contains("Tornado Warning"));
    assert!(out.contains("National Weather Service"));
    assert!(out.contains("day 078 15:23 UTC"));
    let (code, _, _) = run(&["decode", "ZCZC-junk"]);
    assert_eq!(code, 1);
}

#[test]
fn bad_options_fail_cleanly() {
    let (code, _, err) = run(&["report", "--source", "nope", "x"]);
    assert_eq!(code, 1);
    assert!(err.contains("NAME=SENDERID"));
    let (code, _, _) = run(&["report", "--month", "2026-13", "x"]);
    assert_eq!(code, 1);
}

/// 47 CFR 11.61(a)(3)(ii) and (a)(4): a national test replaces the weekly
/// and monthly tests, and a relayed activation can stand in for the weekly test.
const OCT: &[&str] = &[
    "report",
    "--station",
    "WXYZ/FM",
    "--source",
    "LP-1=WAAA/FM",
    "--month",
    "2026-10",
    "--from",
    "2026-10-01",
];

fn csv(rows: &[&str]) -> String {
    let mut s = String::from("Date,Time,Type,Event,Header\n");
    for r in rows {
        s.push_str(r);
        s.push('\n');
    }
    s
}

#[test]
fn national_test_replaces_weekly_and_monthly_tests() {
    let log = csv(&[
        // Weeks of 09-27 and 10-25 are partial; 10-04 has only the NPT.
        "2026-10-01,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-2741600-WAAA/FM -",
        "2026-10-01,11:00:05,Sent,RWT,ZCZC-EAS-RWT-019153+0015-2741700-WXYZ/FM -",
        "2026-10-07,13:20:05,Received,NPT,ZCZC-PEP-NPT-000000+0015-2801820-WAAA/FM -",
        "2026-10-07,13:25:05,Sent,NPT,ZCZC-PEP-NPT-000000+0015-2801820-WXYZ/FM -",
        "2026-10-13,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-2861600-WAAA/FM -",
        "2026-10-13,11:00:05,Sent,RWT,ZCZC-EAS-RWT-019153+0015-2861700-WXYZ/FM -",
        "2026-10-20,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-2931600-WAAA/FM -",
        "2026-10-20,11:00:05,Sent,RWT,ZCZC-EAS-RWT-019153+0015-2931700-WXYZ/FM -",
        "2026-10-27,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-3001600-WAAA/FM -",
        "2026-10-27,11:00:05,Sent,RWT,ZCZC-EAS-RWT-019153+0015-3001700-WXYZ/FM -",
        "2026-11-02,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-3061600-WAAA/FM -",
    ]);
    let p = temp_file("npt.csv", &log);
    let mut args = OCT.to_vec();
    args.push(p.to_str().unwrap());
    let (code, out, err) = run(&args);
    assert!(!week_line(&out, "2026-10-04").ends_with("PROBLEM"), "{out}");
    assert!(!out.contains("No Required Monthly Test"), "{out}");
    assert_eq!(code, 0, "{out}\n{err}");
}

#[test]
fn relayed_activation_stands_in_for_the_weekly_test() {
    let log = csv(&[
        "2026-10-01,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-2741600-WAAA/FM -",
        "2026-10-01,11:00:05,Sent,RWT,ZCZC-EAS-RWT-019153+0015-2741700-WXYZ/FM -",
        "2026-10-05,15:00:05,Received,TOR,ZCZC-WXR-TOR-019153+0030-2782100-WAAA/FM -",
        "2026-10-05,15:01:05,Sent,TOR,ZCZC-WXR-TOR-019153+0030-2782100-WXYZ/FM -",
        "2026-10-14,10:30:05,Received,RMT,ZCZC-CIV-RMT-019000+0015-2871630-WAAA/FM -",
        "2026-10-14,10:45:05,Sent,RMT,ZCZC-CIV-RMT-019000+0015-2871630-WXYZ/FM -",
        "2026-10-20,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-2931600-WAAA/FM -",
        "2026-10-20,11:00:05,Sent,RWT,ZCZC-EAS-RWT-019153+0015-2931700-WXYZ/FM -",
        "2026-11-02,10:00:05,Received,RWT,ZCZC-EAS-RWT-019153+0015-3061600-WAAA/FM -",
    ]);
    let p = temp_file("activation.csv", &log);
    let mut args = OCT.to_vec();
    args.push(p.to_str().unwrap());
    let (_, out, _) = run(&args);
    assert!(!week_line(&out, "2026-10-04").ends_with("PROBLEM"), "{out}");
    assert!(
        out.contains("relayed TOR. It counts as the weekly test if it carried"),
        "{out}"
    );
    assert!(out.contains("11.61(a)(4)"), "{out}");
}

#[test]
fn ean_is_not_held_to_the_60_minute_test_rule() {
    let log = csv(&[
        "2026-10-07,13:00:05,Received,EAN,ZCZC-PEP-EAN-000000+0100-2801700-WAAA/FM -",
        "2026-10-07,14:30:05,Sent,EAN,ZCZC-PEP-EAN-000000+0100-2801700-WXYZ/FM -",
    ]);
    let p = temp_file("ean.csv", &log);
    let mut args = OCT.to_vec();
    args.push(p.to_str().unwrap());
    let (_, out, _) = run(&args);
    assert!(!out.contains("the limit is 60"), "{out}");
    assert!(
        out.contains("EAN Emergency Action Notification") || out.contains("EAN received"),
        "{out}"
    );
    assert!(out.contains("relayed after 90 minute(s)"), "{out}");
}

#[test]
fn missed_test_logging_cites_11_35_a() {
    let mut args = FULL.to_vec();
    let path = fixture("export.csv");
    args.push(&path);
    let (_, out, _) = run(&args);
    assert!(
        out.contains("47 CFR 11.35(a)") && !out.contains("11.35(b)"),
        "{out}"
    );
}
