use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub remaining: Option<f64>,
    pub minutes: Option<i64>,
    pub resets_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Bucket {
    pub id: String,
    pub name: String,
    pub primary: Option<QuotaWindow>,
    pub secondary: Option<QuotaWindow>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub buckets: Vec<Bucket>,
    pub selected: String,
    pub updated_at: Option<u64>,
    pub error: Option<String>,
    pub refreshing: bool,
}
fn window(v: &Value) -> Option<QuotaWindow> {
    v.as_object()?;
    Some(QuotaWindow {
        remaining: v["usedPercent"]
            .as_f64()
            .filter(|n| n.is_finite())
            .map(|n| (100.0 - n).clamp(0.0, 100.0)),
        minutes: v["windowDurationMins"].as_i64().filter(|n| *n > 0),
        resets_at: v["resetsAt"].as_i64().filter(|n| *n >= 0),
    })
}
pub fn bucket(v: &Value, fallback: &str) -> Option<Bucket> {
    v.as_object()?;
    let id = v["limitId"].as_str().unwrap_or(fallback).to_owned();
    Some(Bucket {
        name: v["limitName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(&id)
            .to_owned(),
        id,
        primary: window(&v["primary"]),
        secondary: window(&v["secondary"]),
    })
}
pub fn buckets(v: &Value) -> Vec<Bucket> {
    let mut result: Vec<_> = v["rateLimitsByLimitId"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.iter())
        .filter_map(|(id, v)| bucket(v, id))
        .collect();
    if result.is_empty() {
        if let Some(b) = bucket(&v["rateLimits"], "codex") {
            result.push(b);
        }
    }
    result.sort_by(|a, b| (a.id != "codex", &a.id).cmp(&(b.id != "codex", &b.id)));
    result
}
pub fn widget_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (width as f64 / 2560.0).min(height as f64 / 1440.0);
    (
        (500.0 * scale).round().max(1.0) as u32,
        (200.0 * scale).round().max(1.0) as u32,
    )
}
pub fn fit_position(x: i32, y: i32, work: (i32, i32, u32, u32), size: (u32, u32)) -> (i32, i32) {
    let (left, top, width, height) = work;
    (
        x.clamp(
            left,
            left.saturating_add(width.saturating_sub(size.0) as i32),
        ),
        y.clamp(
            top,
            top.saturating_add(height.saturating_sub(size.1) as i32),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn prefer_multiple_buckets_and_keep_unknowns() {
        let b = buckets(
            &json!({"rateLimits": {"primary":{"usedPercent":99}}, "rateLimitsByLimitId": {
                "other": {"primary": {"usedPercent": -4}},
                "codex": {"primary": {"usedPercent": 25, "windowDurationMins":300, "resetsAt":1730947200}, "secondary":{}}
            }}),
        );
        assert_eq!(b[0].id, "codex");
        assert_eq!(b[0].primary.as_ref().unwrap().remaining, Some(75.0));
        assert_eq!(b[0].primary.as_ref().unwrap().resets_at, Some(1730947200));
        assert_eq!(b[0].secondary.as_ref().unwrap().remaining, None);
        assert_eq!(b[1].primary.as_ref().unwrap().remaining, Some(100.0));
    }
    #[test]
    fn legacy_zero_and_missing() {
        assert!(buckets(&json!({})).is_empty());
        let b = buckets(&json!({"rateLimits":{"primary":{"usedPercent":120},"secondary":null}}));
        assert_eq!(b[0].primary.as_ref().unwrap().remaining, Some(0.0));
        assert_eq!(b[0].secondary, None);
    }
    #[test]
    fn sizing_is_physical_and_preserves_aspect() {
        assert_eq!(widget_size(2560, 1440), (500, 200));
        assert_eq!(widget_size(1920, 1080), (375, 150));
        assert_eq!(widget_size(3840, 2160), (750, 300));
        assert_eq!(widget_size(3440, 1440), (500, 200));
        // Tauri receives these physical pixels, not a second DPI multiplier.
        let logical_width_at_125_percent = widget_size(2560, 1440).0 as f64 / 1.25;
        assert_eq!(logical_width_at_125_percent, 400.0);
    }
    #[test]
    fn position_clamps_on_negative_coordinate_monitors() {
        assert_eq!(
            fit_position(-3000, 5000, (-1920, 0, 1920, 1040), (375, 150)),
            (-1920, 890)
        );
        assert_eq!(
            fit_position(9000, -100, (0, 0, 2560, 1400), (500, 200)),
            (2060, 0)
        );
    }
}
