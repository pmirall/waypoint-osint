# Phase 1: end-to-end GPS benchmark (small diagnostic)

Workflow: \`.github/workflows/phase1-benchmark.yml\`.

This compares the *exact* parent commit \`502318a7...\` with the phase-1 PR branch.
It uses four external OSV-5M-test street images from the public CC BY-SA 4.0
[OSM-Europe-1k](https://huggingface.co/datasets/lebfla11/osm-europe-1k)
dataset (images \`00054.jpg\`, \`00060.jpg\`, \`00059.jpg\`, \`00055.jpg\`).

It downloads the publicly hosted OSV-5M PLONK + StreetCLIP + DISK/LightGlue
ONNX weights, checks upstream SHA-256 hashes, and uses the official Linux
ONNX Runtime CPU release. The baseline and phase1 branches are run with
identical parameters except for the intended 1-vs-3 retrieval-cluster setting.

Parameters: 256 PLONK samples, 1 sampling run, no solar/geocoder,
image budget 90 per retrieval source, 5 verification candidates/source,
CPU fallback, and no Mapillary API token.

It reports: coarse top-cluster error, chosen-prediction error,
nearest-retrieved-candidate (oracle) error, at 10m, 100m, 1km, 25km.
It records whether an individual run failed, rather than ignoring failures.

**Important**: Four images are a *smoke benchmark*, NOT a statistically
meaningful claim about accuracy. Unofficial Google Street View endpoints may
fail due to provider restrictions, and Mapillary lookup is skipped without an
API token. A target might have no nearby imagery in the remaining sources.
The final "chosen match" rule is an evaluation heuristic, not a calibrated
production predictor. Approximate GPS labels can themselves exceed 10m.

Licensing: OSM-Europe-1k uses Mapillary/OSV-5M images under CC BY-SA 4.0.
The workflow downloads but does not redistribute or publish image bytes.
Do not upload model files to the public repository (StreetCLIP is CC BY-NC 4.0).

Check Actions' "Benchmark phase 1 vs upstream" workflow for all raw logs,
downloadable reports, failures and resource limitations.

Baseline and phase1 builds receive the **same fixed PLONK seed** via a local
benchmark-only patch; this patch is not incorporated into the app. The ORT
archive is downloaded from a third-party mirror but verified against the
upstream-pinned Microsoft release SHA-256 before extraction. Previous failed
runs (e.g. download errors) must not be counted as 0% accuracy.

## Diagnostic performance profile (2026-11-23)
The first full end-to-end trial timed out on 2 consecutive baseline images
at 420s each, before reaching phase1. No accuracy result can be inferred.
The runner is now deliberately **budget-limited** (96 PLONK samples, radius
0.6km, 24 reference images per source, 3 verification candidates, max 180s
per image) and interleaves baseline/phase1 per image. Results must NOT be
presented as the expected full-quality production accuracy.
Timeouts preserve NDJSON/stderr and last-stage diagnostics. Accuracy is shown
ONLY for complete A/B pairs; no successful pairs means a failed benchmark,
not a claim of 0% accuracy. A later full-budget benchmark is still required.
