//! Local-first geolocation benchmark runner.
//! Runs inside the desktop application; never starts GitHub Actions or a cloud job.
//! Benchmark photos are downloaded once, reports are written after each sample,
//! and every inference has a cooperative cancellation deadline.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use waypoint_core::geo::haversine_m;
use waypoint_core::native;
use waypoint_core::{run as core_run, Cancel, Mode, RunArgs, Sink};

use crate::AppState;

const DATASET: &str = "https://huggingface.co/datasets/lebfla11/osm-europe-1k/resolve/main";
const BENCH_PROTOCOL: &str = "waypoint-local-v1";
const ENGINE_CACHE_KEY: &str = "phase1-2026-11-23";
const SMOKE_PHOTOS: [&str; 4] = ["00054.jpg", "00060.jpg", "00059.jpg", "00055.jpg"];
const MAX_IMAGE_BYTES: u64 = 18 * 1024 * 1024;

pub struct BenchmarkState {
    active: Mutex<Option<Arc<AtomicBool>>>,
}

impl BenchmarkState {
    pub fn new() -> Self {
        Self { active: Mutex::new(None) }
    }
    pub fn is_running(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkOptions {
    pub preset: String,
    pub profile: String,
    pub max_seconds: u64,
    pub use_cache: bool,
}

#[derive(Clone)]
struct Photo {
    id: String,
    latitude: f64,
    longitude: f64,
    country: String,
}

fn epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn bench_root(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().paths.data.join("benchmarks")
}

fn emit(app: &AppHandle, event: &str, more: Value) {
    let mut o = serde_json::Map::new();
    o.insert("event".into(), json!(event));
    if let Value::Object(fields) = more {
        o.extend(fields);
    }
    let _ = app.emit("benchmark-event", Value::Object(o));
}

fn write_json(path: &Path, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = path.with_extension("partial");
    let data = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    fs::write(&part, data).map_err(|e| e.to_string())?;
    // Windows does not always allow renaming over an existing file.
    if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
    fs::rename(part, path).map_err(|e| e.to_string())
}

fn download_cached(url: &str, path: &Path, max_bytes: u64, stop: &AtomicBool) -> Result<(), String> {
    if path.metadata().is_ok_and(|m| m.len() > 100 && m.len() <= max_bytes) {
        return Ok(());
    }
    if stop.load(Ordering::Relaxed) { return Err("Cancelled".into()); }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = path.with_extension("partial");
    let result = (|| -> Result<(), String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .timeout_write(Duration::from_secs(20))
            .build();
        let response = agent.get(url).call().map_err(|e| format!("Image dataset download: {e}"))?;
        let mut source = response.into_reader();
        let mut dest = File::create(&part).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            if stop.load(Ordering::Relaxed) { return Err("Cancelled".into()); }
            let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 { break; }
            total += n as u64;
            if total > max_bytes { return Err("Dataset file exceeds safe download limit".into()); }
            dest.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        if total < 100 { return Err("Dataset file is empty or unexpectedly short".into()); }
        dest.flush().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(part);
        return Err(e);
    }
    if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
    fs::rename(part, path).map_err(|e| e.to_string())
}

/// Quote-aware parsing for the simple UTF-8 dataset manifest (no multiline fields).
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if quoted && chars.peek() == Some(&'"') {
                field.push('"');
                chars.next();
            } else {
                quoted = !quoted;
            }
        } else if ch == ',' && !quoted {
            fields.push(std::mem::take(&mut field));
        } else {
            field.push(ch);
        }
    }
    fields.push(field);
    fields
}

fn valid_photo_id(id: &str) -> bool {
    !id.contains("..")
        && id.len() < 90
        && (id.ends_with(".jpg") || id.ends_with(".jpeg"))
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn parse_manifest(text: &str) -> Result<Vec<Photo>, String> {
    let mut lines = text.lines();
    let header = csv_fields(lines.next().ok_or("Dataset metadata is empty")?);
    let col = |key: &str| header.iter().position(|x| x == key)
        .ok_or_else(|| format!("Dataset metadata lacks '{key}' column"));
    let (file_idx, lat_idx, lon_idx, country_idx) =
        (col("filename")?, col("lat")?, col("lon")?, col("country")?);
    let mut photos = Vec::new();
    let mut unique = HashSet::new();
    for line in lines {
        let row = csv_fields(line);
        let Some(file) = row.get(file_idx) else { continue; };
        if !valid_photo_id(file) || !unique.insert(file.clone()) { continue; }
        let (Some(la), Some(lo)) = (row.get(lat_idx), row.get(lon_idx)) else { continue; };
        let (Ok(latitude), Ok(longitude)) = (la.parse::<f64>(), lo.parse::<f64>()) else { continue; };
        if !latitude.is_finite() || !longitude.is_finite()
            || latitude.abs() > 90.0 || longitude.abs() > 180.0 { continue; }
        photos.push(Photo {
            id: file.clone(), latitude, longitude,
            country: row.get(country_idx).cloned().unwrap_or_default(),
        });
    }
    if photos.len() < 4 { return Err("Dataset contains too few valid geotagged images".into()); }
    Ok(photos)
}

fn stable_hash(id: &str) -> u64 {
    let mut h = 14695981039346656037u64;
    for c in id.bytes() {
        h ^= c as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn select_photos(photos: Vec<Photo>, count: usize) -> Result<Vec<Photo>, String> {
    if photos.len() < count { return Err(format!("Only {} photos available, need {count}", photos.len())); }
    if count == 4 {
        let mut out = Vec::new();
        for id in SMOKE_PHOTOS {
            out.push(photos.iter().find(|p| p.id == id)
                .ok_or_else(|| format!("Required smoke test image {id} not found"))?.clone());
        }
        return Ok(out);
    }
    let mut per_country: BTreeMap<String, Vec<Photo>> = BTreeMap::new();
    for p in photos { per_country.entry(p.country.clone()).or_default().push(p); }
    for photos in per_country.values_mut() { photos.sort_by_key(|p| stable_hash(&p.id)); }
    let mut selected = Vec::with_capacity(count);
    let mut depth = 0;
    while selected.len() < count {
        let mut grew = false;
        for photos in per_country.values() {
            if let Some(p) = photos.get(depth) {
                selected.push(p.clone());
                grew = true;
                if selected.len() == count { break; }
            }
        }
        if !grew { break; }
        depth += 1;
    }
    if selected.len() != count { return Err("Could not select requested number of photographs".into()); }
    Ok(selected)
}

struct BenchSink {
    result: Mutex<Option<Value>>,
    error: Mutex<Option<String>>,
    stage: Mutex<String>,
    app: AppHandle,
}

impl BenchSink {
    fn new(app: &AppHandle) -> Self {
        Self { result: Mutex::new(None), error: Mutex::new(None),
               stage: Mutex::new("model_load".into()), app: app.clone() }
    }
}

impl Sink for BenchSink {
    fn event(&self, event: Value) {
        let kind = event["event"].as_str().unwrap_or("");
        match kind {
            "done" => { *self.result.lock().unwrap() = Some(event["result"].clone()); }
            "error" => {
                *self.error.lock().unwrap() = Some(
                    event["message"].as_str().unwrap_or("Pipeline failure").to_owned());
            }
            "stage_start" | "stage_done" => {
                let stage = event["stage"].as_str().unwrap_or("").to_owned();
                *self.stage.lock().unwrap() = stage.clone();
                emit(&self.app, "stage", json!({ "stage": stage, "step": kind }));
            }
            _ => {}
        }
    }
    fn log(&self, _line: &str) {
        // Model/provider logs can include URLs or tokens. Never expose them in public reports.
    }
}

fn prediction(result: &Value, ground_truth: &Photo, profile: &str) -> Value {
    let clusters = result.get("clusters").and_then(Value::as_array);
    let Some(clusters) = clusters else { return json!({"prediction": null, "reason": "no_clusters"}); };
    let Some(first) = clusters.first() else { return json!({"prediction": null, "reason": "no_clusters"}); };
    let coarse = first.get("lat").and_then(Value::as_f64)
        .zip(first.get("lon").and_then(Value::as_f64));
    let coarse_error = coarse.map(|(a, b)| haversine_m(ground_truth.latitude, ground_truth.longitude, a, b));
    // Coarse and fine evaluation are separate; do not claim that a PLONK
    // cluster centre is a verified <=10m target camera pose.
    if profile == "coarse" {
        return json!({ "prediction": coarse.map(|(la, lo)| json!([la, lo])),
                       "method": "coarse", "error_m": coarse_error,
                       "coarse_error_m": coarse_error, "oracle_error_m": null });
    }
    let mut candidates = Vec::<(f64, f64, f64, f64, u64)>::new();
    for cluster in clusters {
        for key in ["google_sv_matches", "mapillary_matches", "panoramax_matches"] {
            for m in cluster.get(key).and_then(Value::as_array).into_iter().flatten() {
                let (Some(lat), Some(lon)) = (m["lat"].as_f64(), m["lon"].as_f64()) else { continue; };
                let inliers = m["inliers"].as_u64().unwrap_or(0);
                let total = m["total_matches"].as_u64().unwrap_or(0);
                let sim = m["similarity"].as_f64().unwrap_or(0.0);
                // Prefer geometrically verified matches. This is a heuristic,
                // not a statistically calibrated location confidence.
                let rank = if inliers >= 12 && total >= 12 {
                    2.0 + inliers as f64 / total as f64 + (inliers as f64).ln() * 0.1
                } else { sim };
                candidates.push((rank, lat, lon, sim, inliers));
            }
        }
    }
    let oracle = candidates.iter().map(|(_, la, lo, _, _)| {
        haversine_m(ground_truth.latitude, ground_truth.longitude, *la, *lo)
    }).reduce(f64::min);
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let best = candidates.first().map(|(_, la, lo, _, _)| (*la, *lo)).or(coarse);
    let method = if candidates.is_empty() { "coarse_fallback" } else { "visual_candidate" };
    json!({ "prediction": best.map(|(la,lo)| json!([la,lo])),
            "method": method, "error_m": best.map(|(la,lo)| haversine_m(ground_truth.latitude, ground_truth.longitude, la, lo)),
            "coarse_error_m": coarse_error,
            "oracle_error_m": oracle, "retrieved": candidates.len() })
}

fn metrics(rows: &[Value], n: usize, profile: &str) -> Value {
    let valid = rows.iter().filter(|r| r["status"] == "complete").count();
    let errors: Vec<f64> = rows.iter().filter(|r| r["status"] == "complete")
        .filter_map(|r| r["error_m"].as_f64()).filter(|x| x.is_finite()).collect();
    let mut sorted = errors.clone();
    sorted.sort_by(f64::total_cmp);
    let accuracy = |threshold: f64| -> Value {
        if n == 0 { return Value::Null; }
        json!(errors.iter().filter(|x| **x <= threshold).count() as f64 / n as f64)
    };
    json!({
        "profile": profile,
        "total": n,
        "processed": rows.len(),
        "completed": valid,
        "failed": rows.iter().filter(|r| r["status"] == "error").count(),
        "timed_out": rows.iter().filter(|r| r["status"] == "timeout").count(),
        "cancelled": rows.iter().filter(|r| r["status"] == "cancelled").count(),
        "coverage": if n > 0 { valid as f64 / n as f64 } else { 0.0 },
        "accuracy_10m": accuracy(10.0),
        "accuracy_100m": accuracy(100.0),
        "accuracy_1km": accuracy(1000.0),
        "accuracy_25km": accuracy(25000.0),
        "median_error_m": if sorted.is_empty() { Value::Null } else { json!(sorted[sorted.len()/2]) },
        "metric_note": if profile == "coarse" {
            "PLONK cluster-centre error, NOT precision geolocation"
        } else {
            "Reference-image or coarse fallback error; NOT verified target-camera position"
        },
    })
}

fn run(app: AppHandle, opts: BenchmarkOptions, stopped: Arc<AtomicBool>) -> Result<(), String> {
    let dir = bench_root(&app);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let count = match opts.preset.as_str() {
        "smoke" => 4, "pilot" => 50, "full" => 500, _ => return Err("Invalid benchmark preset".into()),
    };
    let profile = opts.profile.as_str();
    if !["coarse", "precision"].contains(&profile) { return Err("Unknown benchmark profile".into()); }

    emit(&app, "status", json!({ "message": "Preparing geotagged image dataset", "total": count }));
    let metadata = dir.join("metadata.csv");
    download_cached(&format!("{DATASET}/metadata.csv"), &metadata, 10_000_000, &stopped)?;
    let data = fs::read_to_string(&metadata).map_err(|e| e.to_string())?;
    let photos = select_photos(parse_manifest(&data)?, count)?;

    let loc = native::locate(&app.state::<AppState>().paths.data)
        .ok_or_else(|| "Install Waypoint's ONNX models in Setup first".to_string())?;
    if !loc.has("osv5m") { return Err("OSV-5M model unavailable. Run Waypoint Setup.".into()); }
    emit(&app, "status", json!({ "message": "Loading ONNX models locally" }));
    let models = native::open(&loc, "osv5m", &|_s| {})
        .map_err(|e| format!("Could not load local model: {e}"))?;
    let report_file = dir.join("reports").join(format!("{}_{}_{}_{}.json",
        epoch(), opts.preset, profile, ENGINE_CACHE_KEY));
    let mut rows: Vec<Value> = Vec::new();
    let started = epoch();
    let mut finished = false;
    for (index, photo) in photos.iter().enumerate() {
        if stopped.load(Ordering::Relaxed) { break; }
        emit(&app, "photo", json!({
            "index": index + 1, "total": count, "id": photo.id,
            "country": photo.country, "message": "Geolocating on this computer"
        }));
        let cache_file = dir.join("predictions")
            .join(ENGINE_CACHE_KEY)
            .join(profile)
            .join(format!("{}.json", photo.id));
        let cached = if opts.use_cache {
            fs::read_to_string(&cache_file).ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .filter(|v| v["status"] == "complete")
        } else { None };
        let row = if let Some(previous) = cached {
            emit(&app, "status", json!({ "message": "Reusing saved prediction" }));
            previous
        } else {
            let image_path = dir.join("images").join(&photo.id);
            let url = format!("{DATASET}/images/{}", photo.id);
            let started_at = Instant::now();
            match download_cached(&url, &image_path, MAX_IMAGE_BYTES, &stopped) {
                Err(e) => json!({"id": photo.id, "country": photo.country, "status": "error",
                    "error": e, "duration_s": started_at.elapsed().as_secs_f64()}),
                Ok(()) => {
                    let mut args = RunArgs::new(Mode::One, &image_path);
                    args.model = "osv5m".into();
                    args.num_samples = if profile == "coarse" { 96 } else { 192 };
                    args.num_runs = 1;
                    args.no_geocode = true;
                    args.no_sun_refine = true;
                    args.retrieval_top_clusters = if profile == "coarse" { 0 } else { 1 };
                    args.no_verify = profile == "coarse";
                    args.max_images = 18;
                    args.verify_top_n = 3;
                    args.radius_km = Some(1.2);
                    // No Mapillary token: avoids an exact-image match against
                    // the Mapillary-sourced benchmark dataset.
                    args.mapillary_token = None;
                    let cancel = Cancel::new();
                    let watchdog_cancel = cancel.clone();
                    let watchdog_stop = Arc::new(AtomicBool::new(false));
                    let done = watchdog_stop.clone();
                    let user_stop = stopped.clone();
                    let timed_out = Arc::new(AtomicBool::new(false));
                    let watchdog_timeout = timed_out.clone();
                    let seconds = opts.max_seconds;
                    let watchdog = thread::spawn(move || {
                        let began = Instant::now();
                        while !done.load(Ordering::Relaxed) {
                            if user_stop.load(Ordering::Relaxed) {
                                watchdog_cancel.cancel();
                                break;
                            }
                            if began.elapsed() >= Duration::from_secs(seconds) {
                                watchdog_timeout.store(true, Ordering::Relaxed);
                                watchdog_cancel.cancel();
                                break;
                            }
                            thread::sleep(Duration::from_millis(200));
                        }
                    });
                    let sink = BenchSink::new(&app);
                    let exit_code = core_run(&args, &models, &sink, &cancel);
                    watchdog_stop.store(true, Ordering::Relaxed);
                    let _ = watchdog.join();
                    let elapsed = started_at.elapsed().as_secs_f64();
                    let stage = sink.stage.lock().unwrap().clone();
                    let result = sink.result.lock().unwrap().clone();
                    let error = sink.error.lock().unwrap().clone();
                    let status = if stopped.load(Ordering::Relaxed) {
                        "cancelled"
                    } else if timed_out.load(Ordering::Relaxed) {
                        "timeout"
                    } else if exit_code == 0 && result.is_some() {
                        "complete"
                    } else { "error" };
                    let mut row = json!({
                        "id": photo.id, "country": photo.country,
                        "gt_lat": photo.latitude, "gt_lon": photo.longitude,
                        "status": status, "last_stage": stage, "duration_s": elapsed,
                        "error": error,
                    });
                    if status == "complete" {
                        let stats = prediction(&result.unwrap(), photo, profile);
                        if let Some(fields) = stats.as_object() {
                            for (k, v) in fields { row[k] = v.clone(); }
                        }
                        let _ = write_json(&cache_file, &row);
                    }
                    row
                }
            }
        };
        rows.push(row.clone());
        let progress = json!({ "index": index + 1, "total": count, "row": row });
        emit(&app, "result", progress);
        let partial_report = json!({
            "protocol": BENCH_PROTOCOL,
            "engine": ENGINE_CACHE_KEY, "preset": opts.preset,
            "profile": profile, "started_at": started,
            "finished": false, "rows": rows,
            "metrics": metrics(&rows, count, profile)
        });
        write_json(&report_file, &partial_report)?;
        if stopped.load(Ordering::Relaxed) { break; }
    }
    if !stopped.load(Ordering::Relaxed) { finished = true; }
    let report = json!({
        "protocol": BENCH_PROTOCOL, "engine": ENGINE_CACHE_KEY,
        "preset": opts.preset, "profile": profile,
        "started_at": started, "finished": finished,
        "rows": rows, "metrics": metrics(&rows, count, profile)
    });
    write_json(&report_file, &report)?;
    emit(&app, "finished", json!({ "report": report, "report_path": report_file.to_string_lossy() }));
    Ok(())
}

#[tauri::command]
pub fn benchmark_start(app: AppHandle, opts: BenchmarkOptions) -> Result<(), String> {
    if !["smoke", "pilot", "full"].contains(&opts.preset.as_str()) {
        return Err("Invalid preset".into());
    }
    if !["coarse", "precision"].contains(&opts.profile.as_str()) {
        return Err("Invalid profile".into());
    }
    if !(30..=600).contains(&opts.max_seconds) {
        return Err("Per-photo time limit must be 30–600 seconds".into());
    }
    if app.state::<AppState>().active_run.lock().unwrap().is_some() {
        return Err("Stop the normal photo search before starting a benchmark".into());
    }
    let state = app.state::<BenchmarkState>();
    let mut active = state.active.lock().unwrap();
    if active.is_some() { return Err("Benchmark already running".into()); }
    let stop = Arc::new(AtomicBool::new(false));
    *active = Some(stop.clone());
    drop(active);
    thread::spawn(move || {
        let result = run(app.clone(), opts, stop);
        if let Err(error) = result {
            emit(&app, "failed", json!({ "error": error }));
        }
        *app.state::<BenchmarkState>().active.lock().unwrap() = None;
    });
    Ok(())
}

#[tauri::command]
pub fn benchmark_cancel(state: State<'_, BenchmarkState>) {
    if let Some(stop) = state.active.lock().unwrap().as_ref() {
        stop.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub fn benchmark_status(app: AppHandle) -> Value {
    let root = bench_root(&app).join("reports");
    let mut files: Vec<PathBuf> = fs::read_dir(&root).ok().into_iter().flatten()
        .filter_map(|entry| entry.ok().map(|x| x.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    let latest = files.last()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    json!({
        "running": app.state::<BenchmarkState>().is_running(),
        "last_report": latest,
        "report_dir": root.to_string_lossy(),
        "github_actions": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_manifest_and_rejects_bad_coordinates() {
        let text = "filename,lat,lon,country\n00054.jpg,46.58,4.19,FR\n00060.jpg,43.37,-4.91,ES\n00059.jpg,46.66,10.50,IT\n00055.jpg,37.24,-8.11,PT\n../../evil.jpg,0,0,ES\nfoo.jpg,999,1,FR\n";
        let rows = parse_manifest(text).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(select_photos(rows, 4).unwrap()[1].country, "ES");
        assert_eq!(csv_fields("a,\"b,c\",\"x\"\"y\""), ["a","b,c","x\"y"]);
    }

    #[test]
    fn metrics_do_not_treat_failures_as_successes() {
        let rows = vec![json!({"status":"complete","error_m":7.0}),
                        json!({"status":"timeout"}),json!({"status":"error"})];
        let m = metrics(&rows, 3, "precision");
        assert_eq!(m["completed"], 1);
        assert_eq!(m["accuracy_10m"], json!(1.0/3.0));
        assert_eq!(m["timed_out"], 1);
    }

    #[test]
    fn deterministic_country_stratification() {
        let mut a = vec![];
        for i in 0..60 {
            a.push(Photo {id:format!("a{i}.jpg"), latitude:0.0,longitude:0.0,
                          country: if i%2==0 {"ES"} else {"FR"}.into()});
        }
        let b = select_photos(a.clone(), 50).unwrap();
        let c = select_photos(a, 50).unwrap();
        assert_eq!(b.iter().map(|p| &p.id).collect::<Vec<_>>(),
                   c.iter().map(|p| &p.id).collect::<Vec<_>>());
    }
}
