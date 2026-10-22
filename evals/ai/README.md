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

## Other picture services

`--pictures` (the workflow's **pictures** box) also has other image models paint each job, and
Claude redraw the picture as stitches, as Design from Prompt does when another service is chosen:
`google`, `openai`, `fal`, `grok`, or `openrouter:MODEL` for any image model OpenRouter offers
(its model id from openrouter.ai/models). Each needs its key: `GOOGLE_API_KEY`, `OPENAI_API_KEY`,
`FAL_API_KEY`, `GROK_API_KEY` or `OPENROUTER_API_KEY` (in the workflow, repository secrets of those
names), billed to that service. The report shows each picture above its stitches. Use one effort
(`normal`) when comparing pictures: each service adds a drawing per job and effort.

## Results

**2026-10-22**, all four jobs at each effort, on GitHub Actions (`ai-eval.yml`), Claude Opus 5.5:

| Effort | Drawings | Mean output tokens | Cost | Time each |
|---|---|---|---|---|
| Quick | 4 of 4 | 977 | $0.09 in all | 10–12 s |
| Normal | 3 of 4 (the RV's reply had no artwork) | 2,818 | $0.18 in all | 19–37 s |
| Thorough | 3 of 4 (the truck timed out at 5 minutes) | 17,899 | $4.97 in all | 67–324 s |

- Thorough drew the best of every job: the truck towing the fifth wheel and the fifth wheel alone
  were recognisable as what was asked (crew cab, landing legs, slide-out, door, two axles); the
  dog came out with the right brown eyes and nose. Quick and Normal drew the truck alone as a boxy
  SUV, and gave the dog tan ears and yellow eyes.
- Thorough's cost came almost all from one drawing: the fifth wheel alone read 927,711 input
  tokens ($4.30). It searched with `web_search_20260209`, which filters its results by running
  code, in up to ten rounds a request, each reading everything again; then the check sent all of
  it back, with the search tool. Fixed the same day: the basic `web_search_20250305`, at most two
  searches, and a check that is a request of its own with no tools (`sf_core::ai`). The hosted
  service refuses the looping tools and holds for every round (`docs/cloud.md`).
- `Effort::typical_output` was set from these means (Thorough's from its drawings without the
  outlier). The next run should measure Thorough again with the fix.
