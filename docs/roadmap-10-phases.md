# Ten-phase improvement roadmap

Phases are milestones for separate development sessions, not asynchronous work.

1. Multi-cluster search, adaptive seeds, retrieval coverage and perspective views (**in progress: implementation on feature branch; tests pending**)
2. Place-recognition retrieval and stronger reranking (AnyLoc-like alternatives)
3. Geometric matching and false-positive rejection with calibrated evidence
4. Estimate target-camera position from multiple georeferenced views (SfM/PnP)
5. EXIF orientation, GPS provenance, OCR and OpenStreetMap semantic queries
6. Ensemble and scene-adaptive routing of PLONK variants
7. Solar/shadow and terrain cues as evidence filters
8. Fused confidence estimates, abstentions, and uncertainty bounds
9. Compliant source APIs, cache/index design, cost and latency controls
10. Reproducible benchmark and release readiness

Acceptance for every phase: its code builds, targeted tests pass, and a benchmark supports any claimed gain.
