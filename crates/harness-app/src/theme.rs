//! Themes: the palette, how it is applied to egui, and the Aura mark.
//!
//! Four themes ship: **Nature** (the current Aura identity — forest, cream,
//! gold), **Midnight** (cool counterpart), **Retro** (the ∞K edition, kept as
//! an opt-in), and **Paper** (light). Every panel reads its colours through
//! the accessors below, so switching a theme repaints the whole app.

use egui::{Color32, Context, Rounding, Stroke, Visuals};
use harness_core::config::ThemeChoice;
use std::sync::RwLock;

/// A complete colour set for one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub field: Color32,
    pub bubble: Color32,
    pub text: Color32,
    pub dim: Color32,
    /// Accent: buttons, highlights, links, the brand mark.
    pub accent: Color32,
    /// Structure: borders and secondary fills.
    pub structure: Color32,
    /// Text drawn *on* the accent.
    pub on_accent: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub ok: Color32,
    pub info: Color32,
    /// The four stripes of the brand mark.
    pub stripes: [Color32; 4],
    pub dark: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Nature is the default, so it must be constructible in a `static`.
const NATURE: Palette = Palette {
    bg: rgb(0x07, 0x09, 0x07),
    panel: rgb(0x10, 0x13, 0x0e),
    card: rgb(0x17, 0x1b, 0x13),
    field: rgb(0x0b, 0x0d, 0x0a),
    bubble: rgb(0x1c, 0x21, 0x16),
    text: rgb(0xf2, 0xeb, 0xc9),
    dim: rgb(0x93, 0x9a, 0x7c),
    accent: rgb(0xe7, 0xcf, 0x85),
    structure: rgb(0x53, 0x5c, 0x37),
    on_accent: rgb(0x07, 0x09, 0x07),
    danger: rgb(0xE4, 0x31, 0x2B),
    warn: rgb(0xF8, 0xB9, 0x1E),
    ok: rgb(0x2F, 0xAE, 0x4E),
    info: rgb(0x1A, 0xA6, 0xE0),
    stripes: [rgb(0xE4, 0x31, 0x2B), rgb(0xF8, 0xB9, 0x1E), rgb(0x2F, 0xAE, 0x4E), rgb(0x1A, 0xA6, 0xE0)],
    dark: true,
};

const MIDNIGHT: Palette = Palette {
    bg: rgb(0x05, 0x07, 0x0c),
    panel: rgb(0x0b, 0x0f, 0x18),
    card: rgb(0x12, 0x18, 0x24),
    field: rgb(0x08, 0x0b, 0x12),
    bubble: rgb(0x16, 0x1e, 0x2e),
    text: rgb(0xdf, 0xe6, 0xf2),
    dim: rgb(0x7d, 0x8b, 0xa3),
    accent: rgb(0x8f, 0xb8, 0xff),
    structure: rgb(0x2c, 0x3a, 0x54),
    on_accent: rgb(0x05, 0x07, 0x0c),
    danger: rgb(0xf2, 0x5d, 0x5d),
    warn: rgb(0xf5, 0xc2, 0x4c),
    ok: rgb(0x4d, 0xcf, 0x8a),
    info: rgb(0x5c, 0xc8, 0xf0),
    stripes: [rgb(0xf2, 0x5d, 0x5d), rgb(0xf5, 0xc2, 0x4c), rgb(0x4d, 0xcf, 0x8a), rgb(0x5c, 0xc8, 0xf0)],
    dark: true,
};

/// The retired ∞K identity, kept because it was asked for once.
const RETRO: Palette = Palette {
    bg: rgb(0x0b, 0x0b, 0x10),
    panel: rgb(0x14, 0x14, 0x1e),
    card: rgb(0x1c, 0x1c, 0x2a),
    field: rgb(0x10, 0x10, 0x18),
    bubble: rgb(0x24, 0x24, 0x36),
    text: rgb(0xf2, 0xf2, 0xf2),
    dim: rgb(0x8a, 0x8a, 0xa0),
    accent: rgb(0xff, 0x4f, 0xd8),
    structure: rgb(0x3a, 0x3a, 0x54),
    on_accent: rgb(0x0b, 0x0b, 0x10),
    danger: rgb(0xff, 0x4f, 0x6a),
    warn: rgb(0xff, 0xd3, 0x4f),
    ok: rgb(0x4f, 0xff, 0xa8),
    info: rgb(0x4f, 0xd8, 0xff),
    stripes: [rgb(0xff, 0x4f, 0xd8), rgb(0xff, 0xd3, 0x4f), rgb(0x4f, 0xff, 0xa8), rgb(0x4f, 0xd8, 0xff)],
    dark: true,
};

const PAPER: Palette = Palette {
    bg: rgb(0xf4, 0xef, 0xdd),
    panel: rgb(0xec, 0xe6, 0xd0),
    card: rgb(0xe3, 0xdc, 0xc4),
    field: rgb(0xfa, 0xf7, 0xec),
    bubble: rgb(0xdd, 0xd5, 0xba),
    text: rgb(0x1d, 0x1c, 0x14),
    dim: rgb(0x6b, 0x67, 0x52),
    accent: rgb(0x8a, 0x67, 0x14),
    structure: rgb(0xb9, 0xb0, 0x93),
    on_accent: rgb(0xfa, 0xf7, 0xec),
    danger: rgb(0xa8, 0x1f, 0x1b),
    warn: rgb(0x9a, 0x6a, 0x06),
    ok: rgb(0x1f, 0x6b, 0x36),
    info: rgb(0x14, 0x5f, 0x86),
    stripes: [rgb(0xd0, 0x2b, 0x26), rgb(0xe0, 0xa1, 0x14), rgb(0x25, 0x95, 0x43), rgb(0x15, 0x8f, 0xc4)],
    dark: false,
};

pub fn palette_for(choice: ThemeChoice) -> Palette {
    match choice {
        ThemeChoice::Nature => NATURE,
        ThemeChoice::Midnight => MIDNIGHT,
        ThemeChoice::Retro => RETRO,
        ThemeChoice::Paper => PAPER,
    }
}

/// The palette in force. Panels read it through the accessors below rather
/// than holding colours of their own, so a theme switch is a single write.
static CURRENT: RwLock<Palette> = RwLock::new(NATURE);

fn current() -> Palette {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

// ---- accessors used by every panel ---------------------------------------

pub fn card() -> Color32 {
    current().card
}
pub fn field() -> Color32 {
    current().field
}
pub fn bubble() -> Color32 {
    current().bubble
}
pub fn text() -> Color32 {
    current().text
}
pub fn dim() -> Color32 {
    current().dim
}
pub fn accent() -> Color32 {
    current().accent
}
pub fn structure() -> Color32 {
    current().structure
}
pub fn on_accent() -> Color32 {
    current().on_accent
}
pub fn danger() -> Color32 {
    current().danger
}
pub fn warn() -> Color32 {
    current().warn
}
pub fn ok() -> Color32 {
    current().ok
}
/// Apply a theme to egui and make it the palette the panels read.
pub fn apply(ctx: &Context, choice: ThemeChoice) {
    let p = palette_for(choice);
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = p;

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.scroll.bar_width = 8.0;
    ctx.set_style(style);

    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.panel;
    v.window_fill = p.card;
    v.extreme_bg_color = p.field;
    v.faint_bg_color = p.card;
    v.code_bg_color = p.field;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    v.window_stroke = Stroke::new(1.0_f32, p.structure);
    v.window_rounding = Rounding::same(8.0);
    v.selection.bg_fill = p.structure;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);

    let rounding = Rounding::same(6.0);
    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.weak_bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.structure);
    v.widgets.noninteractive.rounding = rounding;

    v.widgets.inactive.bg_fill = p.card;
    v.widgets.inactive.weak_bg_fill = p.card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.structure);
    v.widgets.inactive.rounding = rounding;

    v.widgets.hovered.bg_fill = p.bubble;
    v.widgets.hovered.weak_bg_fill = p.bubble;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
    v.widgets.hovered.rounding = rounding;

    v.widgets.active.bg_fill = p.structure;
    v.widgets.active.weak_bg_fill = p.structure;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    v.widgets.active.rounding = rounding;

    ctx.set_visuals(v);
}

/// A four-stripe accent bar, drawn to the given width.
pub fn stripes(ui: &mut egui::Ui, width: f32) {
    let p = current();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 3.0), egui::Sense::hover());
    let step = rect.width() / p.stripes.len() as f32;
    let painter = ui.painter();
    for (i, color) in p.stripes.iter().enumerate() {
        let min = egui::pos2(rect.min.x + step * i as f32, rect.min.y);
        painter.rect_filled(
            egui::Rect::from_min_size(min, egui::vec2(step, rect.height())),
            Rounding::ZERO,
            *color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_is_distinct_and_legible() {
        let all = [
            (ThemeChoice::Nature, NATURE),
            (ThemeChoice::Midnight, MIDNIGHT),
            (ThemeChoice::Retro, RETRO),
            (ThemeChoice::Paper, PAPER),
        ];
        for (i, (choice, p)) in all.iter().enumerate() {
            assert_eq!(palette_for(*choice), *p);
            // Text must differ from its background, or nothing is readable.
            assert_ne!(p.text, p.bg, "{choice:?} text matches its background");
            assert_ne!(p.accent, p.bg, "{choice:?} accent vanishes");
            // No two themes may be identical.
            for (j, (other, q)) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(p, q, "{choice:?} duplicates {other:?}");
                }
            }
        }
        assert!(!PAPER.dark, "paper is the light theme");
        assert!(NATURE.dark);
    }

    #[test]
    fn accessors_track_the_applied_theme() {
        // Defaults to Nature before anything is applied.
        assert_eq!(text(), NATURE.text);

        let ctx = egui::Context::default();
        apply(&ctx, ThemeChoice::Paper);
        assert_eq!(text(), PAPER.text);
        assert_eq!(accent(), PAPER.accent);
        assert_eq!(field(), PAPER.field);
        assert_eq!(structure(), PAPER.structure);

        apply(&ctx, ThemeChoice::Nature);
        assert_eq!(text(), NATURE.text);
    }
}
