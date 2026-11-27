//! Чистый Rust-декодер и векторный растеризатор анимаций TGS (gzipped Lottie JSON) для
//! стикеров Telegram и пользовательских Premium-эмодзи (`lib_lottie` / Bodymovin v5).
//! Поддерживает ключевые кадры с кривыми Безье, иерархию родителей (`parent`/`ind`), прекомпы,
//! контуры Безье (`sh`), прямоугольники (`rc`), эллипсы (`el`), звёзды (`sr`), Trim Path (`tm`),
//! заливки (`fl`), градиенты (`gf`), обводки (`st`), правила чётности/ненулевого обхода и
//! 2x2 сглаживание без внешних зависимостей.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use eframe::egui::ColorImage;
use flate2::read::GzDecoder;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default)]
pub struct Mat2D {
    pub m: [f32; 6], // [a, b, c, d, tx, ty]: x' = a*x + c*y + tx; y' = b*x + d*y + ty
}

impl Mat2D {
    pub fn mul(self, rhs: Self) -> Self {
        let [a1, b1, c1, d1, tx1, ty1] = self.m;
        let [a2, b2, c2, d2, tx2, ty2] = rhs.m;
        Self {
            m: [
                a1 * a2 + c1 * b2,
                b1 * a2 + d1 * b2,
                a1 * c2 + c1 * d2,
                b1 * c2 + d1 * d2,
                a1 * tx2 + c1 * ty2 + tx1,
                b1 * tx2 + d1 * ty2 + ty1,
            ],
        }
    }

    pub fn transform_pt(self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, tx, ty] = self.m;
        (a * x + c * y + tx, b * x + d * y + ty)
    }

    pub fn approx_scale(self) -> f32 {
        let sx = (self.m[0] * self.m[0] + self.m[1] * self.m[1]).sqrt();
        let sy = (self.m[2] * self.m[2] + self.m[3] * self.m[3]).sqrt();
        0.5 * (sx + sy)
    }
}

#[derive(Clone, Debug)]
struct Keyframe<T> {
    t: f32,
    s: T,
    e: Option<T>,
    ox: f32,
    oy: f32,
    ix: f32,
    iy: f32,
    hold: bool,
}

#[derive(Clone, Debug)]
enum AnimProp<T> {
    Static(T),
    Keyframed(Vec<Keyframe<T>>),
}

impl AnimProp<f32> {
    fn eval(&self, frame: f32) -> f32 {
        match self {
            Self::Static(v) => *v,
            Self::Keyframed(kfs) => eval_keyframes(kfs, frame, |a, b, u| a + (b - a) * u).unwrap_or(0.0),
        }
    }
}

impl AnimProp<[f32; 4]> {
    fn eval(&self, frame: f32) -> [f32; 4] {
        match self {
            Self::Static(v) => *v,
            Self::Keyframed(kfs) => eval_keyframes(kfs, frame, |a, b, u| {
                [
                    a[0] + (b[0] - a[0]) * u,
                    a[1] + (b[1] - a[1]) * u,
                    a[2] + (b[2] - a[2]) * u,
                    a[3] + (b[3] - a[3]) * u,
                ]
            })
            .unwrap_or([0.0; 4]),
        }
    }
}

#[derive(Clone, Debug)]
struct BezierShape {
    vertices: Vec<[f32; 2]>,
    in_pts: Vec<[f32; 2]>,
    out_pts: Vec<[f32; 2]>,
    closed: bool,
}

impl BezierShape {
    fn lerp(&self, other: &Self, u: f32) -> Self {
        let n = self.vertices.len().min(other.vertices.len());
        let mut vertices = Vec::with_capacity(n);
        let mut in_pts = Vec::with_capacity(n);
        let mut out_pts = Vec::with_capacity(n);
        for i in 0..n {
            let l2 = |a: [f32; 2], b: [f32; 2]| [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u];
            vertices.push(l2(self.vertices[i], other.vertices[i]));
            let ai = self.in_pts.get(i).copied().unwrap_or([0.0, 0.0]);
            let bi = other.in_pts.get(i).copied().unwrap_or([0.0, 0.0]);
            let ao = self.out_pts.get(i).copied().unwrap_or([0.0, 0.0]);
            let bo = other.out_pts.get(i).copied().unwrap_or([0.0, 0.0]);
            in_pts.push(l2(ai, bi));
            out_pts.push(l2(ao, bo));
        }
        Self {
            vertices,
            in_pts,
            out_pts,
            closed: self.closed,
        }
    }

    fn flatten(&self) -> (Vec<[f32; 2]>, bool) {
        let n = self.vertices.len();
        if n == 0 {
            return (Vec::new(), false);
        }
        let mut pts = Vec::with_capacity(n * 8);
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        for i in 0..segs {
            let j = (i + 1) % n;
            let p0 = self.vertices[i];
            let p3 = self.vertices[j];
            let o = self.out_pts.get(i).copied().unwrap_or([0.0, 0.0]);
            let ii = self.in_pts.get(j).copied().unwrap_or([0.0, 0.0]);
            if i == 0 {
                pts.push(p0);
            }
            if o[0].abs() < 1e-3 && o[1].abs() < 1e-3 && ii[0].abs() < 1e-3 && ii[1].abs() < 1e-3 {
                pts.push(p3);
            } else {
                let p1 = [p0[0] + o[0], p0[1] + o[1]];
                let p2 = [p3[0] + ii[0], p3[1] + ii[1]];
                const STEPS: usize = 10;
                for s in 1..=STEPS {
                    let t = s as f32 / STEPS as f32;
                    let mt = 1.0 - t;
                    let x = mt * mt * mt * p0[0]
                        + 3.0 * mt * mt * t * p1[0]
                        + 3.0 * mt * t * t * p2[0]
                        + t * t * t * p3[0];
                    let y = mt * mt * mt * p0[1]
                        + 3.0 * mt * mt * t * p1[1]
                        + 3.0 * mt * t * t * p2[1]
                        + t * t * t * p3[1];
                    pts.push([x, y]);
                }
            }
        }
        (pts, self.closed)
    }
}

fn solve_cubic_bezier(ox: f32, oy: f32, ix: f32, iy: f32, target_x: f32) -> f32 {
    if (ox - oy).abs() < 1e-4 && (ix - iy).abs() < 1e-4 {
        return target_x;
    }
    let sample_x = |t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * ox + 3.0 * mt * t * t * ix + t * t * t
    };
    let sample_y = |t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * oy + 3.0 * mt * t * t * iy + t * t * t
    };
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    for _ in 0..10 {
        let mid = 0.5 * (lo + hi);
        if sample_x(mid) < target_x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    sample_y(0.5 * (lo + hi)).clamp(-0.2, 1.2)
}

fn eval_keyframes<T: Clone>(
    kfs: &[Keyframe<T>],
    frame: f32,
    lerp: impl Fn(&T, &T, f32) -> T,
) -> Option<T> {
    if kfs.is_empty() {
        return None;
    }
    if frame <= kfs[0].t {
        return Some(kfs[0].s.clone());
    }
    for w in kfs.windows(2) {
        let a = &w[0];
        let b = &w[1];
        if frame < b.t {
            if a.hold {
                return Some(a.s.clone());
            }
            let span = (b.t - a.t).max(1e-5);
            let raw_u = ((frame - a.t) / span).clamp(0.0, 1.0);
            let u = solve_cubic_bezier(a.ox, a.oy, a.ix, a.iy, raw_u);
            let target = a.e.as_ref().unwrap_or(&b.s);
            return Some(lerp(&a.s, target, u));
        }
    }
    Some(kfs.last()?.s.clone())
}

#[derive(Clone, Debug)]
struct Transform {
    anchor: AnimProp<[f32; 4]>,
    pos_x: Option<AnimProp<f32>>,
    pos_y: Option<AnimProp<f32>>,
    pos: AnimProp<[f32; 4]>,
    scale: AnimProp<[f32; 4]>,
    rotation: AnimProp<f32>,
    opacity: AnimProp<f32>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            anchor: AnimProp::Static([0.0; 4]),
            pos_x: None,
            pos_y: None,
            pos: AnimProp::Static([0.0; 4]),
            scale: AnimProp::Static([100.0, 100.0, 100.0, 100.0]),
            rotation: AnimProp::Static(0.0),
            opacity: AnimProp::Static(100.0),
        }
    }
}

impl Transform {
    fn eval_matrix(&self, frame: f32) -> Mat2D {
        let a = self.anchor.eval(frame);
        let p = match (&self.pos_x, &self.pos_y) {
            (Some(px), Some(py)) => [px.eval(frame), py.eval(frame), 0.0, 0.0],
            _ => self.pos.eval(frame),
        };
        let s = self.scale.eval(frame);
        let sx = s[0] * 0.01;
        let sy = s[1] * 0.01;
        let rad = self.rotation.eval(frame).to_radians();
        let (sin, cos) = rad.sin_cos();

        // T(p) * R(rad) * S(sx, sy) * T(-a)
        let r00 = cos * sx;
        let r01 = -sin * sy;
        let r10 = sin * sx;
        let r11 = cos * sy;
        let tx = p[0] - (r00 * a[0] + r01 * a[1]);
        let ty = p[1] - (r10 * a[0] + r11 * a[1]);
        Mat2D { m: [r00, r10, r01, r11, tx, ty] }
    }

    fn eval_opacity(&self, frame: f32) -> f32 {
        (self.opacity.eval(frame) * 0.01).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug)]
enum ShapeItem {
    Group {
        items: Vec<ShapeItem>,
        transform: Transform,
    },
    Path {
        shape: AnimProp<BezierShape>,
    },
    Rect {
        pos: AnimProp<[f32; 4]>,
        size: AnimProp<[f32; 4]>,
        radius: AnimProp<f32>,
    },
    Ellipse {
        pos: AnimProp<[f32; 4]>,
        size: AnimProp<[f32; 4]>,
    },
    Polystar {
        star: bool,
        points: AnimProp<f32>,
        pos: AnimProp<[f32; 4]>,
        outer_r: AnimProp<f32>,
        inner_r: AnimProp<f32>,
        rot: AnimProp<f32>,
    },
    Trim {
        start: AnimProp<f32>,
        end: AnimProp<f32>,
        offset: AnimProp<f32>,
    },
    Fill {
        color: AnimProp<[f32; 4]>,
        opacity: AnimProp<f32>,
        even_odd: bool,
    },
    Stroke {
        color: AnimProp<[f32; 4]>,
        opacity: AnimProp<f32>,
        width: AnimProp<f32>,
    },
}

#[derive(Clone, Debug)]
enum LayerKind {
    Precomp { ref_id: String },
    Solid { color: [f32; 3], w: f32, h: f32 },
    Null,
    Shape { items: Vec<ShapeItem> },
}

#[derive(Clone, Debug)]
struct Layer {
    ind: i64,
    parent: Option<i64>,
    ip: f32,
    op: f32,
    st: f32,
    sr: f32,
    hidden: bool,
    transform: Transform,
    kind: LayerKind,
}

#[derive(Clone, Debug)]
pub struct TgsAnim {
    pub fps: f32,
    pub ip: f32,
    pub op: f32,
    pub width: f32,
    pub height: f32,
    layers: Vec<Layer>,
    assets: HashMap<String, Vec<Layer>>,
}

/// Повторно используемые буферы растеризации TGS, исключающие покадровые аллокации ~1 МБ в куче.
#[derive(Default)]
pub struct TgsScratch {
    fb: Vec<f32>,
}

impl TgsAnim {
    /// Декодирует `.tgs` (gzipped Lottie JSON) или обычный JSON.
    pub fn from_bytes(bytes: &[u8]) -> Option<Arc<Self>> {
        let json_bytes: Vec<u8> = if bytes.starts_with(&[0x1f, 0x8b]) {
            let dec = GzDecoder::new(bytes);
            let mut out = Vec::new();
            dec.take(8 * 1024 * 1024).read_to_end(&mut out).ok()?;
            out
        } else {
            bytes.to_vec()
        };
        let root: Value = serde_json::from_slice(&json_bytes).ok()?;
        let finite_or = |v: f32, def: f32| if v.is_finite() { v } else { def };
        let fps = finite_or(root.get("fr").and_then(Value::as_f64).unwrap_or(60.0) as f32, 60.0).clamp(1.0, 240.0);
        let ip = finite_or(root.get("ip").and_then(Value::as_f64).unwrap_or(0.0) as f32, 0.0);
        let op = finite_or(root.get("op").and_then(Value::as_f64).unwrap_or(60.0) as f32, 60.0);
        let width = finite_or(root.get("w").and_then(Value::as_f64).unwrap_or(512.0) as f32, 512.0).clamp(1.0, 8192.0);
        let height = finite_or(root.get("h").and_then(Value::as_f64).unwrap_or(512.0) as f32, 512.0).clamp(1.0, 8192.0);
        let layers = root
            .get("layers")
            .and_then(Value::as_array)
            .map(|a| parse_layers(a))
            .unwrap_or_default();
        let mut assets = HashMap::new();
        if let Some(arr) = root.get("assets").and_then(Value::as_array) {
            for asset in arr {
                if let (Some(id), Some(al)) = (
                    asset.get("id").and_then(Value::as_str),
                    asset.get("layers").and_then(Value::as_array),
                ) {
                    assets.insert(id.to_owned(), parse_layers(al));
                }
            }
        }
        if layers.is_empty() {
            return None;
        }
        Some(Arc::new(Self {
            fps,
            ip,
            op: op.max(ip + 1.0),
            width,
            height,
            layers,
            assets,
        }))
    }

    pub fn duration_secs(&self) -> f32 {
        ((self.op - self.ip) / self.fps).max(0.25)
    }

    pub fn frame_at_time(&self, time_secs: f64) -> f32 {
        let dur = f64::from(self.duration_secs());
        let frac = (time_secs.rem_euclid(dur) / dur) as f32;
        self.ip + frac * (self.op - self.ip)
    }

    /// Растеризует кадр `frame` в квадратный `ColorImage` размера `px_size×px_size` (с 2×2 сглаживанием),
    /// используя переданный `TgsScratch` для 0 лишних аллокаций в основном буфере.
    pub fn render_frame_into(&self, frame: f32, px_size: usize, scratch: &mut TgsScratch) -> ColorImage {
        let sz = px_size.clamp(16, 256);
        let ss = sz * 2;
        let scale = ss as f32 / self.width.max(self.height);
        let ox = (ss as f32 - self.width * scale) * 0.5;
        let oy = (ss as f32 - self.height * scale) * 0.5;
        let root_mat = Mat2D { m: [scale, 0.0, 0.0, scale, ox, oy] };
        let need = ss * ss * 4;
        scratch.fb.clear();
        scratch.fb.resize(need, 0.0f32);
        self.render_layers(&self.layers, frame, root_mat, 1.0, &mut scratch.fb, ss, 0);

        let mut pixels = Vec::with_capacity(sz * sz);
        for y in 0..sz {
            for x in 0..sz {
                let mut acc = [0.0f32; 4];
                for sy in 0..2 {
                    for sx in 0..2 {
                        let off = ((y * 2 + sy) * ss + (x * 2 + sx)) * 4;
                        acc[0] += scratch.fb[off];
                        acc[1] += scratch.fb[off + 1];
                        acc[2] += scratch.fb[off + 2];
                        acc[3] += scratch.fb[off + 3];
                    }
                }
                let r = (acc[0] * 0.25 * 255.0).round().clamp(0.0, 255.0) as u8;
                let g = (acc[1] * 0.25 * 255.0).round().clamp(0.0, 255.0) as u8;
                let b = (acc[2] * 0.25 * 255.0).round().clamp(0.0, 255.0) as u8;
                let a = (acc[3] * 0.25 * 255.0).round().clamp(0.0, 255.0) as u8;
                pixels.push(eframe::egui::Color32::from_rgba_premultiplied(r, g, b, a));
            }
        }
        ColorImage {
            size: [sz, sz],
            source_size: eframe::egui::Vec2::new(sz as f32, sz as f32),
            pixels,
        }
    }

    /// Растеризует кадр `frame` в квадратный `ColorImage` размера `px_size×px_size` (с 2×2 сглаживанием).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_frame(&self, frame: f32, px_size: usize) -> ColorImage {
        let mut scratch = TgsScratch::default();
        self.render_frame_into(frame, px_size, &mut scratch)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_layers(
        &self,
        layers: &[Layer],
        frame: f32,
        parent_mat: Mat2D,
        parent_op: f32,
        fb: &mut [f32],
        ss: usize,
        depth: usize,
    ) {
        if depth > 6 {
            return;
        }
        // Слои в AE: индекс 0 сверху, последний внизу — рисуем в обратном порядке.
        for layer in layers.iter().rev() {
            if layer.hidden || frame < layer.ip || frame >= layer.op {
                continue;
            }
            let (mat, op) = resolve_layer_transform(layers, layer, frame, parent_mat, parent_op);
            if op <= 0.002 {
                continue;
            }
            match &layer.kind {
                LayerKind::Null => {}
                LayerKind::Solid { color, w, h } => {
                    let contour = vec![[0.0, 0.0], [*w, 0.0], [*w, *h], [0.0, *h]];
                    let transformed: Vec<[f32; 2]> = contour
                        .into_iter()
                        .map(|[x, y]| {
                            let (px, py) = mat.transform_pt(x, y);
                            [px, py]
                        })
                        .collect();
                    fill_contours(fb, ss, &[(transformed, true)], *color, op, false);
                }
                LayerKind::Precomp { ref_id } => {
                    if let Some(sub) = self.assets.get(ref_id) {
                        let sub_frame = (frame - layer.st) / layer.sr.max(0.01);
                        self.render_layers(sub, sub_frame, mat, op, fb, ss, depth + 1);
                    }
                }
                LayerKind::Shape { items } => {
                    render_shape_list(items, frame, mat, op, fb, ss);
                }
            }
        }
    }
}

fn resolve_layer_transform(
    layers: &[Layer],
    layer: &Layer,
    frame: f32,
    root_mat: Mat2D,
    root_op: f32,
) -> (Mat2D, f32) {
    let mut chain = vec![layer];
    let mut cur = layer.parent;
    let mut guard = 0usize;
    while let Some(pid) = cur
        && guard < 16
    {
        guard += 1;
        if let Some(p) = layers.iter().find(|l| l.ind == pid) {
            chain.push(p);
            cur = p.parent;
        } else {
            break;
        }
    }
    let mut m = root_mat;
    for l in chain.into_iter().rev() {
        m = m.mul(l.transform.eval_matrix(frame));
    }
    (m, root_op * layer.transform.eval_opacity(frame))
}

fn render_shape_list(
    items: &[ShapeItem],
    frame: f32,
    mat: Mat2D,
    opacity: f32,
    fb: &mut [f32],
    ss: usize,
) {
    let mut contours: Vec<(Vec<[f32; 2]>, bool)> = Vec::new();
    let mut trim: Option<(f32, f32)> = None;

    for item in items {
        if let ShapeItem::Trim { start, end, offset } = item {
            let mut s = start.eval(frame) * 0.01;
            let mut e = end.eval(frame) * 0.01;
            let o = offset.eval(frame) / 360.0;
            if s > e {
                std::mem::swap(&mut s, &mut e);
            }
            trim = Some((s + o, e + o));
        }
    }

    // Собираем геометрии и краски слева направо, вложенные группы рисуются поверх/снизу согласно AE.
    for item in items.iter().rev() {
        if let ShapeItem::Group { items: sub, transform } = item {
            let sub_mat = mat.mul(transform.eval_matrix(frame));
            let sub_op = opacity * transform.eval_opacity(frame);
            if sub_op > 0.002 {
                render_shape_list(sub, frame, sub_mat, sub_op, fb, ss);
            }
        }
    }

    for item in items {
        match item {
            ShapeItem::Group { .. } | ShapeItem::Trim { .. } => {}
            ShapeItem::Path { shape } => {
                if let Some(sh) = eval_bezier_prop(shape, frame) {
                    contours.push(sh.flatten());
                }
            }
            ShapeItem::Rect { pos, size, radius } => {
                let c = pos.eval(frame);
                let sz = size.eval(frame);
                let r_raw = radius.eval(frame);
                if c[0].is_finite() && c[1].is_finite() && sz[0].is_finite() && sz[1].is_finite() && r_raw.is_finite() {
                    let r = r_raw.min(sz[0].abs() * 0.5).min(sz[1].abs() * 0.5).max(0.0);
                    contours.push((rounded_rect_pts(c[0], c[1], sz[0], sz[1], r), true));
                }
            }
            ShapeItem::Ellipse { pos, size } => {
                let c = pos.eval(frame);
                let sz = size.eval(frame);
                contours.push((ellipse_pts(c[0], c[1], sz[0] * 0.5, sz[1] * 0.5), true));
            }
            ShapeItem::Polystar { star, points, pos, outer_r, inner_r, rot } => {
                let c = pos.eval(frame);
                let pts = polystar_pts(
                    *star,
                    points.eval(frame),
                    c[0],
                    c[1],
                    outer_r.eval(frame),
                    inner_r.eval(frame),
                    rot.eval(frame),
                );
                contours.push((pts, true));
            }
            ShapeItem::Fill { color, opacity: fop, even_odd } => {
                let a = opacity * (fop.eval(frame) * 0.01).clamp(0.0, 1.0);
                if a > 0.002 && !contours.is_empty() {
                    let rgb = color.eval(frame);
                    let world = transform_contours(&contours, mat);
                    fill_contours(fb, ss, &world, [rgb[0], rgb[1], rgb[2]], a, *even_odd);
                }
            }
            ShapeItem::Stroke { color, opacity: sop, width } => {
                let a = opacity * (sop.eval(frame) * 0.01).clamp(0.0, 1.0);
                let w = width.eval(frame) * mat.approx_scale();
                if a > 0.002 && w > 0.05 && !contours.is_empty() {
                    let rgb = color.eval(frame);
                    let world = transform_contours(&contours, mat);
                    let use_contours = if let Some((ts, te)) = trim {
                        trim_contours(&world, ts, te)
                    } else {
                        world
                    };
                    stroke_contours(fb, ss, &use_contours, [rgb[0], rgb[1], rgb[2]], a, w);
                }
            }
        }
    }
}

fn transform_contours(contours: &[(Vec<[f32; 2]>, bool)], mat: Mat2D) -> Vec<(Vec<[f32; 2]>, bool)> {
    contours
        .iter()
        .map(|(pts, closed)| {
            (
                pts.iter()
                    .map(|&[x, y]| {
                        let (px, py) = mat.transform_pt(x, y);
                        [px, py]
                    })
                    .collect(),
                *closed,
            )
        })
        .collect()
}

fn trim_contours(contours: &[(Vec<[f32; 2]>, bool)], s_raw: f32, e_raw: f32) -> Vec<(Vec<[f32; 2]>, bool)> {
    let span = (e_raw - s_raw).clamp(0.0, 1.0);
    if span <= 0.001 {
        return Vec::new();
    }
    if span >= 0.999 {
        return contours.to_vec();
    }
    let s = s_raw.rem_euclid(1.0);
    let e = s + span;
    let mut out = Vec::new();
    for (pts, closed) in contours {
        if pts.len() < 2 {
            continue;
        }
        let mut expanded = pts.clone();
        if *closed && expanded.first() != expanded.last() {
            expanded.push(expanded[0]);
        }
        let mut total = 0.0f32;
        let mut lens = Vec::with_capacity(expanded.len());
        lens.push(0.0f32);
        for w in expanded.windows(2) {
            total += (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            lens.push(total);
        }
        if total < 1e-4 {
            continue;
        }
        let slice_range = |a_frac: f32, b_frac: f32| -> Vec<[f32; 2]> {
            let da = a_frac * total;
            let db = b_frac * total;
            let mut seg = Vec::new();
            for i in 0..expanded.len() - 1 {
                let l0 = lens[i];
                let l1 = lens[i + 1];
                if l1 <= da || l0 >= db || (l1 - l0) < 1e-6 {
                    continue;
                }
                let p0 = expanded[i];
                let p1 = expanded[i + 1];
                let t0 = ((da - l0) / (l1 - l0)).clamp(0.0, 1.0);
                let t1 = ((db - l0) / (l1 - l0)).clamp(0.0, 1.0);
                if seg.is_empty() {
                    seg.push([p0[0] + (p1[0] - p0[0]) * t0, p0[1] + (p1[1] - p0[1]) * t0]);
                }
                seg.push([p0[0] + (p1[0] - p0[0]) * t1, p0[1] + (p1[1] - p0[1]) * t1]);
            }
            seg
        };
        if e <= 1.0 {
            let res = slice_range(s, e);
            if res.len() >= 2 {
                out.push((res, false));
            }
        } else {
            let p1 = slice_range(s, 1.0);
            if p1.len() >= 2 {
                out.push((p1, false));
            }
            let p2 = slice_range(0.0, e - 1.0);
            if p2.len() >= 2 {
                out.push((p2, false));
            }
        }
    }
    out
}

fn fill_contours(
    fb: &mut [f32],
    ss: usize,
    contours: &[(Vec<[f32; 2]>, bool)],
    rgb: [f32; 3],
    alpha: f32,
    even_odd: bool,
) {
    if ss == 0 || !alpha.is_finite() || alpha <= 0.0 || !rgb[0].is_finite() || !rgb[1].is_finite() || !rgb[2].is_finite() {
        return;
    }
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut edges: Vec<(f32, f32, f32, f32, i32)> = Vec::new();
    for (pts, _) in contours {
        let n = pts.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            if !a[0].is_finite() || !a[1].is_finite() || !b[0].is_finite() || !b[1].is_finite() {
                continue;
            }
            min_y = min_y.min(a[1]);
            max_y = max_y.max(a[1]);
            if (a[1] - b[1]).abs() < 1e-5 {
                continue;
            }
            let dir = if a[1] < b[1] { 1 } else { -1 };
            let (y0, x0, y1, x1) = if a[1] < b[1] {
                (a[1], a[0], b[1], b[0])
            } else {
                (b[1], b[0], a[1], a[0])
            };
            edges.push((y0, y1, x0, (x1 - x0) / (y1 - y0), dir));
        }
    }
    if edges.is_empty() || !min_y.is_finite() || !max_y.is_finite() {
        return;
    }
    let max_c = (ss as isize) - 1;
    let y0_i = min_y.floor() as isize;
    let y1_i = max_y.ceil() as isize;
    if y1_i < 0 || y0_i > max_c {
        return;
    }
    let y_start = y0_i.clamp(0, max_c) as usize;
    let y_end = y1_i.clamp(0, max_c) as usize;
    if y_start > y_end {
        return;
    }
    let mut hits: Vec<(f32, i32)> = Vec::new();
    let sr = rgb[0] * alpha;
    let sg = rgb[1] * alpha;
    let sb = rgb[2] * alpha;
    let inv_a = 1.0 - alpha;

    for y in y_start..=y_end {
        let py = y as f32 + 0.5;
        hits.clear();
        for &(y0, y1, x0, inv_m, dir) in &edges {
            if py >= y0 && py < y1 {
                hits.push((x0 + (py - y0) * inv_m, dir));
            }
        }
        if hits.len() < 2 {
            continue;
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut winding = 0i32;
        for pair in hits.windows(2) {
            winding += pair[0].1;
            let inside = if even_odd { (winding & 1) != 0 } else { winding != 0 };
            if !inside || !pair[0].0.is_finite() || !pair[1].0.is_finite() {
                continue;
            }
            let x0_i = pair[0].0.ceil() as isize;
            let x1_i = pair[1].0.floor() as isize;
            if x1_i < 0 || x0_i > max_c || x1_i < x0_i {
                continue;
            }
            let x0 = x0_i.clamp(0, max_c) as usize;
            let x1 = x1_i.clamp(0, max_c) as usize;
            for x in x0..=x1 {
                let off = (y * ss + x) * 4;
                fb[off] = sr + fb[off] * inv_a;
                fb[off + 1] = sg + fb[off + 1] * inv_a;
                fb[off + 2] = sb + fb[off + 2] * inv_a;
                fb[off + 3] = alpha + fb[off + 3] * inv_a;
            }
        }
    }
}

fn stroke_contours(
    fb: &mut [f32],
    ss: usize,
    contours: &[(Vec<[f32; 2]>, bool)],
    rgb: [f32; 3],
    alpha: f32,
    width: f32,
) {
    if ss == 0
        || !width.is_finite()
        || !alpha.is_finite()
        || alpha <= 0.0
        || !rgb[0].is_finite()
        || !rgb[1].is_finite()
        || !rgb[2].is_finite()
    {
        return;
    }
    let half_r = (width * 0.5).clamp(0.6, ss as f32);
    let r2 = half_r * half_r;
    let pad = half_r.ceil() as isize;
    let mut segs: Vec<([f32; 2], [f32; 2])> = Vec::new();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for (pts, closed) in contours {
        if pts.len() < 2 {
            continue;
        }
        let cnt = if *closed { pts.len() } else { pts.len() - 1 };
        for i in 0..cnt {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            if !a[0].is_finite() || !a[1].is_finite() || !b[0].is_finite() || !b[1].is_finite() {
                continue;
            }
            min_x = min_x.min(a[0]).min(b[0]);
            min_y = min_y.min(a[1]).min(b[1]);
            max_x = max_x.max(a[0]).max(b[0]);
            max_y = max_y.max(a[1]).max(b[1]);
            segs.push((a, b));
        }
    }
    if segs.is_empty() || !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return;
    }
    let max_c = (ss as isize) - 1;
    let x0_i = (min_x.floor() as isize).saturating_sub(pad);
    let x1_i = (max_x.ceil() as isize).saturating_add(pad);
    let y0_i = (min_y.floor() as isize).saturating_sub(pad);
    let y1_i = (max_y.ceil() as isize).saturating_add(pad);
    if x1_i < 0 || y1_i < 0 || x0_i > max_c || y0_i > max_c {
        return;
    }
    let x0 = x0_i.clamp(0, max_c) as usize;
    let x1 = x1_i.clamp(0, max_c) as usize;
    let y0 = y0_i.clamp(0, max_c) as usize;
    let y1 = y1_i.clamp(0, max_c) as usize;
    if x0 > x1 || y0 > y1 {
        return;
    }
    let sr = rgb[0] * alpha;
    let sg = rgb[1] * alpha;
    let sb = rgb[2] * alpha;
    let inv_a = 1.0 - alpha;

    // Отмечаем только пиксели внутри bbox каждого сегмента (O(segs * seg_area) вместо O(W * H * segs)).
    let box_w = x1 - x0 + 1;
    let box_h = y1 - y0 + 1;
    let Some(mask_len) = box_w.checked_mul(box_h) else {
        return;
    };
    let mut mask = vec![0u8; mask_len];
    let pad_f = half_r + 1.0;
    for &(a, b) in &segs {
        let sx0_i = (a[0].min(b[0]) - pad_f).floor() as isize;
        let sx1_i = (a[0].max(b[0]) + pad_f).ceil() as isize;
        let sy0_i = (a[1].min(b[1]) - pad_f).floor() as isize;
        let sy1_i = (a[1].max(b[1]) + pad_f).ceil() as isize;
        if sx1_i < x0 as isize || sx0_i > x1 as isize || sy1_i < y0 as isize || sy0_i > y1 as isize {
            continue;
        }
        let sx0 = sx0_i.clamp(x0 as isize, x1 as isize) as usize;
        let sx1 = sx1_i.clamp(x0 as isize, x1 as isize) as usize;
        let sy0 = sy0_i.clamp(y0 as isize, y1 as isize) as usize;
        let sy1 = sy1_i.clamp(y0 as isize, y1 as isize) as usize;
        let vx = b[0] - a[0];
        let vy = b[1] - a[1];
        let len2 = vx * vx + vy * vy;
        for y in sy0..=sy1 {
            let py = y as f32 + 0.5;
            let row = (y - y0) * box_w;
            for x in sx0..=sx1 {
                if mask[row + (x - x0)] != 0 {
                    continue;
                }
                let px = x as f32 + 0.5;
                let t = if len2 < 1e-6 {
                    0.0
                } else {
                    (((px - a[0]) * vx + (py - a[1]) * vy) / len2).clamp(0.0, 1.0)
                };
                let dx = px - (a[0] + vx * t);
                let dy = py - (a[1] + vy * t);
                if dx * dx + dy * dy <= r2 {
                    mask[row + (x - x0)] = 1;
                }
            }
        }
    }
    for dy_i in 0..box_h {
        let y = y0 + dy_i;
        let mrow = dy_i * box_w;
        for dx_i in 0..box_w {
            if mask[mrow + dx_i] != 0 {
                let off = (y * ss + (x0 + dx_i)) * 4;
                fb[off] = sr + fb[off] * inv_a;
                fb[off + 1] = sg + fb[off + 1] * inv_a;
                fb[off + 2] = sb + fb[off + 2] * inv_a;
                fb[off + 3] = alpha + fb[off + 3] * inv_a;
            }
        }
    }
}

fn rounded_rect_pts(cx: f32, cy: f32, w: f32, h: f32, r: f32) -> Vec<[f32; 2]> {
    let hw = w * 0.5;
    let hh = h * 0.5;
    if r <= 0.5 {
        return vec![[cx - hw, cy - hh], [cx + hw, cy - hh], [cx + hw, cy + hh], [cx - hw, cy + hh]];
    }
    let mut pts = Vec::with_capacity(20);
    let corners = [
        (cx + hw - r, cy - hh + r, -std::f32::consts::FRAC_PI_2, 0.0),
        (cx + hw - r, cy + hh - r, 0.0, std::f32::consts::FRAC_PI_2),
        (cx - hw + r, cy + hh - r, std::f32::consts::FRAC_PI_2, std::f32::consts::PI),
        (cx - hw + r, cy - hh + r, std::f32::consts::PI, 3.0 * std::f32::consts::FRAC_PI_2),
    ];
    for (kx, ky, a0, a1) in corners {
        for s in 0..=4 {
            let a = a0 + (a1 - a0) * (s as f32 / 4.0);
            pts.push([kx + r * a.cos(), ky + r * a.sin()]);
        }
    }
    pts
}

fn ellipse_pts(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<[f32; 2]> {
    const N: usize = 24;
    (0..N)
        .map(|i| {
            let a = (i as f32 / N as f32) * std::f32::consts::TAU;
            [cx + rx * a.cos(), cy + ry * a.sin()]
        })
        .collect()
}

fn polystar_pts(star: bool, pts_f: f32, cx: f32, cy: f32, or: f32, ir: f32, rot_deg: f32) -> Vec<[f32; 2]> {
    if !pts_f.is_finite() || !cx.is_finite() || !cy.is_finite() || !or.is_finite() || !ir.is_finite() || !rot_deg.is_finite() {
        return Vec::new();
    }
    let n = (pts_f.round() as usize).clamp(3, 32);
    let base = rot_deg.to_radians() - std::f32::consts::FRAC_PI_2;
    let count = if star { n * 2 } else { n };
    (0..count)
        .map(|i| {
            let a = base + (i as f32 / count as f32) * std::f32::consts::TAU;
            let r = if star && i % 2 == 1 { ir } else { or };
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

fn eval_bezier_prop(prop: &AnimProp<BezierShape>, frame: f32) -> Option<BezierShape> {
    match prop {
        AnimProp::Static(s) => Some(s.clone()),
        AnimProp::Keyframed(kfs) => eval_keyframes(kfs, frame, |a, b, u| a.lerp(b, u)),
    }
}

fn parse_layers(arr: &[Value]) -> Vec<Layer> {
    arr.iter()
        .filter_map(|v| {
            let ty = v.get("ty")?.as_i64()?;
            let ind = v.get("ind").and_then(Value::as_i64).unwrap_or(-1);
            let parent = v.get("parent").and_then(Value::as_i64);
            let ip = v.get("ip").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let op = v.get("op").and_then(Value::as_f64).unwrap_or(60.0) as f32;
            let st = v.get("st").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let sr = v.get("sr").and_then(Value::as_f64).unwrap_or(1.0) as f32;
            let hidden = v.get("hd").and_then(Value::as_bool).unwrap_or(false);
            let transform = v.get("ks").map(parse_transform).unwrap_or_default();
            let kind = match ty {
                0 => LayerKind::Precomp {
                    ref_id: v.get("refId").and_then(Value::as_str).unwrap_or("").to_owned(),
                },
                1 => {
                    let color = parse_hex_color(v.get("sc").and_then(Value::as_str).unwrap_or("#000000"));
                    let w = v.get("sw").and_then(Value::as_f64).unwrap_or(512.0) as f32;
                    let h = v.get("sh").and_then(Value::as_f64).unwrap_or(512.0) as f32;
                    LayerKind::Solid { color, w, h }
                }
                3 => LayerKind::Null,
                4 => LayerKind::Shape {
                    items: v
                        .get("shapes")
                        .and_then(Value::as_array)
                        .map(|a| parse_shapes(a))
                        .unwrap_or_default(),
                },
                _ => return None,
            };
            Some(Layer { ind, parent, ip, op, st, sr, hidden, transform, kind })
        })
        .collect()
}

fn parse_shapes(arr: &[Value]) -> Vec<ShapeItem> {
    arr.iter()
        .filter_map(|v| {
            let ty = v.get("ty")?.as_str()?;
            if v.get("hd").and_then(Value::as_bool).unwrap_or(false) {
                return None;
            }
            match ty {
                "gr" => {
                    let it = v.get("it").and_then(Value::as_array)?;
                    let mut tr = Transform::default();
                    for sub in it {
                        if sub.get("ty").and_then(Value::as_str) == Some("tr") {
                            tr = parse_transform(sub);
                        }
                    }
                    Some(ShapeItem::Group { items: parse_shapes(it), transform: tr })
                }
                "sh" => {
                    let ks = v.get("ks")?;
                    Some(ShapeItem::Path { shape: parse_bezier_prop(ks) })
                }
                "rc" => Some(ShapeItem::Rect {
                    pos: parse_vec4_prop(v.get("p"), [0.0; 4]),
                    size: parse_vec4_prop(v.get("s"), [100.0, 100.0, 0.0, 0.0]),
                    radius: parse_f32_prop(v.get("r"), 0.0),
                }),
                "el" => Some(ShapeItem::Ellipse {
                    pos: parse_vec4_prop(v.get("p"), [0.0; 4]),
                    size: parse_vec4_prop(v.get("s"), [100.0, 100.0, 0.0, 0.0]),
                }),
                "sr" => Some(ShapeItem::Polystar {
                    star: v.get("sy").and_then(Value::as_i64).unwrap_or(1) == 1,
                    points: parse_f32_prop(v.get("pt"), 5.0),
                    pos: parse_vec4_prop(v.get("p"), [0.0; 4]),
                    outer_r: parse_f32_prop(v.get("or"), 50.0),
                    inner_r: parse_f32_prop(v.get("ir"), 25.0),
                    rot: parse_f32_prop(v.get("r"), 0.0),
                }),
                "tm" => Some(ShapeItem::Trim {
                    start: parse_f32_prop(v.get("s"), 0.0),
                    end: parse_f32_prop(v.get("e"), 100.0),
                    offset: parse_f32_prop(v.get("o"), 0.0),
                }),
                "fl" => Some(ShapeItem::Fill {
                    color: parse_color_prop(v.get("c"), [1.0, 1.0, 1.0, 1.0]),
                    opacity: parse_f32_prop(v.get("o"), 100.0),
                    even_odd: v.get("r").and_then(Value::as_i64).unwrap_or(1) == 2,
                }),
                "gf" => {
                    // Градиент аппроксимируется первым стоп-цветом (без падений).
                    let first = extract_gradient_color(v.get("g"));
                    Some(ShapeItem::Fill {
                        color: AnimProp::Static([first[0], first[1], first[2], 1.0]),
                        opacity: parse_f32_prop(v.get("o"), 100.0),
                        even_odd: v.get("r").and_then(Value::as_i64).unwrap_or(1) == 2,
                    })
                }
                "st" => Some(ShapeItem::Stroke {
                    color: parse_color_prop(v.get("c"), [1.0, 1.0, 1.0, 1.0]),
                    opacity: parse_f32_prop(v.get("o"), 100.0),
                    width: parse_f32_prop(v.get("w"), 2.0),
                }),
                _ => None,
            }
        })
        .collect()
}

fn parse_transform(ks: &Value) -> Transform {
    let p_obj = ks.get("p");
    let split = p_obj.and_then(|v| v.get("s")).and_then(Value::as_bool).unwrap_or(false);
    let (pos_x, pos_y, pos) = if split {
        (
            Some(parse_f32_prop(p_obj.and_then(|v| v.get("x")), 0.0)),
            Some(parse_f32_prop(p_obj.and_then(|v| v.get("y")), 0.0)),
            AnimProp::Static([0.0; 4]),
        )
    } else {
        (None, None, parse_vec4_prop(p_obj, [0.0; 4]))
    };
    Transform {
        anchor: parse_vec4_prop(ks.get("a"), [0.0; 4]),
        pos_x,
        pos_y,
        pos,
        scale: parse_vec4_prop(ks.get("s"), [100.0, 100.0, 100.0, 100.0]),
        rotation: parse_f32_prop(ks.get("r"), 0.0),
        opacity: parse_f32_prop(ks.get("o"), 100.0),
    }
}

fn first_f32(v: Option<&Value>, def: f32) -> f32 {
    let Some(v) = v else { return def };
    if let Some(n) = v.as_f64() {
        return n as f32;
    }
    if let Some(a) = v.as_array()
        && let Some(n) = a.first().and_then(Value::as_f64)
    {
        return n as f32;
    }
    def
}

fn parse_vec4_val(v: &Value, def: [f32; 4]) -> [f32; 4] {
    if let Some(n) = v.as_f64() {
        return [n as f32, def[1], def[2], def[3]];
    }
    let Some(a) = v.as_array() else { return def };
    let mut out = def;
    for (i, item) in a.iter().take(4).enumerate() {
        if let Some(n) = item.as_f64() {
            out[i] = n as f32;
        }
    }
    out
}

fn parse_f32_prop(prop: Option<&Value>, def: f32) -> AnimProp<f32> {
    let Some(v) = prop else { return AnimProp::Static(def) };
    let anim = v.get("a").and_then(Value::as_i64).unwrap_or(0) == 1;
    let Some(k) = v.get("k") else { return AnimProp::Static(def) };
    if !anim && !is_kf_array(k) {
        return AnimProp::Static(first_f32(Some(k), def));
    }
    let Some(arr) = k.as_array() else { return AnimProp::Static(first_f32(Some(k), def)) };
    let kfs = arr
        .iter()
        .filter_map(|kf| {
            let t = kf.get("t")?.as_f64()? as f32;
            let s = first_f32(kf.get("s"), def);
            let e = kf.get("e").map(|ev| first_f32(Some(ev), s));
            let (ox, oy, ix, iy) = parse_easing(kf);
            let hold = kf.get("h").and_then(Value::as_i64).unwrap_or(0) == 1;
            Some(Keyframe { t, s, e, ox, oy, ix, iy, hold })
        })
        .collect();
    AnimProp::Keyframed(kfs)
}

fn parse_vec4_prop(prop: Option<&Value>, def: [f32; 4]) -> AnimProp<[f32; 4]> {
    let Some(v) = prop else { return AnimProp::Static(def) };
    let anim = v.get("a").and_then(Value::as_i64).unwrap_or(0) == 1;
    let Some(k) = v.get("k") else { return AnimProp::Static(def) };
    if !anim && !is_kf_array(k) {
        return AnimProp::Static(parse_vec4_val(k, def));
    }
    let Some(arr) = k.as_array() else { return AnimProp::Static(parse_vec4_val(k, def)) };
    let kfs = arr
        .iter()
        .filter_map(|kf| {
            let t = kf.get("t")?.as_f64()? as f32;
            let s = kf.get("s").map_or(def, |sv| parse_vec4_val(sv, def));
            let e = kf.get("e").map(|ev| parse_vec4_val(ev, s));
            let (ox, oy, ix, iy) = parse_easing(kf);
            let hold = kf.get("h").and_then(Value::as_i64).unwrap_or(0) == 1;
            Some(Keyframe { t, s, e, ox, oy, ix, iy, hold })
        })
        .collect();
    AnimProp::Keyframed(kfs)
}

fn parse_color_prop(prop: Option<&Value>, def: [f32; 4]) -> AnimProp<[f32; 4]> {
    match parse_vec4_prop(prop, def) {
        AnimProp::Static(mut c) => {
            normalize_rgb(&mut c);
            AnimProp::Static(c)
        }
        AnimProp::Keyframed(mut kfs) => {
            for kf in &mut kfs {
                normalize_rgb(&mut kf.s);
                if let Some(e) = &mut kf.e {
                    normalize_rgb(e);
                }
            }
            AnimProp::Keyframed(kfs)
        }
    }
}

fn normalize_rgb(c: &mut [f32; 4]) {
    if c[0] > 1.0 || c[1] > 1.0 || c[2] > 1.0 {
        c[0] /= 255.0;
        c[1] /= 255.0;
        c[2] /= 255.0;
    }
}

fn parse_bezier_prop(ks: &Value) -> AnimProp<BezierShape> {
    let anim = ks.get("a").and_then(Value::as_i64).unwrap_or(0) == 1;
    let Some(k) = ks.get("k") else {
        return AnimProp::Static(BezierShape { vertices: vec![], in_pts: vec![], out_pts: vec![], closed: false });
    };
    if !anim && k.is_object() {
        return AnimProp::Static(parse_bezier_shape(k));
    }
    let Some(arr) = k.as_array() else {
        return AnimProp::Static(parse_bezier_shape(k));
    };
    let kfs = arr
        .iter()
        .filter_map(|kf| {
            let t = kf.get("t")?.as_f64()? as f32;
            let s_val = kf.get("s").and_then(|v| if v.is_array() { v.as_array()?.first() } else { Some(v) })?;
            let s = parse_bezier_shape(s_val);
            let e = kf
                .get("e")
                .and_then(|v| if v.is_array() { v.as_array()?.first() } else { Some(v) })
                .map(parse_bezier_shape);
            let (ox, oy, ix, iy) = parse_easing(kf);
            let hold = kf.get("h").and_then(Value::as_i64).unwrap_or(0) == 1;
            Some(Keyframe { t, s, e, ox, oy, ix, iy, hold })
        })
        .collect();
    AnimProp::Keyframed(kfs)
}

fn parse_bezier_shape(v: &Value) -> BezierShape {
    let pts = |k: &str| -> Vec<[f32; 2]> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let pa = p.as_array()?;
                        Some([pa.first()?.as_f64()? as f32, pa.get(1)?.as_f64()? as f32])
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    BezierShape {
        vertices: pts("v"),
        in_pts: pts("i"),
        out_pts: pts("o"),
        closed: v.get("c").and_then(Value::as_bool).unwrap_or(false),
    }
}

fn is_kf_array(k: &Value) -> bool {
    k.as_array().is_some_and(|a| a.first().is_some_and(|item| item.get("t").is_some()))
}

fn parse_easing(kf: &Value) -> (f32, f32, f32, f32) {
    let ox = first_f32(kf.get("o").and_then(|v| v.get("x")), 0.0);
    let oy = first_f32(kf.get("o").and_then(|v| v.get("y")), 0.0);
    let ix = first_f32(kf.get("i").and_then(|v| v.get("x")), 1.0);
    let iy = first_f32(kf.get("i").and_then(|v| v.get("y")), 1.0);
    (ox, oy, ix, iy)
}

fn parse_hex_color(hex: &str) -> [f32; 3] {
    let h = hex.trim().trim_start_matches('#');
    if h.len() >= 6
        && let Ok(v) = u32::from_str_radix(&h[..6], 16)
    {
        return [
            ((v >> 16) & 0xff) as f32 / 255.0,
            ((v >> 8) & 0xff) as f32 / 255.0,
            (v & 0xff) as f32 / 255.0,
        ];
    }
    [0.0, 0.0, 0.0]
}

fn extract_gradient_color(g: Option<&Value>) -> [f32; 3] {
    let Some(g) = g else { return [1.0, 1.0, 1.0] };
    let k = g.get("k").and_then(|v| v.get("k"));
    let Some(arr) = k.and_then(Value::as_array) else { return [1.0, 1.0, 1.0] };
    let vals: Vec<f32> = arr.iter().filter_map(Value::as_f64).map(|x| x as f32).collect();
    if vals.len() >= 4 {
        [vals[1], vals[2], vals[3]]
    } else {
        [1.0, 1.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    fn sample_lottie_json() -> &'static str {
        r#"{
          "v": "5.5.2",
          "fr": 60,
          "ip": 0,
          "op": 60,
          "w": 512,
          "h": 512,
          "layers": [
            {
              "ty": 4,
              "ind": 1,
              "ip": 0,
              "op": 60,
              "ks": {
                "a": {"a":0,"k":[0,0]},
                "p": {"a":0,"k":[256,256]},
                "s": {"a":1,"k":[{"t":0,"s":[50,50],"o":{"x":0.5,"y":0},"i":{"x":0.5,"y":1}},{"t":60,"s":[100,100]}]},
                "r": {"a":0,"k":0},
                "o": {"a":0,"k":100}
              },
              "shapes": [
                {
                  "ty": "gr",
                  "it": [
                    {"ty":"el","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[320,320]}},
                    {"ty":"fl","c":{"a":0,"k":[0.96,0.51,0.125,1]},"o":{"a":0,"k":100},"r":1},
                    {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}
                  ]
                }
              ]
            }
          ]
        }"#
    }

    #[test]
    fn decodes_gzip_tgs_and_renders_animated_frames() {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(sample_lottie_json().as_bytes()).unwrap();
        let gz = enc.finish().unwrap();

        let anim = TgsAnim::from_bytes(&gz).expect("TGS decoded");
        assert_eq!(((anim.op - anim.ip).round().max(1.0)) as usize, 60);
        let f0 = anim.render_frame(0.0, 48);
        let f59 = anim.render_frame(59.0, 48);
        assert_eq!(f0.size, [48, 48]);
        let non_empty = |img: &ColorImage| img.pixels.iter().filter(|c| c.a() > 0).count();
        assert!(non_empty(&f0) > 50);
        assert!(non_empty(&f59) > non_empty(&f0));
    }

    #[test]
    fn star_and_trim_stroke_render_without_panicking() {
        let json = r#"{
          "fr": 30, "ip": 0, "op": 30, "w": 100, "h": 100,
          "layers": [{
            "ty": 4, "ind": 1, "ip": 0, "op": 30,
            "shapes": [{
              "ty": "gr",
              "it": [
                {"ty":"sr","sy":1,"pt":{"a":0,"k":5},"p":{"a":0,"k":[50,50]},"or":{"a":0,"k":40},"ir":{"a":0,"k":18},"r":{"a":0,"k":0}},
                {"ty":"tm","s":{"a":0,"k":0},"e":{"a":0,"k":65},"o":{"a":0,"k":0}},
                {"ty":"st","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100},"w":{"a":0,"k":4}},
                {"ty":"tr"}
              ]
            }]
          }]
        }"#;
        let anim = TgsAnim::from_bytes(json.as_bytes()).unwrap();
        let img = anim.render_frame(10.0, 32);
        assert!(img.pixels.iter().any(|p| p.a() > 100));
    }

    #[test]
    fn stroke_contours_fast_bbox_renders_accurate_pixels() {
        let mut rgba = vec![0.0_f32; 64 * 64 * 4];
        let poly = vec![[8.0, 8.0], [56.0, 8.0], [56.0, 56.0], [8.0, 56.0]];
        let t0 = std::time::Instant::now();
        for _ in 0..20 {
            stroke_contours(&mut rgba, 64, &[(poly.clone(), true)], [1.0, 0.5, 0.1], 1.0, 3.0);
        }
        let elapsed = t0.elapsed();
        let lit = rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 0.5).count();
        assert!(lit > 300 && lit < 1800, "unexpected stroke pixel count: {lit}");
        assert!(elapsed.as_millis() < 250, "stroke_contours too slow: {elapsed:?}");
    }

    #[test]
    fn stroke_and_fill_offscreen_and_non_finite_never_panic() {
        let mut rgba = vec![0.0_f32; 64 * 64 * 4];
        let cases = [
            vec![[100.0, 100.0], [180.0, 180.0], [100.0, 180.0]],
            vec![[-100.0, -100.0], [-40.0, -20.0], [-80.0, -20.0]],
            vec![[10.0, 120.0], [30.0, 140.0], [50.0, 120.0]],
            vec![[120.0, 10.0], [140.0, 30.0], [120.0, 50.0]],
            vec![[f32::NAN, 10.0], [20.0, f32::INFINITY], [30.0, f32::NEG_INFINITY]],
        ];
        for poly in cases {
            stroke_contours(&mut rgba, 64, &[(poly.clone(), true)], [1.0, 0.5, 0.1], 1.0, 4.0);
            stroke_contours(&mut rgba, 64, &[(poly.clone(), false)], [1.0, 0.5, 0.1], 1.0, 1e9);
            fill_contours(&mut rgba, 64, &[(poly, true)], [1.0, 0.5, 0.1], 1.0, false);
        }
        stroke_contours(&mut [], 0, &[(vec![[1.0, 1.0], [5.0, 5.0]], false)], [1.0; 3], 1.0, 2.0);
        fill_contours(&mut [], 0, &[(vec![[1.0, 1.0], [5.0, 5.0], [1.0, 5.0]], true)], [1.0; 3], 1.0, false);
        stroke_contours(&mut rgba, 64, &[(vec![[5.0, 5.0], [10.0, 10.0]], false)], [1.0; 3], f32::NAN, f32::INFINITY);
    }

    #[test]
    #[ignore]
    fn renders_local_cached_tgs_if_present() {
        let Some(appdata) = std::env::var_os("APPDATA") else {
            return;
        };
        let dir = std::path::PathBuf::from(appdata).join("Rustgramm/accounts/slot_1/cache/tgs");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut count = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tgs")
                && let Ok(bytes) = std::fs::read(&path)
                && let Some(anim) = TgsAnim::from_bytes(&bytes)
            {
                let _ = anim.render_frame(anim.ip, 256);
                let _ = anim.render_frame((anim.ip + anim.op) * 0.5, 256);
                count += 1;
            }
        }
        assert!(count > 0, "no tgs files rendered");
    }
}
