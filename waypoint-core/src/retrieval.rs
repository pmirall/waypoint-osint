//! Shared retrieval-refinement machinery: embedding similarity ranking and
//! geometric verification of the top candidates (the common tail of
//! `mapillary_refine` / `google_sv_refine` / `panoramax_refine`).

use std::path::Path;

use image::RgbImage;
use serde_json::{json, Map, Value};

use crate::models::Models;
use crate::ransac;
use crate::util::{py_round, Cancel, Error, Result};

/// `on_progress(phase, completed, total)`
pub type Progress<'a> = &'a mut dyn FnMut(&str, usize, usize);

/// Everything a source needs to run.
pub struct Ctx<'a> {
    pub models: &'a dyn Models,
    pub target: &'a Path,
    pub fov: f64,
    pub cancel: &'a Cancel,
    pub log: &'a (dyn Fn(&str) + Sync),
}

#[derive(Clone, Debug)]
pub struct ClusterInfo {
    pub search_seeds: Vec<(f64, f64)>,
    pub lat: f64,
    pub lon: f64,
    pub lat_std: f64,
    pub lon_std: f64,
}

impl ClusterInfo {
    pub fn from_json(c: &Value) -> Self {
        let f = |k: &str| c.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let search_seeds = c.get("search_seeds").and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|p| {
                let p = p.as_array()?;
                Some((p.first()?.as_f64()?, p.get(1)?.as_f64()?))
            }).collect()).unwrap_or_default();
        ClusterInfo { lat: f("lat"), lon: f("lon"), lat_std: f("lat_std"), lon_std: f("lon_std"), search_seeds }
    }
}

/// Search several representative locations of a broad PLONK mode without
/// multiplying each source's image budget by the number of search seeds.
pub fn planned_seeds(c: &ClusterInfo, max_images: usize) -> Vec<(ClusterInfo, usize)> {
    let mut centers = vec![(c.lat, c.lon)];
    if max_images >= 90 {
        for &(lat, lon) in &c.search_seeds {
            if centers.iter().all(|&(a, b)| crate::geo::haversine_m(a, b, lat, lon) >= 6000.0) {
                centers.push((lat, lon));
            }
            if centers.len() >= 3 { break; }
        }
    }
    let count = centers.len();
    let base = max_images / count;
    let rest = max_images % count;
    centers.into_iter().enumerate().map(|(i, (lat, lon))| {
        let mut local = c.clone();
        if i != 0 { local.lat_std = 0.0; local.lon_std = 0.0; }
        local.lat = lat;
        local.lon = lon;
        local.search_seeds.clear();
        (local, base + usize::from(i < rest))
    }).collect()
}

/// Merge candidate matches from representative search locations.
pub fn merge_matches(mut rows: Vec<Value>, limit: usize) -> Vec<Value> {
    rows.sort_by(|a, b| {
        let score = |x: &Value| {
            let inl = x.get("inliers").and_then(Value::as_u64).unwrap_or(0) as f64;
            let tot = x.get("total_matches").and_then(Value::as_u64).unwrap_or(0) as f64;
            let sim = x.get("similarity").and_then(Value::as_f64).unwrap_or(0.0);
            if tot >= 12.0 && inl >= 12.0 {
                2.0 + (inl / tot).min(1.0) + (inl + 1.0).ln() * 0.1
            } else { sim }
        };
        score(b).total_cmp(&score(a))
    });
    let mut unique = std::collections::HashSet::new();
    rows.retain(|m| {
        let key = m.get("mapillary_url").or_else(|| m.get("street_view_url"))
            .or_else(|| m.get("panoramax_url")).and_then(Value::as_str).unwrap_or("");
        unique.insert(key.to_owned())
    });
    rows.truncate(limit);
    rows
}

/// Search radius from PLONK's own reported spread for the cluster, clamped.
pub fn cluster_radius_km(c: &ClusterInfo, min_km: f64, max_km: f64) -> f64 {
    let lat_km = c.lat_std * 111.0;
    let lon_km = c.lon_std * 111.0 * c.lat.to_radians().cos().max(0.1);
    lat_km.hypot(lon_km).clamp(min_km, max_km)
}

pub fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum();
    let na = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let nb = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    dot / (na * nb + 1e-8)
}

/// One candidate image with its source metadata, ready to be scored.
pub struct Scorable {
    pub lat: f64,
    pub lon: f64,
    /// (json key, url) of the "open this photo" link.
    pub url: (&'static str, String),
    pub image: RgbImage,
}

/// `verify_utils.match_pair` semantics on top of the model's matches.
pub fn verify_one(models: &dyn Models, target: &Path, cand: &RgbImage) -> (usize, usize) {
    match models.match_pair(target, cand) {
        Ok(m) => {
            if m.n_kp1 < 8 || m.n_kp2 < 8 {
                return (0, 0);
            }
            let total = m.pts1.len();
            if total < 8 {
                return (0, total);
            }
            (ransac::fundamental_ransac_inliers(&m.pts1, &m.pts2, 3.0, 0.99, 1000, 0x5EED), total)
        }
        Err(_) => (0, 0),
    }
}

/// Rank `cands` by embedding similarity to the target, geometrically verify
/// the top `verify_top_n`, and return the best `top_k` as JSON match objects.
pub fn score_and_verify(
    ctx: &Ctx,
    cands: Vec<Scorable>,
    verify: bool,
    verify_top_n: usize,
    top_k: usize,
    on_progress: Progress,
) -> Result<Vec<Value>> {
    ctx.cancel.check()?;
    let target_emb = ctx.models.embed_path(ctx.target)?;
    let refs: Vec<&RgbImage> = cands.iter().map(|c| &c.image).collect();
    let mut embs = Vec::with_capacity(cands.len());
    // Chunked so cancellation is noticed between batches.
    for chunk in refs.chunks(16) {
        ctx.cancel.check()?;
        embs.extend(ctx.models.embed(chunk)?);
    }
    if embs.len() != cands.len() {
        return Err(Error::Msg("embedder returned the wrong number of vectors".into()));
    }

    struct Row {
        sim: f64,
        idx: usize,
        inliers: Option<(usize, usize)>,
    }
    let mut rows: Vec<Row> = embs.iter().enumerate().map(|(i, e)| Row { sim: py_round(cosine_sim(&target_emb, e), 4), idx: i, inliers: None }).collect();
    rows.sort_by(|a, b| b.sim.total_cmp(&a.sim)); // stable, like list.sort(reverse=True) on the key

    if verify && !rows.is_empty() {
        let n = verify_top_n.min(rows.len());
        (ctx.log)(&format!("geometrically verifying top {n} candidates..."));
        for i in 0..n {
            ctx.cancel.check()?;
            let r = &mut rows[i];
            r.inliers = Some(verify_one(ctx.models, ctx.target, &cands[r.idx].image));
            on_progress("verify", i + 1, n);
        }
        // Stable sort by inliers, descending (Python: sort(key=..., reverse=True) keeps ties in order).
        rows[..n].sort_by(|a, b| b.inliers.map_or(0, |x| x.0).cmp(&a.inliers.map_or(0, |x| x.0)));
    }

    Ok(rows
        .into_iter()
        .take(top_k)
        .map(|r| {
            let c = &cands[r.idx];
            let mut m = Map::new();
            m.insert("similarity".into(), json!(r.sim));
            m.insert("lat".into(), json!(c.lat));
            m.insert("lon".into(), json!(c.lon));
            m.insert(c.url.0.into(), json!(c.url.1));
            if let Some((inl, tot)) = r.inliers {
                m.insert("inliers".into(), json!(inl));
                m.insert("total_matches".into(), json!(tot));
            }
            Value::Object(m)
        })
        .collect())
}
