# AI test jobs (AI-EVAL-01)

Design from Prompt's test jobs: the same requests run again after a change to the prompt, the
model or the effort, so the results can be compared side by side with photos of the real thing.

- `jobs/*.json`: one job each: `{id, title, prompt, sizeMm, colors, hoop: {width, height},
  reference, lookFor}`. `lookFor` lists what a good result shows; the report turns it into a
  checklist to tick while comparing.
- `references/`: the owner's own photos the jobs are compared with (`cashew.jpg`, `truck.jpg`,
  `rv.jpg`, `truck-and-rv.jpg`). They are personal and stay on this computer: the folder is
  ignored by git. A job without its photo still runs.

Run them. It spends money, either in credits on your StitchFoundry account (sign in in the app
first, under Embroidery › Foundry Account; the hosted service holds the Anthropic key, as when
the app pays with credits) or, with `ANTHROPIC_API_KEY` set, on that Anthropic account. The
estimate is shown first, in credits with what you have left, and nothing is sent until you type
`yes`; if the account is turned away (signed out, out of credits) it stops at once:

```sh
stitchfoundry-cli ai-eval --dry-run              # the plan and its estimated cost only
stitchfoundry-cli ai-eval --effort all           # every job at Quick, Normal and Thorough
stitchfoundry-cli ai-eval --only havanese-smiling --effort thorough
```

The report is `target/ai-eval/<time>/report.html`: each drawing's stitches beside the reference
photo, with its tokens, cost, time, stitches and colour changes. `results.json` holds the same
numbers; its `byEffort` means are what `sf_core::ai::Effort::typical_output` should be set from.
