# Waypoint OSINT — phase 1: retrieval coverage and visual viewpoints

This is an experimental branch, not a published accuracy improvement.

## Source baseline
Forked from KillaMeep/waypoint-osint, main commit `502318a7da42bdc38af2ad2f87152492dbf9d671`.

## Implemented
- Retrieve across 3 initial PLONK clusters instead of only the strongest.
- Compute high-density search seeds within broad PLONK clusters and distribute image budgets among seeds.
- Prefer spatially diverse road probe points and imagery instead of arbitrarily truncating results.
- Decrease Street View road probe spacing from 60m to 30m and increase probe limit to 800.
- Increase Mapillary per-tile candidate count from 5 to 25.
- Follow bounded, same-host STAC GET pagination for Panoramax.
- Render rectilinear Street View images: coarse 6-heading pass; fine 8-heading matching for strong panoramas.
- Add unit tests for deterministic spatial selection and perspective projection.

## Limitations and review notes
- No controlled geolocation benchmark has run. No claim about 10m accuracy is justified.
- Additional clusters, probes, and perspective crops add considerable API, CPU and memory load.
- This work relies on the upstream Google Street View implementation, which uses undocumented interfaces. Confirm licensing and use approved APIs before deploying at scale.
- The merge-ranking heuristic and 3-cluster defaults are experimental and require calibration against a held-out dataset.
- The Street View candidate coordinate is the reference camera position, not necessarily the target camera position.
- Model parity tests may diverge where defaults/output cluster metadata intentionally change.

## Validation checklist
- [ ] `cargo test --release --locked --manifest-path waypoint-core/Cargo.toml`
- [ ] `cargo check --locked --manifest-path waypoint-core/Cargo.toml`
- [ ] Manual application smoke test on Street View and non-Street View images
- [ ] Upstream-vs-branch candidate recall @5km, retrieval Recall@20, and accuracy @100m/10m
- [ ] Record median runtime, memory, and network requests per image

The branch should remain unmerged until the checks and benchmark are complete.
