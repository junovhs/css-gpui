//! Project border boxes within one continuous CSS preserve-3d context.

use std::collections::BTreeMap;

use crate::resolver::values::{LengthContext, tokens, value_parts};

pub(crate) type Point = [f64; 2];
type HomogeneousPoint = [f64; 4];

/// An untransformed border box in document coordinates, with its cascaded CSS.
pub struct Layer<'a> {
    pub origin: Point,
    pub size: Point,
    pub font_size: f32,
    pub css: &'a BTreeMap<String, String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProjectedBox {
    /// Border vertices in CSS order, before viewport clipping.
    /// A vertex on or behind the eye plane has no finite screen coordinate.
    pub corners: [Option<Point>; 4],
    /// Polygon clipped in homogeneous space against the eye and viewport.
    pub viewport_polygon: Vec<Point>,
}

#[derive(Debug)]
pub struct ProjectionError {
    pub layer: usize,
    pub property: &'static str,
    pub value: String,
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "projection layer {} has unsupported {} value {:?}",
            self.layer, self.property, self.value
        )
    }
}

impl std::error::Error for ProjectionError {}

/// Project the last box in a parent-to-child chain. Supply the untransformed
/// border boxes from a layout engine, in viewport coordinates at scroll zero.
/// The chain must include all ancestors affecting the projection and every
/// intermediate box must preserve 3D (including its used value after grouping
/// effects). This routine computes geometry independently of paint, such as
/// back-face culling, depth ordering, and ancestor overflow clipping.
pub fn project_box(
    layers: &[Layer<'_>],
    viewport: Point,
    root_font_size: f32,
) -> Result<Option<ProjectedBox>, ProjectionError> {
    let Some(last) = layers.last() else {
        return Ok(None);
    };
    let matrix = accumulated_matrix(layers, viewport, root_font_size)?;
    Ok(Some(matrix.project_box(last.size, viewport)))
}

/// Geometry and a prepared inverse for rasterizing the last box in a chain.
pub struct ProjectedPlane {
    pub geometry: ProjectedBox,
    /// Absent when the border plane has no stable screen inverse.
    pub sampling: Option<SamplingPlane>,
    /// The box center's homogeneous Z before perspective division. With one
    /// CSS perspective camera, this is the center's Z after the model
    /// transforms. Face ordering remains a paint decision.
    pub center_depth: f64,
}

/// Project the last box and prepare its viewport-to-border-plane mapping.
/// Uses the same layer and clipping rules as [`project_box`].
pub fn project_plane(
    layers: &[Layer<'_>],
    viewport: Point,
    root_font_size: f32,
) -> Result<Option<ProjectedPlane>, ProjectionError> {
    let Some(last) = layers.last() else {
        return Ok(None);
    };
    let matrix = accumulated_matrix(layers, viewport, root_font_size)?;
    let [width, height] = last.size;
    Ok(Some(ProjectedPlane {
        geometry: matrix.project_box(last.size, viewport),
        sampling: matrix.sampling_plane(),
        center_depth: matrix.apply([width * 0.5, height * 0.5, 0.0, 1.0])[2],
    }))
}

pub(crate) fn accumulated_matrix(
    layers: &[Layer<'_>],
    viewport: Point,
    root_font_size: f32,
) -> Result<Matrix, ProjectionError> {
    let mut matrix = Matrix::IDENTITY;
    let mut parent_origin = [0.0, 0.0];
    for (index, layer) in layers.iter().enumerate() {
        let css = |property| layer.css.get(property).map_or("none", String::as_str);
        let error = |property, value: &str| ProjectionError {
            layer: index,
            property,
            value: value.to_owned(),
        };
        let lengths = LengthContext {
            em: layer.font_size,
            rem: root_font_size,
            viewport: Some((viewport[0] as f32, viewport[1] as f32)),
            percent: None,
        };
        let local = [
            layer.origin[0] - parent_origin[0],
            layer.origin[1] - parent_origin[1],
            0.0,
        ];
        let transform = css("transform");
        let origin = layer
            .css
            .get("transform-origin")
            .map_or("50% 50%", String::as_str);
        let origin = parse_origin(origin, layer.size, lengths, true)
            .ok_or_else(|| error("transform-origin", origin))?;
        let transform = transform_matrix(transform, layer.size, lengths)
            .ok_or_else(|| error("transform", transform))?;
        matrix = matrix
            .then(Matrix::translation(local))
            .then(transform.around(origin));

        if index + 1 < layers.len() {
            let style = css("transform-style");
            if style != "preserve-3d" {
                return Err(error("transform-style", style));
            }
            let perspective = css("perspective");
            if perspective != "none" {
                let distance = lengths
                    .pixels(perspective)
                    .filter(|&d| d >= 0.0)
                    .ok_or_else(|| error("perspective", perspective))?;
                let origin = layer
                    .css
                    .get("perspective-origin")
                    .map_or("50% 50%", String::as_str);
                let origin = parse_origin(origin, layer.size, lengths, false)
                    .ok_or_else(|| error("perspective-origin", origin))?;
                let mut perspective = Matrix::IDENTITY;
                perspective.0[3][2] = -1.0 / f64::from(distance.max(1.0));
                matrix = matrix.then(perspective.around(origin));
            }
        }
        parent_origin = layer.origin;
    }
    Ok(matrix)
}

fn to_screen([x, y, _, w]: HomogeneousPoint) -> Option<Point> {
    (w > 0.0).then(|| [x / w, y / w])
}

fn clip_to_viewport(points: &[HomogeneousPoint], [width, height]: Point) -> Vec<Point> {
    let mut polygon = points.to_vec();
    let planes = [
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, width],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0, height],
    ];
    for plane in planes {
        let Some(&last) = polygon.last() else { break };
        let mut previous = last;
        let mut previous_distance = dot(plane, previous);
        let mut clipped = Vec::with_capacity(polygon.len() + 1);
        for point in polygon {
            let distance = dot(plane, point);
            if (distance < 0.0) != (previous_distance < 0.0) {
                let t = previous_distance / (previous_distance - distance);
                if t > 0.0 && t < 1.0 {
                    clipped.push(std::array::from_fn(|i| {
                        previous[i] + t * (point[i] - previous[i])
                    }));
                }
            }
            if distance >= 0.0 {
                clipped.push(point);
            }
            previous = point;
            previous_distance = distance;
        }
        polygon = clipped;
    }
    polygon.into_iter().filter_map(to_screen).collect()
}

fn dot(left: HomogeneousPoint, right: HomogeneousPoint) -> f64 {
    left.into_iter().zip(right).map(|(a, b)| a * b).sum()
}

#[derive(Clone, Copy)]
pub(crate) struct Matrix([[f64; 4]; 4]);

impl Matrix {
    pub(crate) fn project_box(self, [width, height]: Point, viewport: Point) -> ProjectedBox {
        let corners = [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]]
            .map(|[x, y]| self.apply([x, y, 0.0, 1.0]));
        ProjectedBox {
            corners: corners.map(to_screen),
            viewport_polygon: clip_to_viewport(&corners, viewport),
        }
    }

    /// Inverse of the plane-to-screen homography and the composed depth row.
    /// The cofactor also gives the transformed normal's z sign after dividing
    /// by the full determinant, even for reflected planes.
    pub(crate) fn sampling_plane(self) -> Option<SamplingPlane> {
        let plane = |row: usize| [self.0[row][0], self.0[row][1], self.0[row][3]];
        let cross = |[ax, ay, az]: [f64; 3], [bx, by, bz]: [f64; 3]| {
            [ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx]
        };
        let product =
            |a: [f64; 3], b: [f64; 3]| a.into_iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
        let [x, y, z, w] = [plane(0), plane(1), plane(2), plane(3)];
        let adjugate_columns = [cross(y, w), cross(w, x), cross(x, y)];
        let area = product(x, adjugate_columns[0]);
        if !area.is_finite() || area.abs() <= f64::EPSILON {
            return None;
        }
        let determinant = self.0[0][2] * product(y, cross(z, w))
            - self.0[1][2] * product(x, cross(z, w))
            + self.0[2][2] * area
            - self.0[3][2] * product(x, cross(y, z));
        Some(SamplingPlane {
            inverse: std::array::from_fn(|row| adjugate_columns.map(|column| column[row] / area)),
            depth: z,
            back_facing: area * determinant < -f64::EPSILON,
        })
    }

    const IDENTITY: Self = Self([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    fn translation([x, y, z]: [f64; 3]) -> Self {
        let mut matrix = Self::IDENTITY;
        matrix.0[0][3] = x;
        matrix.0[1][3] = y;
        matrix.0[2][3] = z;
        matrix
    }

    fn scale([x, y, z]: [f64; 3]) -> Self {
        let mut matrix = Self::IDENTITY;
        matrix.0[0][0] = x;
        matrix.0[1][1] = y;
        matrix.0[2][2] = z;
        matrix
    }

    fn rotation(axis: usize, angle: f64) -> Self {
        let (sine, cosine) = angle.sin_cos();
        let mut matrix = Self::IDENTITY;
        let (first, second) = ((axis + 1) % 3, (axis + 2) % 3);
        matrix.0[first][first] = cosine;
        matrix.0[second][second] = cosine;
        matrix.0[first][second] = -sine;
        matrix.0[second][first] = sine;
        matrix
    }

    fn around(self, origin: [f64; 3]) -> Self {
        Self::translation(origin)
            .then(self)
            .then(Self::translation(origin.map(|value| -value)))
    }

    fn then(self, right: Self) -> Self {
        Self(
            self.0
                .map(|row| std::array::from_fn(|column| dot(row, right.0.map(|row| row[column])))),
        )
    }

    fn apply(self, point: HomogeneousPoint) -> HomogeneousPoint {
        self.0.map(|row| dot(row, point))
    }
}

/// The inverse screen mapping of a projected border plane.
pub struct SamplingPlane {
    inverse: [[f64; 3]; 3],
    depth: [f64; 3],
    pub(crate) back_facing: bool,
}

impl SamplingPlane {
    /// Map a viewport point to local border coordinates and depth (Z/W).
    /// Samples on or behind the eye plane are absent. The caller checks
    /// whether a returned point is inside the border box or viewport.
    #[inline]
    pub fn sample(&self, x: f64, y: f64) -> Option<(Point, f64)> {
        let [u, v, w] = self.inverse.map(|[a, b, c]| a * x + b * y + c);
        (w > 0.0).then(|| {
            (
                [u / w, v / w],
                self.depth[0] * u + self.depth[1] * v + self.depth[2] * w,
            )
        })
    }
}

fn parse_origin(
    value: &str,
    [width, height]: Point,
    lengths: LengthContext,
    depth: bool,
) -> Option<[f64; 3]> {
    let parts = value_parts(value).collect::<Vec<_>>();
    let (mut x, mut y, z) = match parts.as_slice() {
        [vertical @ ("top" | "bottom")] => ("center", *vertical, "0"),
        [x] => (*x, "center", "0"),
        [x, y] => (*x, *y, "0"),
        [x, y, z] if depth => (*x, *y, *z),
        _ => return None,
    };
    if matches!(x, "top" | "bottom") || matches!(y, "left" | "right") {
        std::mem::swap(&mut x, &mut y);
    }
    let keywords = |token, start, end| match token {
        "center" => "50%",
        token if token == start => "0%",
        token if token == end => "100%",
        _ => token,
    };
    Some([
        length(lengths, keywords(x, "left", "right"), Some(width))?,
        length(lengths, keywords(y, "top", "bottom"), Some(height))?,
        length(lengths, z, None)?,
    ])
}

fn length(lengths: LengthContext, value: &str, relative: Option<f64>) -> Option<f64> {
    LengthContext {
        percent: relative.map(|value| value as f32),
        ..lengths
    }
    .pixels(value)
    .map(f64::from)
}

fn transform_matrix(mut value: &str, size: Point, lengths: LengthContext) -> Option<Matrix> {
    let mut matrix = Matrix::IDENTITY;
    if value == "none" {
        return Some(matrix);
    }
    while !value.is_empty() {
        let (name, arguments) = value.split_once('(')?;
        // Find the closing delimiter of the function, skipping nested calc() expressions.
        let end = tokens(arguments).find_map(|(index, token)| (token == ')').then_some(index))?;
        let arguments_text = &arguments[..end];
        let mut start = 0;
        let args = tokens(arguments_text)
            .filter_map(|(index, token)| (token == ',').then_some(index))
            .chain(std::iter::once(arguments_text.len()))
            .map(|end| {
                let arg = arguments_text[start..end].trim();
                start = end + 1;
                arg
            })
            .collect::<Vec<_>>();
        matrix = matrix.then(transform_function(name, &args, size, lengths)?);
        value = arguments[end + 1..].trim_start();
    }
    Some(matrix)
}

fn transform_function(
    name: &str,
    args: &[&str],
    size: Point,
    lengths: LengthContext,
) -> Option<Matrix> {
    let px = |value, axis| length(lengths, value, axis);
    let x = |value| px(value, Some(size[0]));
    let y = |value| px(value, Some(size[1]));
    let z = |value| px(value, None);
    let number = |value: &str| value.parse::<f64>().ok().filter(|n| n.is_finite());
    let angle = |value: &str| {
        if let Some(degrees) = value.strip_suffix("deg") {
            return Some(number(degrees)?.to_radians());
        }
        if let Some(radians) = value.strip_suffix("rad") {
            return number(radians);
        }
        if let Some(turns) = value.strip_suffix("turn") {
            return Some(number(turns)? * std::f64::consts::TAU);
        }
        number(value).filter(|&n| n == 0.0)
    };
    Some(match (name.to_ascii_lowercase().as_str(), args) {
        ("translate3d", [a, b, c]) => Matrix::translation([x(a)?, y(b)?, z(c)?]),
        ("translate", [a]) | ("translatex", [a]) => Matrix::translation([x(a)?, 0.0, 0.0]),
        ("translate", [a, b]) => Matrix::translation([x(a)?, y(b)?, 0.0]),
        ("translatey", [a]) => Matrix::translation([0.0, y(a)?, 0.0]),
        ("translatez", [a]) => Matrix::translation([0.0, 0.0, z(a)?]),
        ("rotate" | "rotatez", [a]) => Matrix::rotation(2, angle(a)?),
        ("rotatex", [a]) => Matrix::rotation(0, angle(a)?),
        ("rotatey", [a]) => Matrix::rotation(1, angle(a)?),
        ("scale", [a]) => Matrix::scale([number(a)?, number(a)?, 1.0]),
        ("scale", [a, b]) => Matrix::scale([number(a)?, number(b)?, 1.0]),
        ("scalex", [a]) => Matrix::scale([number(a)?, 1.0, 1.0]),
        ("scaley", [a]) => Matrix::scale([1.0, number(a)?, 1.0]),
        ("scalez", [a]) => Matrix::scale([1.0, 1.0, number(a)?]),
        ("scale3d", [a, b, c]) => Matrix::scale([number(a)?, number(b)?, number(c)?]),
        _ => return None,
    })
}
