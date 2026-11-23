//! Deterministic spatial subsampling: cover nearby and distant search locations.
use crate::geo::haversine_m;

pub fn diverse<T, F>(items: Vec<T>, max_count: usize, center: (f64, f64), pos: F) -> Vec<T>
where F: Fn(&T) -> (f64, f64),
{
    let n = items.len();
    if n <= max_count { return items; }
    if max_count == 0 { return vec![]; }
    let coords: Vec<(f64, f64)> = items.iter().map(pos).collect();
    let mut picked = vec![false; n];
    let mut min_dist = vec![f64::INFINITY; n];
    let mut selection = Vec::with_capacity(max_count);
    let first = (0..n).min_by(|&a, &b| {
        let da = haversine_m(coords[a].0, coords[a].1, center.0, center.1);
        let db = haversine_m(coords[b].0, coords[b].1, center.0, center.1);
        da.total_cmp(&db).then_with(|| a.cmp(&b))
    }).unwrap();
    let mut current = first;
    for _ in 0..max_count {
        picked[current] = true;
        selection.push(current);
        for i in 0..n {
            if picked[i] { continue; }
            let dist = haversine_m(coords[i].0, coords[i].1, coords[current].0, coords[current].1);
            min_dist[i] = min_dist[i].min(dist);
        }
        if selection.len() == max_count { break; }
        current = (0..n).filter(|&i| !picked[i]).max_by(|&a, &b| {
            min_dist[a].total_cmp(&min_dist[b]).then_with(|| b.cmp(&a))
        }).unwrap();
    }
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    selection.into_iter().filter_map(|i| slots[i].take()).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn covers_near_and_both_extremes() {
        let a = diverse(vec![(-1.0, 0.0), (-0.5, 0.0), (0.0, 0.0), (0.5, 0.0), (1.0, 0.0)], 3, (0.0, 0.0), |p| *p);
        assert_eq!(a.len(), 3);
        assert!(a.contains(&(0.0,0.0)) && a.contains(&(-1.0,0.0)) && a.contains(&(1.0,0.0)));
    }
    #[test]
    fn deterministic_and_limited() {
        let items: Vec<_> = (0..100).map(|i| (i as f64 / 100.0, 0.0)).collect();
        assert_eq!(diverse(items.clone(), 12, (0.5,0.0), |p| *p),
                   diverse(items, 12, (0.5,0.0), |p| *p));
    }
}