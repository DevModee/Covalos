//! Product identity registry. Names remain neutral while logo redistribution permission is unresolved.
use crate::assistant::Provider;
pub(super) struct Brand {
    pub(super) name: &'static str,
    pub(super) owner: &'static str,
    pub(super) _source: &'static str,
    pub(super) _restriction: &'static str,
}
pub(super) fn get(provider: Provider) -> Brand {
    match provider {
        Provider::Codex => Brand {
            name: "Codex",
            owner: "OpenAI",
            _source: "https://openai.com/brand/",
            _restriction: "Product logo permission pending; do not substitute the OpenAI Blossom or a generic robot.",
        },
        Provider::Claude => Brand {
            name: "Claude Code",
            owner: "Anthropic",
            _source: "https://www.anthropic.com/press-kit",
            _restriction: "Anthropic trademark guidelines require prior approval for supplied marks. Neutral product name until approved.",
        },
        Provider::Gemini => Brand {
            name: "Gemini CLI",
            owner: "Google",
            _source: "https://github.com/google-gemini/gemini-cli",
            _restriction: "Product icon usage review pending. No invented sparkle or Antigravity substitution.",
        },
        Provider::Antigravity => Brand {
            name: "Antigravity",
            owner: "Google",
            _source: "https://antigravity.google/press",
            _restriction: "Official press assets located; third-party product icon usage review pending.",
        },
    }
}
pub(super) fn title(ui: &mut eframe::egui::Ui, provider: Provider) {
    let b = get(provider);
    ui.label(
        eframe::egui::RichText::new(b.name)
            .size(super::theme::LEAD)
            .strong(),
    )
    .on_hover_text(format!("{} · {}", b.owner, b.name));
}
