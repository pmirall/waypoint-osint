# Waypoint — canonical project state

Updated: **2026-11-23**. This document is the cross-session handoff for the
10-phase geolocation improvement project. It is authoritative over chat summaries.

## Repositories and branches

- Upstream: https://github.com/KillaMeep/waypoint-osint
- Public fork: https://github.com/pmirall/waypoint-osint
- Phase 1 experimental branch: \`feat/phase1-retrieval-coverage\`
- Phase 1 draft PR: https://github.com/pmirall/waypoint-osint/pull/1
- Integrated desktop Benchmarks branch: \`feat/local-benchmark-lab\` (based on phase1).
- Upstream reference commit: \`502318a7da42bdc38af2ad2f87152492dbf9d671\`.

## Original goal

Given a generic photograph, estimate camera coordinates as precisely as possible,
with **≤10 m** as a long-term target. It cannot be guaranteed for all photos.
Location of a visually matching street-camera image is NOT target-camera pose.

## Current technical state

- Phase 1 changes: multi-cluster retrieval (3), spatially diverse candidates,
  Mapillary/Panoramax coverage, rectilinear panorama projections.
- Phase 1 compiled and passed core tests in prior CI; **no controlled accuracy
  improvement has been measured**. The 4-photo online benchmark timed out on
  the baseline when running on GitHub Actions; those values are NOT accuracy.
- Benchmarks local v1: embedded Tauri UI, 4/50/500 real geotagged photos,
  coarse/precision profiles, local ONNX inference, image/result cache, progress,
  soft per-photo timeout, cancellation, locally saved JSON, CSV and HTML reports.
- Local build helper: BUILD-AND-RUN-WINDOWS.cmd. No packaged Windows installer verified.
- Fork identity is Waypoint Lab and its updater endpoint points at this fork;
  startup auto-update checks disabled to prevent installing upstream releases.
- Benchmarks local branch has NOT been compiled or exercised as a packaged
  desktop app as of this handoff. Do not call it released.
- Exact upstream-vs-phase1 A/B in the embedded app remains a future feature:
  separate versioned executables or repeatable benchmarks from two builds are
  needed. Do NOT label an approximate switch as exact upstream.

## Resource and safety decisions

- **Zero GitHub Actions by default.** User explicitly stopped runs after CPU
  baseline cases exceeded 420s. Never retry or dispatch workflows unasked.
- Benchmarks MUST run on the user's PC with already installed ONNX models;
  exploit GPU when possible; cache photos and successful predictions.
- Downloads are limited; source photographs stay on the PC. The public test
  manifest is OSM-Europe-1k from Hugging Face, attribution/license required.
- Cooperatively cancelled Rust inference may finish the current network call;
  timeout is **soft** until truly killable worker isolation is implemented.
- Do not redistribute third-party commercial or noncommercial model weights.

## Immediate next tasks

1. Compile/test integrated Benchmarks on a Windows dev PC with Rust, Node and
   Tauri prerequisites. This sandbox cannot compile Rust.
2. Run 4-image local smoke in coarse profile and verify dataset columns/paths.
3. Test cancellation, crash resume, GPU auto selection, report persistence and
   result correctness.
4. Run 50-photo pilot on PC with strict resource constraints.
5. Add exact A/B baseline comparison and version-pinned reference imagery.
6. Continue phase 2 only after phase 1 metrics and regression checks.

## Truth criteria

A valid accuracy claim must report sample count, genuine reference GPS quality,
model setup, P50/P90 error, accuracy@10m/100m/1km/25km, success/abstention
coverage, elapsed time, and paired statistical uncertainty.

Preserve this state file, update it with each phase, and follow
\`docs/roadmap-10-phases.md\`. No execution is asynchronous from a chat.
