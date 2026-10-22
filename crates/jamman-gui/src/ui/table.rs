//! Tabla central con los bucles de la tarjeta.

use eframe::egui::{self, Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use jamman_core::card::{RhythmType, StopMode};
use jamman_core::device::Slot;

use crate::app::JamApp;
use crate::state::{Row, SortKey};
use crate::{fmt, theme};

/// Acción pedida desde una fila.
enum RowAction {
    Select(Slot),
    Play(Slot),
    Export(Slot),
    Delete(Slot),
}

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    if app.lib.card().is_none() {
        empty_state(ui);
        return;
    }
    let rows: Vec<Row> = app.lib.visible().into_iter().cloned().collect();
    if rows.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak(if app.lib.rows().is_empty() {
                "La tarjeta está vacía. Añade canciones con «＋ Añadir canciones…» o arrastrándolas aquí."
            } else {
                "Ningún bucle coincide con la búsqueda."
            });
        });
        return;
    }

    let selected = app.lib.selected();
    let playing = app.player.current().filter(|_| app.player.is_playing());
    let (sort, desc) = (app.lib.sort, app.lib.sort_desc);
    let mut action = None;
    let mut sort_click = None;

    let mut header_cell = |ui: &mut egui::Ui, label: &str, key: Option<SortKey>| {
        let arrow = match key {
            Some(k) if k == sort => {
                if desc {
                    " ▼"
                } else {
                    " ▲"
                }
            }
            _ => "",
        };
        let text = RichText::new(format!("{label}{arrow}")).strong();
        match key {
            Some(k) => {
                if ui.add(egui::Button::new(text).frame(false)).clicked() {
                    sort_click = Some(k);
                }
            }
            None => {
                ui.label(text);
            }
        }
    };

    // Ancho del nombre: lo que dejan libre las demás columnas y sus separaciones.
    const FIXED: [f32; 7] = [44.0, 66.0, 48.0, 86.0, 44.0, 56.0, 84.0];
    let spacing = ui.spacing().item_spacing.x * FIXED.len() as f32;
    let name_width = (ui.available_width() - FIXED.iter().sum::<f32>() - spacing - 16.0).max(160.0);

    TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(FIXED[0]))
        .column(Column::exact(name_width).clip(true))
        .column(Column::exact(FIXED[1]))
        .column(Column::exact(FIXED[2]))
        .column(Column::exact(FIXED[3]))
        .column(Column::exact(FIXED[4]))
        .column(Column::exact(FIXED[5]))
        .column(Column::exact(FIXED[6]))
        .header(30.0, |mut h| {
            h.col(|ui| header_cell(ui, "Nº", Some(SortKey::Slot)));
            h.col(|ui| header_cell(ui, "Nombre", Some(SortKey::Name)));
            h.col(|ui| header_cell(ui, "Duración", Some(SortKey::Duration)));
            h.col(|ui| header_cell(ui, "BPM", Some(SortKey::Bpm)));
            h.col(|ui| header_cell(ui, "Modo", None));
            h.col(|ui| header_cell(ui, "Ritmo", None));
            h.col(|ui| header_cell(ui, "Parada", None));
            h.col(|ui| header_cell(ui, "Valoración", None));
        })
        .body(|body| {
            body.rows(30.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                let slot = r.slot;
                row.set_selected(selected == Some(slot));

                row.col(|ui| {
                    if playing == Some(slot) {
                        ui.label(RichText::new("▶").color(theme::ACCENT).strong());
                    } else {
                        ui.label(RichText::new(format!("{:02}", slot.get())).monospace());
                    }
                });
                row.col(|ui| match &r.error {
                    Some(err) => {
                        ui.label(RichText::new(r.name()).color(theme::DANGER))
                            .on_hover_text(err);
                    }
                    None => {
                        ui.label(r.name());
                    }
                });
                row.col(|ui| {
                    ui.label(RichText::new(fmt::duration(r.duration_secs)).monospace());
                });
                if let Some(s) = &r.settings {
                    row.col(|ui| {
                        ui.label(fmt::bpm(s.bpm));
                    });
                    row.col(|ui| {
                        ui.label(if s.is_loop {
                            "🔁 Bucle"
                        } else {
                            "1× Una vez"
                        });
                    });
                    row.col(|ui| {
                        let code = RhythmType::from_xml(&s.rhythm_type)
                            .map_or("?", |r| r.label().split(" · ").next().unwrap_or("?"));
                        ui.label(code).on_hover_text(
                            RhythmType::from_xml(&s.rhythm_type)
                                .map_or_else(|| s.rhythm_type.clone(), |r| r.label().to_owned()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            StopMode::from_xml(&s.stop_mode).map_or(s.stop_mode.as_str(), |m| {
                                m.label().split(" · ").next().unwrap_or("")
                            }),
                        );
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(stars(s.rating)).color(theme::ACCENT));
                    });
                } else {
                    for _ in 0..5 {
                        row.col(|_| {});
                    }
                }

                let response = row.response();
                if response.double_clicked() {
                    action = Some(RowAction::Play(slot));
                } else if response.clicked() {
                    action = Some(RowAction::Select(slot));
                }
                response.context_menu(|ui| {
                    if ui.button("▶ Reproducir").clicked() {
                        action = Some(RowAction::Play(slot));
                    }
                    if ui.button("⬇ Exportar WAV…").clicked() {
                        action = Some(RowAction::Export(slot));
                    }
                    ui.separator();
                    if ui
                        .button(RichText::new("🗑 Borrar…").color(theme::DANGER))
                        .clicked()
                    {
                        action = Some(RowAction::Delete(slot));
                    }
                });
            });
        });

    if let Some(key) = sort_click {
        app.lib.sort_by(key);
    }
    match action {
        Some(RowAction::Select(slot)) => app.request_select(Some(slot)),
        Some(RowAction::Play(slot)) => {
            app.request_select(Some(slot));
            if app.lib.selected() == Some(slot) {
                app.play(slot, 0.0);
            }
        }
        Some(RowAction::Export(slot)) => app.export(slot),
        Some(RowAction::Delete(slot)) => {
            app.request_select(Some(slot));
            app.confirm_delete = Some(slot);
        }
        None => {}
    }
}

fn stars(rating: u8) -> String {
    (1..=5)
        .map(|i| if i <= rating { '★' } else { '☆' })
        .collect()
}

fn empty_state(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.3);
        ui.label(RichText::new("💾").size(48.0));
        ui.add_space(8.0);
        ui.label(
            RichText::new("No hay ninguna tarjeta abierta")
                .size(20.0)
                .strong(),
        );
        ui.add_space(4.0);
        ui.weak("Inserta la tarjeta SD del JamMan en el ordenador y pulsa «Detectar»,");
        ui.weak("o ábrela con «Abrir carpeta…». No hace falta conectar el pedal.");
    });
}
