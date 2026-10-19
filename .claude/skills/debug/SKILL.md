---
name: debug
description: Systematic debugging workflow (reproduce, probe, examine, fix) for StitchFoundry. Use when a design sews or renders wrong, an import or export misbehaves, a test or `cargo xtask ci` fails for an unclear reason, a change "should have helped" and didn't, or any bug resists the obvious approaches. Based on Kuhlmann, The Book of Debugging (No Starch Press); chapter numbers refer to it.
---

# Debugging workflow

Use this when something fails and the cause is not obvious. The core claim: the computer is
deterministic, so every bug has a cause that can be found. A workflow keeps effort from scattering.
Loop through the four steps until the bug cannot be reproduced; always start with Reproduce.

Based on Johannes Kuhlmann, *The Book of Debugging* (No Starch Press); chapter numbers refer to it.
The owner's detailed notes on the book are in bwattendorf/autolayout, branch `m7-escape`,
`docs/debug-book-notes.md` (paraphrased working notes; not copied here).

## 0. Before anything else

- Take responsibility: assume the bug is in our code, not in the OS, a dependency or "the file"
  (Intro).
- Open a debugging log for the bug (template below) and write the symptom, what you expected and
  the exact command that shows it. Symptoms only; keep theories out of that line (Part I).
- One bug at a time. Finish or park it before starting another (ch. 18).

## 1. Reproduce (ch. 1-3)

- Get the failure to happen on demand, from a named input and a command. A result nobody can
  re-run is not a result.
- **Shorten the loop first.** If one run takes minutes, make a faster path before investigating
  (see "This project" below). Fast loops are worth more than clever theories.
- **Cut the case down** (ch. 20, minimal repro): the smallest input that still fails. If it stops
  failing, the part you removed matters; put it back in halves.
- Vary one thing at a time to learn which steps matter: order, timing, input, settings, platform.
- Automate the repeat. Keep the repro as a test once the bug is fixed (ch. 3, 25).

## 2. Probe: find an entry point (ch. 4-9)

- Make an educated guess, but cap the time: stop after about an hour, or after two or three
  shortcuts that did not pan out, or when the guesses stop being based on evidence (ch. 4).
  Switch strategy then.
- Rule systems out as well as in. Check code that has broken before, and the glue between systems,
  before old trusted code or libraries (ch. 4).
- **Bisect when it used to work** (ch. 6): a good commit and a bad one, `git bisect` between them,
  then read the offending diff for the one change that matters.
- **Instrument before theorising** (ch. 8): assertions as early as possible, check every return
  value, make bad data look obviously bad. Keep instrumentation optional (env switches, flags), off
  by default, and check it doesn't change the behaviour you are chasing.
- **Remove randomness** (ch. 9): fixed seeds, fixed order, no clock. A "random" result has a cause.
- Look at the data the program actually produced (dumps, images, logs) before reasoning about it
  (ch. 15).

## 3. Examine: find the root cause (ch. 13-19)

- **Follow the data** (ch. 15): find the original input, follow it through each transformation,
  find where it first becomes bad, compare good and bad cases side by side.
- **Question everything** (ch. 16): list your assumptions and verify each one. Don't trust a
  feature because it exists; check that it actually fires (count how often a branch ran). A symptom
  can sit far from its cause.
- **Experiment with a hypothesis** (ch. 17). For each one write: the hypothesis, how it will be
  tested, what will be learned either way. Test with the least effort; try the hackiest fix first
  to confirm the place, then do the clean one. Keep experiments revertible (`git diff` shows them).
- **Don't drop a failed experiment as "no good" until it's re-tested on a baseline known to be
  correct.** If a baseline bug turns up later, re-run the reverted experiments.
- **Keep a debugging log** (ch. 18): every idea, every result, what's ruled out, a prioritised list
  of next leads so there's always a plan. Reference it from the commit.
- **Build tools** (ch. 19): when you keep wanting to see something, make a flag or a command for it
  and document it. A hidden tool doesn't help.
- Talk it through (ch. 23) when stuck: stating the problem (in the log, or to the user) often
  exposes the wrong assumption.

## 4. Fix (ch. 23-30)

- A great fix is minimal, complete, revertible, and addresses the root cause, not the symptom. One
  bug per change (ch. 23, 24).
- **Verify** (ch. 23): reproduce several times; undo the change and confirm the bug returns;
  re-apply and confirm it goes. Test the fix alone, without other local edits.
- Check side effects separately from the fix: run the wider test suite and compare (ch. 24).
- **Share what you learned** (ch. 25): commit message with symptom, root cause and fix; ask how the
  cause got in and what safeguard prevents it (an assertion, a test, a CI check). Turn the repro
  into a regression test.
- **Search for related problems** (ch. 26): the same mistake may exist elsewhere. Fix the family
  while the context is fresh; make bulk changes with care.
- If a clean fix isn't possible yet: a workaround, said so plainly (ch. 28), a rewrite of the
  offending piece compared against the old one (ch. 29), or a changed requirement (ch. 30). Don't
  stop at the workaround; record what remains.

## This project (StitchFoundry)

- **Fast loops, not the full CI.** `cargo xtask ci` takes about an hour: it's the gate before a
  commit, never the debugging loop. Run the one failing test (`cargo test -p <crate> --lib <name>`,
  or `--test <file> <name>`), with `CARGO_INCREMENTAL=0` and a separate `CARGO_TARGET_DIR` when
  other agents share the machine. Long runs go in the background.
- **Run a command on its own:** `cargo run -q -p vectorcraft-cli -- run --cmd <id> --params '<json>'
  --cmd … --export out.png`. Note: `--export` draws the **art**; to see the **stitches**, chain
  `--cmd embroidery.exportImage --params '{"path":"stitches.png"}'` and look at that PNG.
- **Look at what it did:** `embroidery.info` and `embroidery.stats` (sizes, stitch and colour
  counts), the Stitch List, `document.commands` (params docs), `ui.screenshot` through the control
  channel (`--control 7979`), `RUST_LOG=debug`.
- **Keep the repro:** as a test beside the code (`crates/*/src/tests*.rs`, `crates/*/tests/`), and
  extend the fuzz and sweep suites (`engine/tests/import_fuzz.rs`, `command_sweep.rs`) when the bug
  came from untrusted input.
- **Instrumentation obeys the no-panic rule:** checks return errors or log, never `unwrap`/`panic!`
  in shipped code.
- Debugging logs go in `docs/debug-logs/` (link them from the commit message).

## Anti-patterns (avoid)

- Tuning parameters one at a time hoping a number moves (desperate guesses, ch. 4).
- Dropping an idea on one bad run, and trusting a baseline that had a latent bug.
- Reading a count as proof without looking at why.
- Running a ten-minute experiment when a forty-second one answers the same question.
- Claiming a feature works because it compiled; measure that it did what you think.

## Debugging log template

Create `docs/debug-logs/YYYY-MM-DD-<short-topic>.md`:

```
# <topic>  (input, command, commit)
Symptom (objective, no theory):
Expected:
Repro command + input:   (fast version: ...)
Frequency / measured numbers:

## Leads (prioritised: effort vs chance)
1. ...

## Experiments
- [date] Hypothesis / test / result / what it rules in or out (file paths, numbers)

## Ruled out
- ...

## Root cause (when found) / fix / verification / related places checked
```
