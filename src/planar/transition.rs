use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};

use serde::Serialize;

use crate::resolver::{ResolvedScene, values::value_parts};

/// Sampled local CSS transforms, keyed by source node ID.
#[derive(Default, Serialize)]
pub struct Pose(BTreeMap<usize, String>);

impl Pose {
    pub fn transform(&self, node: usize) -> Option<&str> {
        self.0.get(&node).map(String::as_str)
    }
}

/// A bounded CSS transform transition for the planar preview. It supports
/// single-axis rotations and translations with compatible units. Other CSS
/// animation behavior is handled by the settled preview.
pub struct Transition {
    tracks: Vec<Track>,
    duration: Duration,
}

impl Transition {
    /// Compare checked-state cascades for the same document and viewport.
    /// An optional sampled pose provides the starting values on interruption.
    pub(super) fn between(
        from: &ResolvedScene,
        to: &ResolvedScene,
        root: usize,
        starting_pose: Option<&Pose>,
    ) -> Option<Self> {
        let before: BTreeMap<_, _> = from.nodes.iter().map(|node| (node.node.id, node)).collect();
        let mut inside = HashSet::new();
        let mut tracks = Vec::new();
        for next in &to.nodes {
            let id = next.node.id;
            if id != root && !next.node.parent_id.is_some_and(|id| inside.contains(&id)) {
                continue;
            }
            inside.insert(id);
            let before = &before.get(&id)?.style.projection_properties;
            let after = &next.style.projection_properties;
            if before.get("perspective") != after.get("perspective") {
                return None;
            }
            let initial = starting_pose
                .and_then(|pose| pose.transform(id))
                .or_else(|| before.get("transform").map(String::as_str))
                .unwrap_or("none");
            let target = after.get("transform").map_or("none", String::as_str);
            if initial == target {
                continue;
            }
            let animation = |style: &BTreeMap<String, String>| {
                style
                    .get("animation-name")
                    .or_else(|| style.get("animation"))
                    .is_some_and(|value| value != "none")
            };
            if animation(after) || after.keys().any(|key| key.starts_with("transition-")) {
                // The settled renderer already accounts for the new cascade.
                // Avoid presenting an invented path through unsupported motion.
                return None;
            }
            let Some(timing) = after.get("transition").and_then(|raw| Timing::parse(raw)) else {
                continue;
            };
            if timing.end() <= 0.0 {
                continue;
            }
            let transform = Transform::between(initial, target)?;
            tracks.push(Track {
                id,
                transform,
                timing,
            });
        }
        let duration = tracks
            .iter()
            .map(|track| track.timing.end())
            .reduce(f64::max)?;
        Some(Self {
            tracks,
            duration: Duration::try_from_secs_f64(duration).ok()?,
        })
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn sample(&self, at: Duration) -> Pose {
        let seconds = at.as_secs_f64();
        Pose(
            self.tracks
                .iter()
                .map(|track| {
                    let resolved = track.transform.sample(track.timing.sample(seconds));
                    (track.id, resolved)
                })
                .collect(),
        )
    }
}

struct Track {
    id: usize,
    transform: Transform,
    timing: Timing,
}

struct Timing {
    duration: f64,
    delay: f64,
    easing: [f64; 4],
}

impl Timing {
    fn parse(item: &str) -> Option<Self> {
        if crate::paint::layers(item).count() != 1 {
            return None;
        }
        let mut property = None;
        let mut times = Vec::with_capacity(2);
        let mut easing = None;
        for part in value_parts(item) {
            if let Some(time) = seconds(part) {
                times.push(time);
            } else if let Some(value) = easing_curve(part) {
                if easing.replace(value).is_some() {
                    return None;
                }
            } else if property.replace(part).is_some() {
                return None;
            }
        }
        if !matches!(property, None | Some("all" | "transform")) {
            return None;
        }
        let (&duration, delay) = times.split_first()?;
        if delay.len() > 1 || duration <= 0.0 {
            return None;
        }
        Some(Self {
            duration,
            delay: delay.first().copied().unwrap_or(0.0),
            easing: easing.unwrap_or([0.25, 0.1, 0.25, 1.0]),
        })
    }

    fn end(&self) -> f64 {
        self.delay + self.duration
    }

    fn sample(&self, seconds: f64) -> f64 {
        let progress = ((seconds - self.delay) / self.duration).clamp(0.0, 1.0);
        if matches!(progress, 0.0 | 1.0) {
            return progress;
        }
        let [x1, y1, x2, y2] = self.easing;
        let cubic = |t: f64, p1, p2| {
            let remaining = 1.0 - t;
            3.0 * remaining * remaining * t * p1 + 3.0 * remaining * t * t * p2 + t * t * t
        };
        let (mut lower, mut upper) = (0.0, 1.0);
        for _ in 0..24 {
            let t = (lower + upper) * 0.5;
            if cubic(t, x1, x2) < progress {
                lower = t;
            } else {
                upper = t;
            }
        }
        cubic((lower + upper) * 0.5, y1, y2)
    }
}

fn seconds(value: &str) -> Option<f64> {
    if let Some(ms) = value.strip_suffix("ms") {
        return finite(ms).map(|value| value / 1000.0);
    }
    finite(value.strip_suffix('s')?)
}

fn finite(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn easing_curve(value: &str) -> Option<[f64; 4]> {
    Some(match value {
        "linear" => [0.0, 0.0, 1.0, 1.0],
        "ease" => [0.25, 0.1, 0.25, 1.0],
        "ease-in" => [0.42, 0.0, 1.0, 1.0],
        "ease-out" => [0.0, 0.0, 0.58, 1.0],
        "ease-in-out" => [0.42, 0.0, 0.58, 1.0],
        _ => {
            let args = value.strip_prefix("cubic-bezier(")?.strip_suffix(')')?;
            let values: [f64; 4] = args
                .split(',')
                .map(|arg| finite(arg.trim()))
                .collect::<Option<Vec<_>>>()?
                .try_into()
                .ok()?;
            if !(0.0..=1.0).contains(&values[0]) || !(0.0..=1.0).contains(&values[2]) {
                return None;
            }
            values
        }
    })
}

struct Transform {
    function: String,
    components: Vec<Component>,
}

struct Component {
    start: f64,
    end: f64,
    unit: &'static str,
}

impl Transform {
    fn between(from: &str, to: &str) -> Option<Self> {
        let from_fn = transform_parts(from)?;
        let to_fn = transform_parts(to)?;
        let (name, args) = to_fn.as_ref().or(from_fn.as_ref())?;
        let (arity, rotation) = match name.as_str() {
            "rotate" | "rotatex" | "rotatey" | "rotatez" => (1, true),
            "translatex" | "translatey" | "translatez" => (1, false),
            "translate3d" => (3, false),
            _ => return None,
        };
        if args.len() != arity
            || from_fn
                .as_ref()
                .is_some_and(|(other, args)| other != name || args.len() != arity)
        {
            return None;
        }
        let components = (0..arity)
            .map(|index| {
                let value = |func: Option<&(String, Vec<&str>)>| {
                    let raw = func.map_or("0", |(_, args)| args[index]);
                    if rotation {
                        return degrees(raw).map(|value| (value, "deg"));
                    }
                    length(raw)
                };
                let (start, start_unit) = value(from_fn.as_ref())?;
                let (end, end_unit) = value(to_fn.as_ref())?;
                let unit = match (start_unit, end_unit) {
                    (unit, "") | ("", unit) => unit,
                    (a, b) if a == b => a,
                    _ => return None,
                };
                Some(Component { start, end, unit })
            })
            .collect::<Option<_>>()?;
        Some(Self {
            function: name.clone(),
            components,
        })
    }

    fn sample(&self, progress: f64) -> String {
        let components = self
            .components
            .iter()
            .map(|value| {
                let number = value.start + (value.end - value.start) * progress;
                format!("{number}{}", value.unit)
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({components})", self.function)
    }
}

fn transform_parts(value: &str) -> Option<Option<(String, Vec<&str>)>> {
    if value == "none" {
        return Some(None);
    }
    let (name, args) = value.split_once('(')?;
    let args = args.strip_suffix(')')?;
    Some(Some((
        name.to_ascii_lowercase(),
        args.split(',').map(str::trim).collect(),
    )))
}

fn degrees(raw: &str) -> Option<f64> {
    for (unit, factor) in [
        ("deg", 1.0),
        ("rad", 180.0 / std::f64::consts::PI),
        ("turn", 360.0),
    ] {
        if let Some(value) = raw.strip_suffix(unit) {
            return Some(finite(value)? * factor);
        }
    }
    finite(raw).filter(|&value| value == 0.0)
}

fn length(raw: &str) -> Option<(f64, &'static str)> {
    for suffix in ["rem", "em", "px", "vw", "vh", "%"] {
        if let Some(value) = raw.strip_suffix(suffix) {
            return Some((finite(value)?, suffix));
        }
    }
    finite(raw)
        .filter(|&value| value == 0.0)
        .map(|zero| (zero, ""))
}
