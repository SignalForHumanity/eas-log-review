# eas-log-review

Turn your EAS decoder's logs into the weekly and monthly review the FCC
expects. For each week it shows whether a test or alert came in from **each**
monitoring source. It also checks that the Required Monthly Test was received
and relayed within 60 minutes, and lists everything you need to explain in
the station log.

Repository: https://github.com/SignalForHumanity/eas-log-review ·
Why this exists: [PROBLEM.md](PROBLEM.md)

## The problem

Every US broadcast station must monitor two assigned EAS sources plus
IPAWS. Each week it must receive a test or alert from each of them, and it
must relay the monthly RMT within 60 minutes. Everything must be logged and
reviewed weekly (47 CFR 11.35, 11.52, 11.61). The EAS box records the raw
headers, but someone still has to read them source by source, week by week.
In practice that step gets skipped. A source that went quiet weeks ago, or a
late RMT, is one of the most common reasons the FCC fines small stations.

## Who it is for

Volunteer engineers and managers at LPFM community stations and small AM/FM
stations, and contract engineers who look after several stations. You need
to be able to save your EAS box's log or alert e-mails to a folder and run
one command.

## Install

Download a release binary for your platform, or build from source with Rust
installed:

```
cargo install --git https://github.com/SignalForHumanity/eas-log-review
```

It has no dependencies, needs no network and has no settings file.

## Usage

1. Get your logs out of the EAS box as text. Any of these work:
   - a log export or "print to file" from the web interface,
   - a capture of the serial/printer port,
   - a folder of saved alert e-mails (`.eml`) or a mail folder exported as
     mbox (many stations already send every event to an EAS mailbox),
   - a CSV or spreadsheet saved as CSV.

   The tool looks for SAME headers (`ZCZC-...`), so the vendor format does
   not matter.

2. Find the sender IDs of your sources:

   ```
   $ eas-log-review senders eas-logs/
   sender      count  first       last        events
   IPAWSOPN        5  2026-03-02  2026-03-30  RWT
   WAAA/FM         5  2026-03-02  2026-04-01  RWT RMT
   WBBB/AM         4  2026-03-03  2026-03-31  RWT RMT
   WXYZ/FM         4  2026-03-04  2026-04-03  RWT RMT
   KDMX/NWS        1  2026-03-19  2026-03-19  TOR
   ```

3. Run the review:

   ```
   $ eas-log-review report --station WXYZ/FM \
       --source LP-1=WAAA/FM --source LP-2=WBBB/AM --source IPAWS=IPAWSOPN \
       --month 2026-03 --from 2026-03-01 eas-logs/

   EAS LOG REVIEW: March 2026
   ============================================================
   Station sender ID: WXYZ/FM
   Source LP-1: WAAA/FM
   Source LP-2: WBBB/AM
   Source IPAWS: IPAWSOPN

   Week (Sun-Sat)            LP-1      LP-2      IPAWS     Sent      Result
   2026-03-01 .. 03-07       RWT       RWT       RWT       RWT       ok
   2026-03-08 .. 03-14       RMT       RMT       RWT       RMT       ok (RMT week)
   2026-03-15 .. 03-21       RWT       --        RWT       RWT       PROBLEM
   2026-03-22 .. 03-28       RWT       RWT       RWT       --        PROBLEM
   2026-03-29 .. 04-04       RWT       RWT       RWT       RWT       not judged: log ends 2026-04-03

   Monthly and national tests
     2026-03-11 10:30  RMT Required Monthly Test from LP-1 (WAAA/FM); relayed 2026-03-11 10:45 (15 min)

   Other alerts received
     2026-03-19 15:24  TOR Tornado Warning from KDMX/NWS; National Weather Service; areas 019153 019169

   Problems (2):
     - Week of 2026-03-15: no test or alert received from LP-2 (WBBB/AM). Enter the reason in the station log (47 CFR 11.35(a)).
     - Week of 2026-03-22: no RWT, RMT or NPT transmitted by the station (47 CFR 11.61(a)(2)).

   Weekly review
     Week of 2026-03-01  Reviewed by: ______________________  Date: __________
     ...
   ```

   Print it, sign the weekly lines, and file it with the station log.

**LPFM / decoder-only stations:** add `--decoder-only`. The tool then does
not expect transmitted headers. It reminds you to log that the RMT test
script was aired within 60 minutes.

### Options

| Option | Meaning |
|---|---|
| `--station ID` | Your own sender ID, as in the headers you send. Headers with this ID count as *sent*. All others count as *received*. Repeatable. |
| `--source NAME=ID` | A monitoring assignment from your state EAS plan. End the ID with `*` to match by prefix. Repeatable. |
| `--decoder-only` | Don't expect sent headers (typical LPFM). |
| `--month YYYY-MM` | Review one month. Default: every month in the logs. |
| `--from` / `--to YYYY-MM-DD` | The days your logs cover. Default: the first and last entry. Weeks that are not fully covered are shown but not judged. |
| `--year YYYY` | Year for logs that contain no dates at all. |
| `--utc-offset H[:MM]` | Your offset from UTC. It is used only when a time has to come from the header itself (e.g. `-6`). Default 0 (UTC); the tool warns when it had to use header times without this option. |
| `--week-start monday` | Use Monday–Sunday weeks instead of Sunday–Saturday. |
| `--csv FILE` | Also write every event (time, direction, source, decoded fields, file and line) to CSV. |

`eas-log-review decode 'ZCZC-...'` explains a single header in plain
English.

Exit status: `0` no problems, `2` problems found, `1` error. A weekly cron
job such as `eas-log-review report ... || mail -s "EAS log problems" ...`
warns you before an inspector does.

## How it works

- **Source and direction come from the header itself.** When a station
  relays an alert, it puts its own sender ID in the header (47 CFR 11.31).
  So the last field of each header tells you who sent it.
- **Time.** For each header the tool uses the nearest date and time written
  before it, or on the same line. If there is none, it uses the e-mail's
  `Date:`. Many styles are recognised: `2026-03-02 10:00:05`, `03/02/26
  10:00 AM`, `Mon, 2 Mar 2026 10:00:05`, `Mar 02, 2026 10:00:05`, etc. If no
  usable timestamp exists, or it is more than 36 hours from the header's own
  UTC issue time, the issue time plus `--utc-offset` is used instead. Such
  entries are marked `[time from header]`.
- **Duplicates.** The same header seen again within 15 minutes counts once.
  You can therefore feed in e-mails and an export together.
- **Rules applied** (from the FCC rules, the 2021 EAS Operating Handbook
  and the REC Networks LPFM checklist):
  - Every fully covered week needs at least one received message (test or
    alert) from each `--source`. In a week with an RMT or a national test
    (NPT), a missing source is only noted, because that may be the only test
    that week.
  - Unless `--decoder-only` is set, every week needs a sent RWT, RMT or NPT.
    A week with only a relayed activation (such as a TOR) is noted, not
    flagged: an activation sent with the EAS header and EOM codes can stand
    in for the weekly test (47 CFR 11.61(a)(4)), which the log cannot confirm.
  - Every received RMT or NPT must be relayed: a sent header with the same
    event and issue time, within 60 minutes (47 CFR 11.61(a)(1)). A received
    EAN must be relayed immediately; its relay delay is reported, not judged.
  - A fully covered month needs an RMT or an NPT. A national test replaces
    the weekly and monthly tests (47 CFR 11.61(a)(3)(ii)).
  - Missed tests must be explained in the station log (47 CFR 11.35(a)).

## Data sources

Only your own log files. The rule logic comes from 47 CFR Part 11
(https://www.ecfr.gov/current/title-47/chapter-I/subchapter-A/part-11), the
FCC EAS Operating Handbook (2021) and the REC Networks LPFM EAS checklist
(https://recnet.com/checklist-eas). Event code names follow 47 CFR 11.31.

## Limitations

- **Not legal advice.** It reports what your logs show. Your state EAS plan,
  your logs and the FCC rules decide what compliance means for you.
- The tool can only see what is in the logs. If the box was off and logged
  nothing, it reports "nothing received", which is correct but doesn't
  explain why.
- Log timestamps are taken as local time, and zone suffixes are ignored.
  The header fallback uses one fixed `--utc-offset`, so it is off by an hour
  across daylight-saving changes. Week and date checks tolerate this.
- Base64-encoded e-mail bodies are not decoded. Plain text and
  quoted-printable are.
- Headers broken across lines by a printer are not reassembled.
- Whether an LPFM aired the test script is not in any decoder log. The tool
  reminds you to log it.
- The test fixtures are synthetic. They are modelled on the record layout of
  Sage ENDEC serial output (as parsed by DigiDEC), on alert e-mails and on
  CSV exports. Reports of real log formats that don't parse are welcome.

## Development

```
cargo test        # unit tests plus CLI tests on fixtures, no network
cargo clippy --all-targets -- -D warnings
```

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
