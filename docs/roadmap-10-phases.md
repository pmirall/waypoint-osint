# Waypoint — ten-phase accuracy roadmap

This document is the permanent implementation roadmap. **Never silently delete
or renumber phases.** Each phase requires tests and measured before/after
results; compilation alone does not demonstrate improved accuracy.

| # | Phase | Deliverable | Status (2026-11-23) |
|---|---|---|---|
| 1 | Multi-cluster, adaptive search, street-image coverage, panorama views | Retrieve more relevant reference imagery and avoid missing correct regions | Code on \`feat/phase1-retrieval-coverage\`; builds, benchmark unverified |
| 2 | Place recognition and candidate re-ranking | Compare StreetCLIP with visual-place models and run verified top-K retrieval | Planned |
| 3 | Stronger geometric consistency | Better RANSAC, homography/essential matrix diagnostics, calibrated false-positive rejection | Planned |
| 4 | Target camera localization in 3D | Multi-view SfM/PnP and georeferenced pose uncertainty, not just camera-of-reference coordinate | Planned |
| 5 | EXIF + OCR + OSM context | Correct orientation, extract text, query geotagged structured clues | Planned |
| 6 | Ensemble of PLONK variants | Combine OSV5M/YFCC/iNaturalist using scene routing and geographic probability | Planned |
| 7 | Solar/shadow/terrain plausibility | Better sun direction and terrain matching as conditional evidence | Planned |
| 8 | Confidence and abstention | Reliability calibration, uncertainty bounds, refusal to assert false 10 m precision | Planned |
| 9 | Compliant imagery retrieval / indexes / cost | Provider-safe APIs, cache reference imagery, spatial indexing, budget/latency controls | Planned |
| 10 | Independent evaluation & release | Large held-out ground truth, regression gates, packaged Windows release | Planned |

## Cross-cutting: local benchmark lab

The desktop Benchmarks feature is **supporting infrastructure**, not a
replacement for phase 10. It uses local models and a public geotagged dataset.
The initial release allows one-click benchmark launch on the user's computer,
but exact A/B of an upstream binary versus phase 1, strict hard timeouts,
reference-image snapshotting and strong 10 m GPS ground truth are pending.

## Per-phase gates

- A: Code compiles on the actual desktop target; focused tests and smoke pass.
- B: Every benchmark source has acceptable usage and licensing.
- C: Paired original-versus-changed results on the same fixed images, seeds,
  image references and resource budget.
- D: Report accuracy@10 m, @100 m, @1 km, @25 km, P50/P90, false positives,
  coverage, runtime and network requests. Do not discard unsuccessful cases.
- E: Require confidence intervals and dedicated held-out test split before
  claiming a generalizable accuracy improvement.

## No-cloud testing policy

GitHub Actions runs cost minutes. No workflow should auto-start because of a
branch push or PR. Only manual \`workflow_dispatch\` may be retained; never
trigger it without permission. Local CPU/GPU runs are the default.
