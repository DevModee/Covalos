//! One motion vocabulary. Decorative motion never gates input or owns keyboard focus.
use eframe::egui::{self, Color32, Context, Id, Ui};
pub(super) const HOVER: f32 = 0.12;
pub(super) const CONTROL: f32 = 0.16;
pub(super) const PANEL: f32 = 0.24;
pub(super) const VIEW: f32 = 0.28;
pub(super) fn duration(ctx: &Context, value: f32) -> f32 {
    if ctx.style().animation_time > 0.0 {
        value
    } else {
        0.0
    }
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
