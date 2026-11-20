//! Dither Field: the AEROS home background, rendered on the GPU.
//!
//! Art direction verified against the owner's reference video (x.com/sarthakguptadev,
//! status 2122214682924900813) and React Bits "Pixel Blast", which the reference reproduces:
//! a field of tiny square pixels on a fixed grid, ordered (Bayer 8x8) dithering of slowly
//! drifting value-noise clouds in a neutral ink, a cursor brush that lights the pixels it
//! passes in the accent color and fades, and click ripples that travel outward as dithered
//! rings. The second reference (x.com/praveenisomer, status 2123061164276101240) adds the
//! "Bloom" variant: one large, soft, slowly breathing form rendered with the same ordered
//! dither in a single tint.
//!
//! This is an original GLSL implementation for egui's glow backend. It does not include
//! React Bits source code. Work per frame on the CPU is a handful of uniforms; each pixel
//! is evaluated once on the GPU inside the background rectangle only.
use eframe::{egui, egui_glow, glow};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const TRAIL: usize = 16;
pub(super) const RIPPLES: usize = 6;
pub(super) const TRAIL_SECONDS: f64 = 0.9;
pub(super) const RIPPLE_SECONDS: f64 = 2.4;

/// Set when the GPU path cannot run (GLSL 1.20 / ES 2 contexts or a compile failure);
/// callers then fall back to the CPU dot grid.
static UNAVAILABLE: AtomicBool = AtomicBool::new(false);

pub(super) fn available() -> bool {
    !UNAVAILABLE.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Variant {
    Field,
    Bloom,
}

/// Everything the shader needs for one frame, in points relative to the background rect.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub(super) variant: Variant,
    pub(super) time: f32,
    pub(super) ink: egui::Rgba,
    pub(super) accent: egui::Rgba,
    pub(super) strength: f32,
    pub(super) density: f32,
    pub(super) pixel: f32,
    pub(super) light: bool,
    /// (x, y, age 0..1) per trail sample; age >= 1 means unused.
    pub(super) trail: [[f32; 3]; TRAIL],
    /// (x, y, seconds since click); negative seconds means unused.
    pub(super) ripples: [[f32; 3]; RIPPLES],
}

/// Pointer history kept on the UI thread.
#[derive(Default)]
pub(super) struct Input {
    trail: std::collections::VecDeque<(egui::Pos2, f64)>,
    ripples: std::collections::VecDeque<(egui::Pos2, f64)>,
}

impl Input {
    pub(super) fn record(&mut self, now: f64, pointer: Option<egui::Pos2>, click: bool) {
        if let Some(p) = pointer
            && self
                .trail
                .back()
                .is_none_or(|(last, _)| last.distance(p) > 3.0)
        {
            self.trail.push_back((p, now));
        }
        if click && let Some(p) = pointer {
            self.ripples.push_back((p, now));
        }
        while self.trail.len() > TRAIL
            || self
                .trail
                .front()
                .is_some_and(|(_, t)| now - t > TRAIL_SECONDS)
        {
            self.trail.pop_front();
        }
        while self.ripples.len() > RIPPLES
            || self
                .ripples
                .front()
                .is_some_and(|(_, t)| now - t > RIPPLE_SECONDS)
        {
            self.ripples.pop_front();
        }
    }

    pub(super) fn clear(&mut self) {
        self.trail.clear();
        self.ripples.clear();
    }

    pub(super) fn busy(&self) -> bool {
        !self.trail.is_empty() || !self.ripples.is_empty()
    }

    pub(super) fn fill(&self, frame: &mut Frame, rect: egui::Rect, now: f64) {
        frame.trail = [[0.0, 0.0, 1.0]; TRAIL];
        frame.ripples = [[0.0, 0.0, -1.0]; RIPPLES];
        for (slot, (p, t)) in frame.trail.iter_mut().zip(self.trail.iter().rev()) {
            *slot = [
                p.x - rect.left(),
                p.y - rect.top(),
                ((now - t) / TRAIL_SECONDS) as f32,
            ];
        }
        for (slot, (p, t)) in frame.ripples.iter_mut().zip(self.ripples.iter().rev()) {
            *slot = [p.x - rect.left(), p.y - rect.top(), (now - t) as f32];
        }
    }
}

pub(super) fn paint(painter: &egui::Painter, rect: egui::Rect, frame: Frame) {
    let callback = egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, glow_painter| {
            draw(info, glow_painter.gl(), &frame);
        })),
    };
    painter.add(callback);
}

struct Gpu {
    program: glow::Program,
    vao: glow::VertexArray,
}

thread_local! {
    static GPU: RefCell<Option<Gpu>> = const { RefCell::new(None) };
}

fn draw(info: egui::PaintCallbackInfo, gl: &Arc<glow::Context>, frame: &Frame) {
    use glow::HasContext as _;
    if !available() {
        return;
    }
    GPU.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            match unsafe { compile(gl) } {
                Ok(gpu) => *slot = Some(gpu),
                Err(error) => {
                    eprintln!("AEROS Dither Field unavailable: {error}");
                    UNAVAILABLE.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }
        let Some(gpu) = slot.as_ref() else { return };
        let viewport = info.viewport_in_pixels();
        let ppp = info.pixels_per_point;
        unsafe {
            gl.use_program(Some(gpu.program));
            let u = |name: &str| gl.get_uniform_location(gpu.program, name);
            gl.uniform_2_f32(
                u("uOrigin").as_ref(),
                viewport.left_px as f32,
                viewport.from_bottom_px as f32,
            );
            gl.uniform_2_f32(
                u("uSize").as_ref(),
                viewport.width_px as f32,
                viewport.height_px as f32,
            );
            gl.uniform_1_f32(u("uScale").as_ref(), ppp);
            gl.uniform_1_f32(u("uTime").as_ref(), frame.time);
            gl.uniform_1_f32(u("uPixel").as_ref(), (frame.pixel * ppp).round().max(2.0));
            gl.uniform_1_f32(u("uStrength").as_ref(), frame.strength);
            gl.uniform_1_f32(u("uDensity").as_ref(), frame.density);
            gl.uniform_1_f32(
                u("uVariant").as_ref(),
                if frame.variant == Variant::Bloom {
                    1.0
                } else {
                    0.0
                },
            );
            gl.uniform_1_f32(u("uLight").as_ref(), if frame.light { 1.0 } else { 0.0 });
            gl.uniform_3_f32(
                u("uInk").as_ref(),
                frame.ink.r(),
                frame.ink.g(),
                frame.ink.b(),
            );
            gl.uniform_3_f32(
                u("uAccent").as_ref(),
                frame.accent.r(),
                frame.accent.g(),
                frame.accent.b(),
            );
            let trail: Vec<f32> = frame.trail.iter().flatten().copied().collect();
            let ripples: Vec<f32> = frame.ripples.iter().flatten().copied().collect();
            gl.uniform_3_f32_slice(u("uTrail[0]").as_ref(), &trail);
            gl.uniform_3_f32_slice(u("uRipples[0]").as_ref(), &ripples);
            gl.bind_vertex_array(Some(gpu.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    });
}

unsafe fn compile(gl: &glow::Context) -> Result<Gpu, String> {
    use glow::HasContext as _;
    let version = egui_glow::ShaderVersion::get(gl);
    if !version.is_new_shader_interface() {
        return Err(format!("{version:?} has no gl_VertexID"));
    }
    let header = format!(
        "{}{}",
        version.version_declaration(),
        if matches!(version, egui_glow::ShaderVersion::Es300) {
            "precision highp float;\n"
        } else {
            ""
        }
    );
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, body) in [
            (glow::VERTEX_SHADER, VERTEX),
            (glow::FRAGMENT_SHADER, FRAGMENT),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &format!("{header}{body}"));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        let vao = gl.create_vertex_array()?;
        Ok(Gpu { program, vao })
    }
}

const VERTEX: &str = r#"
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"
uniform vec2 uOrigin;
uniform vec2 uSize;
uniform float uScale;
uniform float uTime;
uniform float uPixel;
uniform float uStrength;
uniform float uDensity;
uniform float uVariant;
uniform float uLight;
uniform vec3 uInk;
uniform vec3 uAccent;
uniform vec3 uTrail[16];
uniform vec3 uRipples[6];
out vec4 fragColor;

// Ordered dithering thresholds, built recursively from the 2x2 Bayer matrix.
float bayer2(vec2 a) { a = floor(a); return fract(a.x * 0.5 + a.y * a.y * 0.75); }
float bayer4(vec2 a) { return bayer2(0.5 * a) * 0.25 + bayer2(a); }
float bayer8(vec2 a) { return bayer4(0.5 * a) * 0.25 + bayer2(a); }

float hash(vec3 p) {
    p = fract(p * 0.1031);
    p += dot(p, p.zyx + 31.32);
    return fract((p.x + p.y) * p.z);
}

float noise(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    vec3 w = f * f * (3.0 - 2.0 * f);
    float a = mix(mix(hash(i), hash(i + vec3(1, 0, 0)), w.x),
                  mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), w.x), w.y);
    float b = mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), w.x),
                  mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), w.x), w.y);
    return mix(a, b, w.z);
}

float clouds(vec2 uv, float t) {
    vec3 p = vec3(uv, t);
    float sum = 0.0;
    float amp = 0.55;
    for (int i = 0; i < 4; i++) {
        sum += amp * noise(p);
        p = vec3(p.xy * 2.03 + vec2(17.1, 9.3), p.z * 1.21);
        amp *= 0.5;
    }
    return sum;
}

void main() {
    // Pixel coordinates inside the background rect, origin at its top-left like egui.
    vec2 px = gl_FragCoord.xy - uOrigin;
    px.y = uSize.y - px.y;
    vec2 cell = floor(px / uPixel);
    vec2 local = fract(px / uPixel);
    vec2 center = (cell + 0.5) * uPixel / uScale;   // in points
    float aspect = uSize.y / uScale;
    vec2 uv = center / aspect;

    float field;
    if (uVariant < 0.5) {
        vec2 drift = vec2(uTime * 0.035, -uTime * 0.018);
        field = clouds(uv * 2.4 + drift, uTime * 0.05);
        field = smoothstep(0.30, 0.78, field) * (0.62 + uDensity * 0.6) - 0.08;
    } else {
        // Bloom: a large soft form breathing in from the lower right.
        vec2 c = vec2(uSize.x / uScale * 0.72, uSize.y / uScale * 0.62) / aspect;
        vec2 d = uv - c;
        float a = atan(d.y, d.x);
        float petals = 0.5 + 0.5 * sin(a * 5.0 + uTime * 0.25 + clouds(uv * 1.5, uTime * 0.04) * 3.0);
        float r = length(d) / (0.42 + 0.08 * petals + 0.03 * sin(uTime * 0.4));
        field = (1.0 - smoothstep(0.15, 1.0, r)) * (0.55 + 0.45 * petals);
        field *= 0.75 + uDensity * 0.5;
    }

    // Cursor brush: recent pointer samples light the pixels they pass, then fade.
    float heat = 0.0;
    for (int i = 0; i < 16; i++) {
        vec3 s = uTrail[i];
        if (s.z >= 1.0) continue;
        float d = distance(center, s.xy) / 70.0;
        float life = 1.0 - s.z;
        heat = max(heat, exp(-d * d * 2.2) * life * life);
    }
    // Click ripples: dithered rings that travel outward and dissolve.
    float ring = 0.0;
    for (int i = 0; i < 6; i++) {
        vec3 c = uRipples[i];
        if (c.z < 0.0) continue;
        float radius = c.z * 260.0;
        float r = distance(center, c.xy);
        float band = (r - radius) / (26.0 + c.z * 30.0);
        ring = max(ring, exp(-band * band) * exp(-c.z * 1.25));
        // A short-lived solid blot at the click point.
        ring = max(ring, (1.0 - smoothstep(0.0, 40.0 + c.z * 30.0, r)) * exp(-c.z * 3.5));
    }
    float lit = clamp(max(heat, ring), 0.0, 1.0);
    float value = clamp(field + lit * 0.95, 0.0, 1.2);

    float threshold = bayer8(cell);
    float on = step(threshold, value);
    // Square pixels with a visible gutter, like the reference.
    vec2 q = abs(local - 0.5);
    float dotMask = step(max(q.x, q.y), 0.30);

    // Soft vignette so the field never collides with the window edges.
    vec2 n = (gl_FragCoord.xy - uOrigin) / uSize;
    vec2 edge = smoothstep(vec2(0.0), vec2(0.08), n) * smoothstep(vec2(0.0), vec2(0.08), 1.0 - n);
    float fade = edge.x * edge.y;

    float tone = smoothstep(0.0, 0.85, lit);
    vec3 color = mix(uInk, uAccent, tone);
    float alpha = on * dotMask * fade * mix(0.30 + 0.22 * uLight, 0.95, tone) * uStrength;
    fragColor = vec4(color * alpha, alpha);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_history_is_bounded_and_expires() {
        let mut input = Input::default();
        for i in 0..40 {
            input.record(
                f64::from(i) * 0.01,
                Some(egui::pos2(i as f32 * 10.0, 0.0)),
                i % 5 == 0,
            );
        }
        assert!(input.trail.len() <= TRAIL);
        assert!(input.ripples.len() <= RIPPLES);
        input.record(10.0, None, false);
        assert!(!input.busy());
        let mut frame = Frame {
            variant: Variant::Field,
            time: 0.0,
            ink: egui::Rgba::WHITE,
            accent: egui::Rgba::WHITE,
            strength: 1.0,
            density: 0.5,
            pixel: 4.0,
            light: false,
            trail: [[0.0; 3]; TRAIL],
            ripples: [[0.0; 3]; RIPPLES],
        };
        input.record(10.0, Some(egui::pos2(50.0, 60.0)), true);
        input.fill(
            &mut frame,
            egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 100.0)),
            10.2,
        );
        assert_eq!(frame.trail[0][0], 40.0);
        assert!(frame.trail[1][2] >= 1.0);
        assert!((frame.ripples[0][2] - 0.2).abs() < 1e-4);
        assert!(frame.ripples[1][2] < 0.0);
    }
}
