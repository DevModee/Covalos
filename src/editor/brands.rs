//! Provider identity. Official marks are embedded as monochrome masks and shown only to
//! identify the matching integration, never larger than the AEROS identity.
//!
//! Sources and terms are recorded in `assets/brands/PROVENANCE.md`.
use super::theme::colors;
use crate::assistant::Provider;
use eframe::egui::{self, Color32, Painter, Rect, Stroke, vec2};

pub(super) struct Brand {
    pub(super) name: &'static str,
    pub(super) owner: &'static str,
    /// Embedded official mark, if its use for identification has been verified.
    pub(super) mark: Option<&'static str>,
    /// Brand colour published with the mark.
    pub(super) color: Color32,
}

pub(super) fn get(provider: Provider) -> Brand {
    match provider {
        // Official monochrome Blossom from openai.com/brand (terms accepted by the owner on
        // 2026-11-19). OpenAI forbids added colour, so it is always drawn in the text tone.
        Provider::Codex => Brand {
            name: "Codex",
            owner: "OpenAI",
            mark: Some("brand_openai"),
            color: Color32::from_rgb(236, 236, 241),
        },
        Provider::Claude => Brand {
            name: "Claude Code",
            owner: "Anthropic",
            mark: Some("brand_claude"),
            color: Color32::from_rgb(0xD9, 0x77, 0x57),
        },
        Provider::Gemini => Brand {
            name: "Gemini CLI",
            owner: "Google",
            mark: Some("brand_gemini"),
            color: Color32::from_rgb(0x8E, 0x75, 0xB2),
        },
        Provider::Antigravity => Brand {
            name: "Antigravity",
            owner: "Google",
            mark: Some("brand_antigravity"),
            color: Color32::from_rgb(0x6A, 0x9B, 0xF5),
        },
    }
}

/// Paint the provider mark centred in `rect`.
pub(super) fn mark(painter: &Painter, rect: Rect, provider: Provider) {
    let brand = get(provider);
    let side = rect.width().min(rect.height());
    match brand.mark {
        Some(name) => {
            let tint = if matches!(provider, Provider::Antigravity | Provider::Codex) {
                colors().text
            } else {
                brand.color
            };
            super::assets::tinted(
                painter,
                Rect::from_center_size(rect.center(), vec2(side, side)),
                name,
                tint,
            );
        }
        None => {
            // Neutral terminal monogram: a prompt chevron and a cursor bar.
            let c = rect.center();
            let k = side * 0.22;
            let stroke = Stroke::new((side * 0.11).max(1.2), colors().text);
            painter.add(egui::Shape::line(
                vec![
                    c + vec2(-k * 1.45, -k),
                    c + vec2(-k * 0.35, 0.0),
                    c + vec2(-k * 1.45, k),
                ],
                stroke,
            ));
            painter.line_segment([c + vec2(k * 0.2, k), c + vec2(k * 1.5, k)], stroke);
        }
    }
}

/// Rounded avatar tile with the provider mark. `active` adds a live ring.
pub(super) fn avatar(ui: &egui::Ui, rect: Rect, provider: Provider, active: bool) {
    let painter = ui.painter();
    let brand = get(provider);
    let radius = (rect.width() * 0.28) as u8;
    painter.rect_filled(rect, radius, colors().elevated);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, brand.color.gamma_multiply(0.28)),
        egui::StrokeKind::Inside,
    );
    if active {
        let t = ui.input(|i| i.time) as f32;
        let pulse = if ui.style().animation_time > 0.0 {
            (t * 3.0).sin() * 0.5 + 0.5
        } else {
            1.0
        };
        painter.rect_stroke(
            rect.expand(2.0 + pulse * 1.5),
            radius + 3,
            Stroke::new(
                1.0_f32,
                colors().accent.gamma_multiply(0.35 + 0.4 * (1.0 - pulse)),
            ),
            egui::StrokeKind::Outside,
        );
    }
    mark(painter, rect.shrink(rect.width() * 0.22), provider);
}
