# Progress

## Nightly log

- 2026-11-01 nightly sweep: no reports to triage; main CI green through tests (later steps stopped on a full disk); cargo audit has no vulnerabilities. Filed #31 and #32 (bug-report scrubber).
- 2026-11-01 fix for #31: names with letters like `İ` are now hidden by `help.bugReport.build`.
- 2026-10-31, issue 22 (part A, app side): `help.bugReport.build` / `.queue` / `.pending` in `crates/engine/src/cmd/bugreport.rs`. Scrubs emails, phones and masked names, caps sizes, marks reporter text untrusted, queues offline reports as files.
- Still to do for issue 22: the report form UI, the StitchHub endpoint (token, rate limiting, spam filter, reply channel) and the labels (needs-triage, triaged, cant-reproduce, needs-info, reproduced, fix-proposed, security). These live outside this repo or need a UI task.
