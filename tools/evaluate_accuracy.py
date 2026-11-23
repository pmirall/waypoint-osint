#!/usr/bin/env python3
"""Measure camera-position accuracy from a CSV with ground-truth coordinates.

CSV: id,gt_lat,gt_lon,pred_lat,pred_lon
Blank predicted coordinates indicate abstention. A rejected prediction counts
as incorrect for all-input accuracy, but contributes to the coverage metric.
"""
import argparse
import csv
import json
import math
import statistics

THRESHOLDS_M = (10, 100, 1000, 25000)


def haversine_m(lat1, lon1, lat2, lon2):
    a, b = map(math.radians, (lat1, lat2))
    dlat = b - a
    dlon = math.radians(lon2 - lon1)
    h = math.sin(dlat/2)**2 + math.cos(a)*math.cos(b)*math.sin(dlon/2)**2
    return 2 * 6371000 * math.asin(math.sqrt(min(1, max(0, h))))


def percentile(values, pct):
    x = sorted(values)
    k = (len(x)-1) * pct / 100
    lo = int(k)
    return x[lo]*(1-(k-lo)) + x[min(lo+1,len(x)-1)]*(k-lo)


def summarize(rows):
    if not rows:
        raise ValueError("No rows supplied")
    seen = set()
    errors = []
    for row in rows:
        key = row.get("id")
        if not key or key in seen:
            raise ValueError(f"Missing or duplicate id: {key}")
        seen.add(key)
        gt = [float(row[k]) for k in ("gt_lat", "gt_lon")]
        if not (-90 <= gt[0] <= 90 and -180 <= gt[1] <= 180):
            raise ValueError(f"Invalid ground truth: {key}")
        la, lo = row.get("pred_lat"), row.get("pred_lon")
        if bool(la) != bool(lo):
            raise ValueError(f"Incomplete prediction: {key}")
        if not la:
            errors.append(None)
            continue
        pred = [float(la), float(lo)]
        if not (-90 <= pred[0] <= 90 and -180 <= pred[1] <= 180):
            raise ValueError(f"Invalid prediction: {key}")
        errors.append(haversine_m(*gt, *pred))
    reported = [e for e in errors if e is not None]
    n, m = len(errors), len(reported)
    out = {
        "n_total": n,
        "n_predicted": m,
        "coverage": m/n,
        "median_error_m_on_predicted": statistics.median(reported) if reported else None,
        "p90_error_m_on_predicted": percentile(reported,90) if reported else None,
    }
    for t in THRESHOLDS_M:
        good = sum(e is not None and e <= t for e in errors)
        out[f"accuracy_all_at_{t}m"] = good/n
        out[f"accuracy_predicted_at_{t}m"] = good/m if m else None
    return out


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("predictions_csv")
    args = p.parse_args()
    with open(args.predictions_csv, newline="", encoding="utf-8") as f:
        print(json.dumps(summarize(list(csv.DictReader(f))), indent=2))
