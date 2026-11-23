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
ORT = ("https://github.com/microsoft/onnxruntime/releases/download/v1.24.4/"
       "onnxruntime-linux-x64-1.24.4.tgz")
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
    for mode in ("baseline", "phase1"):
        for gt in rows:
            path = output / f"{mode}_{gt['id']}.ndjson"
            records = []
            if path.exists():
                for line in path.read_text().splitlines():
                    try:
                        records.append(json.loads(line))
                    except json.JSONDecodeError:
                        pass
            done = next((r for r in reversed(records) if r.get("event") == "done"), None)
            errors = [r.get("message") for r in records if r.get("event") == "error"]
            row = dict(gt, variant=mode, error=";".join(str(e) for e in errors))
            if done:
                result = done.get("result", {})
                clusters = result.get("clusters") or []
                row["n_clusters"] = len(clusters)
                row["coarse_error_m"] = haversine(gt["gt_lat"],gt["gt_lon"],
                    clusters[0]["lat"],clusters[0]["lon"]) if clusters else None
                coord, matches, src = best_pred(result, gt["exclude_mapillary_id"])
                row["chosen_source"] = src
                row["n_matching_candidates"] = len(matches)
                row["best_error_m"] = haversine(gt["gt_lat"],gt["gt_lon"],*coord) if coord else None
                row["oracle_retrieval_error_m"] = min((haversine(
                    gt["gt_lat"],gt["gt_lon"],m["lat"],m["lon"]) for _,m in matches),default=None)
            else:
                row.update(n_clusters=0, coarse_error_m=None, chosen_source="failed",
                           n_matching_candidates=0,best_error_m=None,oracle_retrieval_error_m=None)
            details.append(row)
    for mode in ("baseline", "phase1"):
        group = [r for r in details if r["variant"] == mode]
        n = len(group)
        def metric(key,t):
            return sum(r[key] is not None and r[key] <= t for r in group)/n
        print(f"\n{mode}: total={n}, complete={sum(r['best_error_m'] is not None for r in group)}")
        for key in ("coarse_error_m","best_error_m","oracle_retrieval_error_m"):
            print("  ",key,
                  "med=",round(sorted(r[key] for r in group if r[key] is not None)[
                      sum(r[key] is not None for r in group)//2],0)
                       if any(r[key] is not None for r in group) else None,
                  "accuracy@10m=",metric(key,10),
                  "@100m=",metric(key,100),
                  "@1km=",metric(key,1000),
                  "@25km=",metric(key,25000))
    import csv as _csv
    with open(output/"details.csv", "w", newline="") as f:
        writer = _csv.DictWriter(f, fieldnames=details[0].keys())
        writer.writeheader()
        writer.writerows(details)
    (output/"summary.json").write_text(json.dumps(details,indent=2))
    print("Saved", output/"details.csv", output/"summary.json")


def run_tests(root, baseline_binary, phase_binary):
    root = Path(root)
    images = json.loads((root/"samples.json").read_text())
    for mode,binary in (("baseline",baseline_binary),("phase1",phase_binary)):
        for sample in images:
            image = root/"images"/sample["id"]
            cmd = [str(Path(binary).resolve()), "one", "--image_path", str(image.resolve()),
                   "--model","osv5m", "--num_samples","256", "--num_runs","1",
                   "--no_sun_refine", "--no_geocode", "--max_images","90",
                   "--verify_top_n","5", "--top_k","5",
                   "--retrieval_top_clusters",("1" if mode == "baseline" else "3")]
            env = dict(os.environ, WAYPOINT_ACCEL="cpu",
                       WAYPOINT_ORT_DLL=str((Path("benchdata")/"ort"/"libonnxruntime.so").resolve()),
                       WAYPOINT_ONNX_DIR=str((Path("benchdata")/"onnx").resolve()))
            print(f"\nRUN {mode} {sample['id']} at {time.strftime('%H:%M:%S')}",flush=True)
            start=time.monotonic()
            try:
                proc = subprocess.run(cmd,env=env,capture_output=True,text=True,timeout=420)
                (root/f"{mode}_{sample['id']}.ndjson").write_text(proc.stdout)
                (root/f"{mode}_{sample['id']}.stderr").write_text(proc.stderr)
                print(f"exit {proc.returncode}, seconds={time.monotonic()-start:.1f}, "
                      f"events={len(proc.stdout.splitlines())}, "
                      f"stderr_tail={proc.stderr[-350:]!r}",flush=True)
            except subprocess.TimeoutExpired as e:
                (root/f"{mode}_{sample['id']}.stderr").write_text("timeout: " + str(e))
                print("TIMED OUT", mode, sample["id"],flush=True)


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
        evaluate(a.root)
