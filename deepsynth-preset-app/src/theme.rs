// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Synthwave visual theme for the DeepSynth Preset app. The palette matches the
// GitHub Pages landing page (docs/index.html) exactly, so the desktop app and
// the website read as one product.

use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke};

// --- brand palette (identical to docs/index.html :root vars) ----------------
pub const BG: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x12);
pub const BG2: Color32 = Color32::from_rgb(0x12, 0x12, 0x1d);
pub const PANEL: Color32 = Color32::from_rgb(0x17, 0x17, 0x25);
pub const LINE: Color32 = Color32::from_rgb(0x26, 0x26, 0x3a);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xf0);
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0x96, 0xb0);
pub const ACCENT: Color32 = Color32::from_rgb(0xe0, 0x50, 0x7a); // pink
pub const ACCENT2: Color32 = Color32::from_rgb(0x56, 0xd8, 0xc9); // teal
pub const MONO: Color32 = Color32::from_rgb(0xcf, 0xcb, 0xe4); // code text
pub const WELL: Color32 = Color32::from_rgb(0x0d, 0x0d, 0x15); // input well

/// Corner rounding used across the app.
pub const RADIUS: u8 = 8;

/// Install the synthwave visuals + spacing onto the egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.extreme_bg_color = WELL; // TextEdit / scroll background
    visuals.faint_bg_color = BG2; // zebra striping in striped grids
    visuals.hyperlink_color = ACCENT2;
    // Accent selection + focused-input border.
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0xe0, 0x50, 0x7a, 70);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    let r = CornerRadius::same(RADIUS);
    let w = &mut visuals.widgets;

    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = BG2;
    w.inactive.weak_bg_fill = BG2;
    w.inactive.bg_stroke = Stroke::new(1.0, LINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = PANEL;
    w.hovered.weak_bg_fill = PANEL;
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
    w.hovered.corner_radius = r;

    w.active.bg_fill = BG2;
    w.active.weak_bg_fill = BG2;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, TEXT);
    w.active.corner_radius = r;

    w.open.bg_fill = BG2;
    w.open.weak_bg_fill = BG2;
    w.open.bg_stroke = Stroke::new(1.0, LINE);
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = r;

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = Margin::same(16);
    ctx.set_style(style);
}

/// A small uppercase, letter-spaced, muted field label (web "DESCRIBE THE SOUND").
pub fn field_label(ui: &mut egui::Ui, text: &str) {
    // egui has no letter-spacing, so widen with thin spaces for the same feel.
    let spaced: String = text
        .to_uppercase()
        .chars()
        .flat_map(|c| [c, '\u{2009}'])
        .collect();
    ui.label(egui::RichText::new(spaced.trim_end()).size(11.0).color(MUTED));
}

/// The "Deep<accent>Synth</accent> Preset" wordmark at a given size.
pub fn wordmark(ui: &mut egui::Ui, size: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(egui::RichText::new("Deep").size(size).strong().color(TEXT));
        ui.label(egui::RichText::new("Synth").size(size).strong().color(ACCENT));
        ui.label(egui::RichText::new(" Preset").size(size).strong().color(TEXT));
    });
}
