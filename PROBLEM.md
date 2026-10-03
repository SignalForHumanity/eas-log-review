---
slug: eas-log-review
title: CLI that turns a radio station's EAS decoder logs into a weekly and monthly FCC compliance review, flagging missed tests per monitoring source
verdict: build
---

## Problem

Every US broadcast station, including about 2,000 volunteer-run low-power FM
(LPFM) community stations, must monitor at least two assigned Emergency Alert
System (EAS) sources plus FEMA IPAWS. Each week it must receive a test (RWT)
or an activation from each source. Each month it must receive the Required
Monthly Test (RMT) and air it within 60 minutes. It must also log everything
and review the log every week. If a test was missed, the log must say why
(47 CFR 11.35, 11.52, 11.61).

The EAS box keeps the raw data, as a list of decoded headers in memory, in
emails or on a serial printer. Nothing turns that list into the answer an
inspector wants: "for each week, did each source come in, and was the RMT
relayed in time?" Engineers do this by hand. Missed tests from one source go
unnoticed for weeks or months. That is one of the FCC's most common fines.

Sources of the need:

- The Broadcasters' Desktop Reference, "Keeping Control of Your EAS Logs"
  (https://www.thebdr.net/keeping-control-of-your-eas-logs/): "Reports from
  the field indicate a large number of stations that do not do this as
  required. Tests are missed and, often, the weekly review is not signed off."
  The author's fix is a hand-filled monthly Excel form.
- REC Networks LPFM checklist (https://recnet.com/checklist-eas) calls EAS
  "one of the hot items on the FCC's lists of things to fine stations for".
  Its advisory letter #9 (https://recnet.com/advisory-letter-9) tells LPFMs to
  "check the EAS logs on a weekly basis to assure that your decoder is
  receiving weekly or monthly tests ... from all three monitoring sources".
- RadioDiscussions, "EAS/CAP Equipment" for LPFM
  (https://radiodiscussions.com/threads/eas-cap-equipment.657795/): "Download
  the logs, reformat, look over them and type your name and save them."
  Budget LPFMs run old Sage 1822 boxes bought on eBay. These have no e-mailed
  reports.
- FCC enforcement such as DOC-237881A1
  (https://docs.fcc.gov/public/attachments/DOC-237881A1.pdf): a station "failed
  to receive a RWT from WPST between March 13, 2002 and May 12, 2002 ...".
  It is a months-long gap from one source that a weekly per-source check
  would have caught.

## Who benefits

The volunteer engineer or station manager at an LPFM or small AM/FM station,
and contract engineers who look after several such stations. They copy the
EAS box's log out as a text export, a saved e-mail folder (mbox/.eml) or
captured serial output. Then they run one command each week or month and
print the result for the station log. They would find the tool through REC
Networks, the BDR, SBE chapters and r/radio. A binary download needs no
install and no account.

## Existing solutions

- **The EAS box itself** (Sage ENDEC, Digital Alert Systems DASDEC,
  Gorman-Redlich, TFT). Newer units list events and can e-mail per-event
  messages or a periodic list. None of them checks per source and per week,
  and none checks RMT relay delay. A human still has to read the list.
  Older units (Sage 1822, common at LPFMs) only print or send serial text.
- **Paper or Excel monthly log forms**: the BDR form
  (https://www.thebdr.net/keeping-control-of-your-eas-logs/) and the Donelan
  blank log (https://www.donelan.com/easequip/EAS-Log-Blank-Month-Instructions.pdf).
  These are filled in by hand, and that is the step stations skip.
- **trevor229/DigiDEC** (https://github.com/trevor229/DigiDEC, 4 stars, last
  push 2025-03): a live Sage 1822 serial logger. It needs Apache, PHP, MySQL
  and a Discord webhook, works with one vendor, and has no compliance checks.
- **trininox/EAS-Logger** (https://github.com/trininox/EAS-Logger): a 2011
  PHP/MySQL web app for typing in entries by hand. Abandoned.
- **wbor-fm/wbor-endec** (https://github.com/wbor-fm/wbor-endec): forwards
  Sage alerts to news feeds. Not a log review.
- **SAME decoders** (`sameold`/`samedec` crates, f1r3-b1rd/SAMEDecode,
  browser header decoders): these decode audio or a single header. None
  works over a log.
- crates.io and GitHub searches ("EAS log", "ZCZC", "EAS compliance",
  "SAME header", "DASDEC log") found nothing else.

## Why build anything

Every existing option leaves the actual review, per source, per week, plus
RMT relay timing, to a person reading raw headers. That person is usually an
unpaid volunteer, and the FCC fines the result. The SAME header standard
already holds everything needed. Each relayed header carries the sender's
station ID, so the source and the direction (received or sent) can be found
without knowing the vendor's log format. No tool uses this.

## Smallest useful intervention

`eas-log-review`, a dependency-free single binary that reads any text that
contains SAME headers: log exports, serial captures, mbox or .eml files,
CSV. For each header it takes the nearest timestamp. It then prints, per
calendar month:

- a week-by-week table showing what came from each configured source
  (LP-1, LP-2, IPAWS, NWS...) and what the station sent, with every gap
  flagged;
- each RMT/NPT received, with relay time and the 60-minute check (or a
  reminder for decoder-only LPFMs);
- the other activations, decoded into plain English;
- a list of problems to explain in the log, and a weekly review signature
  block.

It also writes an optional CSV of every event and has a `senders` command to
help with setup. The exit code is non-zero when problems are found, so the
check can run from cron.

## Success criterion

- Tests on fixtures in three log styles (Sage-style serial text, mbox
  e-mails, CSV) produce exactly the expected per-week, per-source verdicts.
  This includes a week with a missing source, an RMT week, a late relay and
  an unrelayed RMT.
- The header parser decodes every field of valid headers and rejects
  malformed ones.
- A month of logs goes from export to a printable review in one command,
  in under a second.

## Maintenance

There is no server, no network and no dependencies, so very little can rot.
The SAME header format (47 CFR 11.31) has been stable since 1997. New event
codes only change display names, and unknown codes are still reported. The
vendor timestamp formats are the fragile part. The parser takes several
common date styles and falls back to the header's own UTC issue time. New
styles can be added as small, test-covered cases. A change to the FCC test
rules (for example the proposed EAS modernisation) would need the rule
checks updated. Those checks are in one small module.

## Decision

Build. The need is documented by the industry's own references and by FCC
enforcement. The people affected are small, often volunteer, stations doing
a manual review that they demonstrably skip. No existing tool does the
per-source weekly check over vendor logs. It is a narrow, offline,
standard-based tool with near-zero upkeep. The tool reports what the logs
show. It is not legal advice, and the README says so.

## Also considered

- tier2-t2s-viewer (read EPA Tier2 Submit .t2s chemical inventory files for LEPCs and fire departments): EPA CAMEO data manager imports .t2s, and E-Plan gives responders free access. I found no sourced unmet need.
