//! Aspecto visual: tema oscuro y claro con un color de acento.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Theme, Vec2};

/// Color de acento (naranja).
pub const ACCENT: Color32 = Color32::from_rgb(0xF5, 0x8A, 0x1F);

/// Color para acciones destructivas.
pub const DANGER: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);

/// Aplica el estilo a los temas oscuro y claro.
pub fn apply(ctx: &egui::Context) {
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = Vec2::new(8.0, 6.0);
            style.spacing.button_padding = Vec2::new(10.0, 5.0);
            style.spacing.interact_size.y = 26.0;

            let dark = theme == Theme::Dark;
            let v = &mut style.visuals;
            let radius = CornerRadius::same(6);
            v.window_corner_radius = CornerRadius::same(10);
            v.menu_corner_radius = radius;
            for w in [
                &mut v.widgets.noninteractive,
                &mut v.widgets.inactive,
                &mut v.widgets.hovered,
                &mut v.widgets.active,
                &mut v.widgets.open,
            ] {
                w.corner_radius = radius;
            }
            v.selection.bg_fill = if dark {
                Color32::from_rgb(0x7A, 0x45, 0x10)
            } else {
                Color32::from_rgb(0xFF, 0xD9, 0xB3)
            };
            // Color del texto seleccionado: contraste sobre el fondo de acento.
            v.selection.stroke =
                Stroke::new(1.0, if dark { Color32::WHITE } else { Color32::BLACK });
            v.hyperlink_color = ACCENT;
            v.slider_trailing_fill = true;
            v.striped = true;
            if dark {
                v.panel_fill = Color32::from_rgb(0x1B, 0x1D, 0x22);
                v.window_fill = Color32::from_rgb(0x22, 0x25, 0x2B);
                v.extreme_bg_color = Color32::from_rgb(0x13, 0x14, 0x18);
                v.faint_bg_color = Color32::from_rgb(0x21, 0x24, 0x29);
            }
        });
    }
}
