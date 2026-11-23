#!/usr/bin/env python3
"""Small reproducible *end-to-end* image -> GPS experiment for Waypoint.

Uses OSM-Europe-1k public geotagged images (OSV5M test split, CC BY-SA 4.0).
Results for only a few photos are a smoke/diagnostic benchmark, not population
accuracy estimates. Exact-image matches are excluded at evaluation time.
"""
import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.request

DATASET = "https://huggingface.co/datasets/lebfla11/osm-europe-1k/resolve/main"
MODELS = "https://huggingface.co/killameep/waypoint-models/resolve/main"
ORT = ("https://huggingface.co/csukuangfj2/onnxruntime-libs/resolve/main/1.24.4/"
       "onnxruntime-linux-x64-1.24.4.tgz?download=true")
SAMPLES = ("00054.jpg", "00060.jpg", "00059.jpg", "00055.jpg")


def download(url, dest, sha=None):
    dest = Path(dest)
    if dest.is_file() and (not sha or sha256(dest) == sha):
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    err = None
    for attempt in range(3):
        try:
            with urllib.request.urlopen(urllib.request.Request(
                    url, headers={"User-Agent": "waypoint-accuracy-benchmark/1.0"}),
                    timeout=100) as inp, open(str(dest) + ".part", "wb") as out:
                shutil.copyfileobj(inp, out, 1024 * 1024)
            if sha and sha256(str(dest) + ".part") != sha:
                raise ValueError("SHA-256 mismatch for " + dest.name)
            os.replace(str(dest) + ".part", dest)
            return
        except Exception as e:
            err = e
            print(f"download retry {attempt+1} for {dest.name}: {e}", file=sys.stderr, flush=True)
            time.sleep(2 * (attempt + 1))
    raise RuntimeError(f"Download failed: {url}: {err}")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for c in iter(lambda: f.read(1024 * 1024), b""):
            h.update(c)
    return h.hexdigest()


def setup_models(root):
    root = Path(root)
    manifest = json.loads(Path("src-tauri/models.json").read_text())
    include = {"streetclip_vision.onnx", "disk_unet.onnx", "plonk_osv5m_step.onnx"}
    include.update(x["name"] for x in manifest["files"] if x["name"].startswith("lg_"))
    for item in manifest["files"]:
        if item["name"] in include:
            print("fetching", item["name"], f"({item['bytes']/1e6:.0f} MB)", flush=True)
            download(MODELS + "/" + item["name"] + "?download=true",
                     root / "onnx" / item["name"], item["sha256"])
    assert all((root/"onnx"/name).exists() for name in include)
    tgz = root / "ort.tgz"
    download(ORT, tgz, sha="3a211fbea252c1e66290658f1b735b772056149f28321e71c308942cdb54b747")
    dll = root / "ort" / "libonnxruntime.so"
    dll.parent.mkdir(exist_ok=True)
    with tarfile.open(tgz, "r:gz") as t:
        found = False
        for member in t:
            name = member.name.rsplit("/", 1)[-1]
            if not member.isfile() or "/lib/" not in member.name:
                continue
            if name.startswith("libonnxruntime.so."):
                with t.extractfile(member) as src, open(dll, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                found = True
            elif name == "libonnxruntime_providers_shared.so":
                with t.extractfile(member) as src, open(dll.parent / name, "wb") as dst:
                    shutil.copyfileobj(src, dst)
        if not found:
            raise RuntimeError("Official ONNX Runtime tarball did not contain runtime .so")


def setup_images(root):
    root = Path(root)
    manifest = root / "metadata.csv"
    download(DATASET + "/metadata.csv", manifest)
    source = {r["filename"]: r for r in csv.DictReader(open(manifest, newline=""))}
    labels = []
    for name in SAMPLES:
        if name not in source:
            raise RuntimeError(f"{name} not found in dataset")
        r = source[name]
        download(DATASET + "/images/" + name, root / "images" / name)
        labels.append({"id": name, "gt_lat": float(r["lat"]),
                       "gt_lon": float(r["lon"]), "country": r.get("country"),
                       "exclude_mapillary_id": str(r.get("osv5m_id", ""))})
    (root / "samples.json").write_text(json.dumps(labels, indent=2))
    print("Dataset:", [(r["id"], r["country"], r["gt_lat"], r["gt_lon"]) for r in labels])


def haversine(a,b,c,d):
    lat1, lat2 = math.radians(a), math.radians(c)
    x = math.sin((lat2-lat1)/2)**2 + math.cos(lat1)*math.cos(lat2)*math.sin(math.radians(d-b)/2)**2
    return 2*6371000*math.asin(math.sqrt(max(0, min(1,x))))


def best_pred(result, image_id):
    clusters = result.get("clusters") or []
    if not clusters:
        return None, [], "no_clusters"
    found = []
    for c in clusters:
        for k in ("google_sv_matches", "mapillary_matches", "panoramax_matches"):
            for m in c.get(k, []):
                mid = m.get("mapillary_url", "").split("pKey=")[-1].split("&")[0]
                if mid == image_id: # never use exact target image as its own reference
                    continue
                found.append((k, m))
    if not found:
        return (float(clusters[0]["lat"]), float(clusters[0]["lon"])), [], "coarse_fallback"
    def score(rec):
        m = rec[1]
        n = m.get("inliers", 0)
        tot = m.get("total_matches", 0)
        if n >= 12 and tot >= 12:
            return (2, n/max(tot,1), math.log1p(n))
        return (1, 0, float(m.get("similarity",0)))
    found.sort(key=score, reverse=True)
    m = found[0][1]
    return (float(m["lat"]), float(m["lon"])), found, found[0][0]


def evaluate(output):
    output = Path(output)
    rows = json.loads((output / "samples.json").read_text())
    details = []
    for gt in rows:
        for mode in ("baseline", "phase1"):
            path = output / f"{mode}_{gt['id']}.ndjson"
            meta_path = output / f"{mode}_{gt['id']}.status.json"
            meta = json.loads(meta_path.read_text()) if meta_path.exists() else {}
            records = []
            if path.exists():
                for line in path.read_text(errors="replace").splitlines():
                    try:
                        records.append(json.loads(line))
                    except json.JSONDecodeError:
                        pass
            done = next((r for r in reversed(records) if r.get("event") == "done"), None)
            errors = [r.get("message") for r in records if r.get("event") == "error"]
            row = dict(gt, variant=mode, run_status=meta.get("status", "missing"),
                       duration_s=meta.get("duration_s"),
                       last_stage=meta.get("last_stage", ""),
                       error=";".join(str(e) for e in errors))
            if done and not errors and meta.get("status") == "complete":
                result = done.get("result", {})
                clusters = result.get("clusters") or []
                row["n_clusters"] = len(clusters)
                row["coarse_error_m"] = haversine(gt["gt_lat"], gt["gt_lon"],
                    clusters[0]["lat"], clusters[0]["lon"]) if clusters else None
                coord, matches, src = best_pred(result, gt["exclude_mapillary_id"])
                row["chosen_source"] = src
                row["n_matching_candidates"] = len(matches)
                row["best_error_m"] = haversine(gt["gt_lat"], gt["gt_lon"], *coord) if coord else None
                row["oracle_retrieval_error_m"] = min((haversine(
                    gt["gt_lat"], gt["gt_lon"], m["lat"], m["lon"]) for _, m in matches), default=None)
            else:
                row.update(n_clusters=0, coarse_error_m=None, chosen_source="unavailable",
                           n_matching_candidates=0, best_error_m=None,
                           oracle_retrieval_error_m=None)
            details.append(row)

    # Accuracy is defined over *paired completed runs* only. Otherwise report
    # runtime failure, not an invented 0% accuracy.
    paired = {r["id"] for r in details if r["variant"] == "baseline"
              and r["run_status"] == "complete" and any(
                  x["id"] == r["id"] and x["variant"] == "phase1"
                  and x["run_status"] == "complete" for x in details)}
    print(f"\nPAIRED COMPLETE CASES: {len(paired)}/{len(rows)}", flush=True)
    for mode in ("baseline", "phase1"):
        group = [r for r in details if r["variant"] == mode]
        ready = [r for r in group if r["id"] in paired]
        print(f"{mode}: completed={sum(r['run_status']=='complete' for r in group)}/{len(group)}, "
              f"timed_out={sum(r['run_status']=='timeout' for r in group)}, "
              f"paired={len(ready)}", flush=True)
        for key in ("coarse_error_m", "best_error_m", "oracle_retrieval_error_m"):
            vals = [r[key] for r in ready if r[key] is not None]
            rates = {f"accuracy@{t}m": sum(v <= t for v in vals)/len(ready) if ready else None
                     for t in (10,100,1000,25000)}
            print("  ", key, "median_m=", sorted(vals)[len(vals)//2] if vals else None,
                  "rates=", rates, flush=True)

    with open(output / "details.csv", "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=details[0].keys())
        writer.writeheader()
        writer.writerows(details)
    (output / "summary.json").write_text(json.dumps({
        "n_images": len(rows), "n_paired_complete": len(paired),
        "benchmark_valid": bool(paired), "details": details
    }, indent=2))
    print("Saved", output/"details.csv", output/"summary.json", flush=True)
    return bool(paired)


def run_tests(root, baseline_binary, phase_binary):
    root = Path(root)
    images = json.loads((root / "samples.json").read_text())
    timeout = int(os.environ.get("WAYPOINT_BENCH_TIMEOUT_SECONDS", "180"))
    # Interleave A/B so timeouts cannot prevent any phase-1 image being tested.
    for sample in images:
        for mode, binary in (("baseline", baseline_binary), ("phase1", phase_binary)):
            image = root / "images" / sample["id"]
            cmd = [str(Path(binary).resolve()), "one", "--image_path", str(image.resolve()),
                   "--model", "osv5m", "--num_samples", "96", "--num_runs", "1",
                   "--no_sun_refine", "--no_geocode", "--radius_km", "0.6",
                   "--max_images", "24", "--verify_top_n", "3", "--top_k", "5",
                   "--retrieval_top_clusters", ("1" if mode == "baseline" else "3")]
            env = dict(os.environ, WAYPOINT_ACCEL="cpu", WAYPOINT_BENCH_SEED="20261123",
                       WAYPOINT_ORT_DLL=str((root / "ort" / "libonnxruntime.so").resolve()),
                       WAYPOINT_ONNX_DIR=str((root / "onnx").resolve()))
            print(f"RUN {mode} {sample['id']} at {time.strftime('%H:%M:%S')}", flush=True)
            start = time.monotonic()
            stdout_path = root / f"{mode}_{sample['id']}.ndjson"
            stderr_path = root / f"{mode}_{sample['id']}.stderr"
            status_path = root / f"{mode}_{sample['id']}.status.json"
            outcome = "failed"
            retcode = None
            # Writing straight to files preserves progress even on timeout.
            with open(stdout_path, "w") as stdout, open(stderr_path, "w") as stderr:
                try:
                    proc = subprocess.run(cmd, env=env, stdout=stdout,
                                          stderr=stderr, timeout=timeout)
                    retcode = proc.returncode
                    outcome = "complete" if retcode == 0 else "failed"
                except subprocess.TimeoutExpired:
                    outcome = "timeout"
            last_stage = None
            recent = []
            for line in stdout_path.read_text(errors="replace").splitlines():
                try:
                    evt = json.loads(line)
                except json.JSONDecodeError:
                    continue
                recent.append(evt)
                if len(recent) > 6:
                    recent.pop(0)
                if evt.get("event") in ("stage_start", "stage_done"):
                    last_stage = evt.get("stage")
            duration = round(time.monotonic() - start, 2)
            status_path.write_text(json.dumps({
                "id": sample["id"], "variant": mode, "status": outcome,
                "duration_s": duration, "return_code": retcode,
                "last_stage": last_stage,
                "last_events": recent,
            }, indent=2))
            tail = stderr_path.read_text(errors="replace")[-650:]
            print(f"{mode} {sample['id']}: {outcome}, {duration:.1f}s, "
                  f"last_stage={last_stage}, stderr_tail={tail!r}", flush=True)



if __name__ == "__main__":
    p=argparse.ArgumentParser()
    p.add_argument("command",choices=["setup","run","evaluate"])
    p.add_argument("--root",default="benchdata")
    p.add_argument("--baseline",default="baseline/waypoint-core/target/release/waypoint-cli")
    p.add_argument("--phase1",default="waypoint-core/target/release/waypoint-cli")
    a=p.parse_args()
    if a.command=="setup":
        setup_images(Path(a.root))
        setup_models(Path(a.root))
    elif a.command=="run":
        run_tests(a.root,a.baseline,a.phase1)
    else:
        raise SystemExit(0 if evaluate(a.root) else 1)
