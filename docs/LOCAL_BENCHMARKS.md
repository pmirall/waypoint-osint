# One-click Benchmarks inside Waypoint — first implementation

**Branch:** \`feat/local-benchmark-lab\` (from Phase 1).

## User experience

Once the local Windows Waypoint app is built and installed, open Waypoint,
choose **Benchmarks**, select **4 / 50 / 500 images**, and press
**Run benchmark locally**. Only the user's PC computes predictions.
The setup wizard already installs ONNX Runtime and PLONK / DISK / LightGlue;
the new tab reuses those models (and uses GPU via the existing backend).

Two modes:

- **Fast / coarse:** 96 PLONK samples per photograph, no Street View/Mapillary/
  Panoramax. Produces regional location errors, NOT meaningful metric camera pose.
- **Street-level:** 192 samples, one cluster, up to 18 reference images per
  source, 3 geometric verifications, 1.2 km search radius, 60–600s per-photo
  cooperative deadline. This is a resource-limited diagnostic profile, not
  the full-quality production preset.

The dataset is [OSM-Europe-1k](https://huggingface.co/datasets/lebfla11/osm-europe-1k).
It requires network for first-time metadata/images; images and completed
predictions are cached in \`<Waypoint app data>/benchmarks/\`, alongside
checkpointed JSON reports. Repeated runs can reuse local files. Mapillary
lookup is disabled in the benchmark to avoid exact-image test leakage.
The reported coordinate may be a reference location rather than target-camera
pose. No claim of 10 m accuracy has yet been validated.

## Status and limitations

- **Source integrated; desktop package untested in the current environment.**
  Rust and Windows Tauri Build Tools are not installed in this container.
- Timeout is cooperative: external provider calls may not abort instantly.
  The Stop button requests cancellation and reports completed cases.
- Reference panoramas are still fetched live; a future phase must snapshot
  provider-compliant imagery to make A/B more repeatable.
- Exact A/B against upstream original is not implemented in this first UI
  integration; do not call repeat runs of the same engine a baseline comparison.
- Runs of 500 images can take many hours and should be chosen deliberately.
- Public dataset metadata may have GPS label error greater than 10 m.

## Development without GitHub Actions

On a Windows dev PC that has the official Waypoint prerequisites:
Node.js 20+, Rust 1.90+, Visual Studio Build Tools / WebView2, clone the feature
branch and use \`npm ci\` then \`npm start\` to run the desktop UI.
No remote workflow is needed. Installing a signed distributable is a separate
release step after local compile and QA.

## Persistence

Canonical cross-session state: \`docs/PROJECT_STATE.md\`.
Phases 1 through 10: \`docs/roadmap-10-phases.md\`.
Attributions and provider terms must be rechecked before distributing
benchmark images or model files.
