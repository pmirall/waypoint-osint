# Development continuity — Waypoint OSINT

Read **docs/PROJECT_STATE.md** and **docs/roadmap-10-phases.md** before changing the
geolocation engine. These files preserve the decisions made over multiple sessions.

## Hard project rules

1. **NO GitHub Actions runs or workflow retries without explicit user approval.**
   Prefer local verification on the user's PC. Do not enable automatic triggers.
2. Preserve the 10-phase roadmap; mark delivered, validated and unvalidated
   states separately. Do not conflate compilation with improved geolocation accuracy.
3. Work in isolated feature branches. Do not merge to \`main\` automatically.
4. The current benchmark uses local ONNX models. Cache dataset photos and
   completed predictions. Report timeouts and retrieval failures separately.
5. Do not claim 10 m localization accuracy from PLONK cluster centers or
   georeferenced reference-camera coordinates.
6. Do not publish users' photos, private locations, tokens or test imagery in
   public GitHub reports. Review licenses and imagery provider terms.
7. When changing accuracy, state target dataset, protocol, sample count,
   seeds, model versions, and per-case errors. Prefer paired comparisons.
8. If the connected environment cannot compile Tauri/Rust, document that
   limitation and do not substitute speculative assurances.

The user wants a one-click local desktop workflow. Avoid cloud/CI expenditures.
