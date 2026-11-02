# Progress

## Nightly log

- 2026-10-31, issue 22 (part A, app side): `help.bugReport.build` / `.queue` / `.pending` in `crates/engine/src/cmd/bugreport.rs`. Scrubs emails, phones and masked names, caps sizes, marks reporter text untrusted, queues offline reports as files.
- Still to do for issue 22: the report form UI, the StitchHub endpoint (token, rate limiting, spam filter, reply channel) and the labels (needs-triage, triaged, cant-reproduce, needs-info, reproduced, fix-proposed, security). These live outside this repo or need a UI task.
- 2026-11-02 nightly sweep: no reported bugs to triage; no vulnerable dependencies (6 unmaintained notices); filed #39 (XXX reader 32-bit overflow) and #40 (hand-rolled date math).
- Fix #32: the bug-report scrub keeps ISO dates, numbers followed by a unit and space-grouped thousands instead of masking them as [phone].
