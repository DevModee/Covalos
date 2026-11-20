//! AEROS Motion: one vocabulary of durations and damped springs. Decorative motion never
//! gates input or owns keyboard focus, every value is interruptible and retargets from its
//! current position and velocity, and the user's intensity setting (0 = instant) and
//! Reduced Motion are honoured everywhere.
//!
//! Springs follow the stiffness/damping model used by Motion (the engine behind Sora UI,
//! Tweenly and ApexUI) but are tuned close to critical damping: interface elements arrive
//! without visible overshoot, as in desktop IDEs, instead of the bouncy web defaults.
use eframe::egui::{self, Color32, Context, Id, Ui};
pub(super) const HOVER: f32 = 0.12;
pub(super) const CONTROL: f32 = 0.16;
pub(super) const PANEL: f32 = 0.24;
pub(super) const VIEW: f32 = 0.28;

thread_local! {
    static INTENSITY: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Global motion intensity in `0..=1`; installed once per frame from preferences.
pub(super) fn set_intensity(value: f32) {
    INTENSITY.with(|i| i.set(value.clamp(0.0, 1.0)));
}

pub(super) fn intensity() -> f32 {
    INTENSITY.with(|i| i.get())
}

pub(super) fn duration(ctx: &Context, value: f32) -> f32 {
    if ctx.style().animation_time > 0.0 {
        value * intensity()
    } else {
        0.0
    }
}

/// Spring presets. `stiffness` in 1/s², `damping` as a ratio of critical damping.
#[derive(Clone, Copy, Debug)]
pub(super) struct Spring {
    pub(super) stiffness: f32,
    pub(super) damping: f32,
}

impl Spring {
    /// Hover, press and small indicators: settles in about 120 ms.
    pub(super) const SNAPPY: Self = Self {
        stiffness: 900.0,
        damping: 1.0,
    };
    /// Tabs, selection pills and panel resizes: about 220 ms, no overshoot.
    pub(super) const SMOOTH: Self = Self {
        stiffness: 380.0,
        damping: 0.92,
    };
    /// Floating surfaces and the Companion island: about 320 ms with a 1-2 % settle.
    pub(super) const GENTLE: Self = Self {
        stiffness: 210.0,
        damping: 0.86,
    };
}

#[derive(Clone, Copy)]
struct SpringState {
    value: f32,
    velocity: f32,
    time: f64,
}

/// Advance a damped spring toward `target` and return its current value. The first call
/// starts at the target so nothing animates on first appearance.
pub(super) fn spring(ctx: &Context, id: Id, target: f32, spring: Spring) -> f32 {
    let now = ctx.input(|i| i.time);
    let scale = intensity();
    let instant = ctx.style().animation_time <= 0.0 || scale < 0.05;
    let mut state = ctx
        .data(|d| d.get_temp::<SpringState>(id))
        .unwrap_or(SpringState {
            value: target,
            velocity: 0.0,
            time: now,
        });
    if instant || !target.is_finite() {
        state = SpringState {
            value: target,
            velocity: 0.0,
            time: now,
        };
    } else {
        // Lower intensity shortens the motion instead of removing it.
        let k = spring.stiffness / (scale * scale);
        let c = 2.0 * spring.damping * k.sqrt();
        let mut remaining = ((now - state.time) as f32).clamp(0.0, 0.1);
        while remaining > 0.0 {
            let dt = remaining.min(1.0 / 240.0);
            let force = -k * (state.value - target) - c * state.velocity;
            state.velocity += force * dt;
            state.value += state.velocity * dt;
            remaining -= dt;
        }
        state.time = now;
        let scale_of = target.abs().max(1.0);
        if (state.value - target).abs() < 0.0005 * scale_of
            && state.velocity.abs() < 0.01 * scale_of
        {
            state.value = target;
            state.velocity = 0.0;
        } else {
            ctx.request_repaint();
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, state));
    state.value
}

/// Spring between two rectangles (tab indicators, selection pills, docking previews).
pub(super) fn spring_rect(ctx: &Context, id: Id, target: egui::Rect, spring: Spring) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            self::spring(ctx, id.with(0), target.min.x, spring),
            self::spring(ctx, id.with(1), target.min.y, spring),
        ),
        egui::pos2(
            self::spring(ctx, id.with(2), target.max.x, spring),
            self::spring(ctx, id.with(3), target.max.y, spring),
        ),
    )
}
pub(super) fn value(ctx: &Context, id: Id, target: f32, seconds: f32) -> f32 {
    ctx.animate_value_with_time(id, target, duration(ctx, seconds))
}
pub(super) fn state(ui: &Ui, id: Id, active: bool, seconds: f32) -> f32 {
    let t = ui
        .ctx()
        .animate_bool_with_time(id, active, duration(ui.ctx(), seconds));
    egui::emath::easing::cubic_out(t)
}
pub(super) fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    egui::lerp(egui::Rgba::from(a)..=egui::Rgba::from(b), t).into()
}
pub(super) fn enter(ui: &mut Ui, id: Id, seconds: f32) -> f32 {
    let duration = duration(ui.ctx(), seconds);
    if duration <= 0.0 {
        return 1.0;
    }
    let frame = ui.ctx().cumulative_frame_nr();
    let now = ui.input(|i| i.time);
    let start = ui.ctx().data_mut(|d| {
        let (last, start) = d.get_temp::<(u64, f64)>(id).unwrap_or((frame, now));
        let start = if frame > last + 1 { now } else { start };
        d.insert_temp(id, (frame, start));
        start
    });
    let t = egui::emath::easing::cubic_out(
        ((now - start) / f64::from(duration)).clamp(0.0, 1.0) as f32
    );
    ui.multiply_opacity(t);
    if t < 1.0 {
        ui.ctx().request_repaint();
    }
    t
}

#[cfg(test)]
pub(super) fn record(ui: &Ui, label: impl Into<String>, rect: egui::Rect) {
    ui.ctx().data_mut(|d| {
        let key = Id::new("native-review-targets");
        let mut targets = d
            .get_temp::<std::collections::HashMap<String, egui::Rect>>(key)
            .unwrap_or_default();
        targets.insert(label.into(), rect);
        d.insert_temp(key, targets);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ctx: &Context, time: f64, target: f32) -> f32 {
        let mut out = 0.0;
        let _ = ctx.run(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            |ctx| {
                ctx.style_mut(|s| s.animation_time = 0.12);
                out = spring(ctx, Id::new("probe"), target, Spring::SMOOTH);
            },
        );
        out
    }

    #[test]
    fn springs_settle_without_large_overshoot_and_respect_intensity() {
        set_intensity(1.0);
        let ctx = Context::default();
        assert_eq!(run(&ctx, 0.0, 0.0), 0.0);
        let mut peak: f32 = 0.0;
        let mut t = 0.0;
        while t < 0.6 {
            t += 1.0 / 60.0;
            peak = peak.max(run(&ctx, t, 100.0));
        }
        assert!(peak <= 101.5, "overshoot {peak}");
        assert!((run(&ctx, 0.7, 100.0) - 100.0).abs() < 0.1);
        set_intensity(0.0);
        assert_eq!(run(&ctx, 0.72, 10.0), 10.0);
        set_intensity(1.0);
    }
}
