//! Tabla central con los bucles del destino abierto (tarjeta o pedal).

use eframe::egui::{self, Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder};
use jamman_core::card::{RhythmType, StopMode};
use jamman_core::device::Slot;

use crate::app::JamApp;
use crate::state::{Row, SortKey};
use crate::{fmt, theme};

/// Acción pedida desde una fila.
enum RowAction {
    Click(Slot),
    Play(Slot),
    Export(Slot),
    Delete(Slot),
}

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    if !app.lib.is_open() {
        empty_state(ui);
        return;
    }
    if app.lib.loading {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            ui.spinner();
            ui.label("Leyendo los bucles del pedal por USB…");
            ui.weak("Con muchos bucles tarda unos segundos.");
        });
        return;
    }
    let rows: Vec<Row> = app.lib.visible().into_iter().cloned().collect();
    if rows.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.weak(if app.lib.rows().is_empty() {
                "No hay bucles. Añade canciones desde el explorador, con «＋ Añadir canciones…» o arrastrándolas aquí."
            } else {
                "Ningún bucle coincide con la búsqueda."
            });
        });
        return;
    }

    let selection = app.lib.selection().clone();
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

    // Ancho del nombre: lo que dejan libre las demás columnas y sus
    // separaciones. Si no cabe todo, la valoración se oculta (sigue en el
    // panel de ajustes).
    const FIXED: [f32; 6] = [44.0, 64.0, 46.0, 82.0, 44.0, 56.0];
    const RATING: f32 = 80.0;
    const MIN_NAME: f32 = 150.0;
    let spacing = ui.spacing().item_spacing.x;
    let fixed: f32 = FIXED.iter().sum::<f32>() + spacing * FIXED.len() as f32 + 16.0;
    let show_rating = ui.available_width() - fixed - RATING - spacing >= MIN_NAME;
    let extra = if show_rating { RATING + spacing } else { 0.0 };
    let name_width = (ui.available_width() - fixed - extra).max(MIN_NAME);

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(FIXED[0]))
        .column(Column::exact(name_width).clip(true));
    for w in &FIXED[1..] {
        table = table.column(Column::exact(*w));
    }
    if show_rating {
        table = table.column(Column::exact(RATING));
    }
    table
        .header(30.0, |mut h| {
            h.col(|ui| header_cell(ui, "Nº", Some(SortKey::Slot)));
            h.col(|ui| header_cell(ui, "Nombre", Some(SortKey::Name)));
            h.col(|ui| header_cell(ui, "Duración", Some(SortKey::Duration)));
            h.col(|ui| header_cell(ui, "BPM", Some(SortKey::Bpm)));
            h.col(|ui| header_cell(ui, "Modo", None));
            h.col(|ui| header_cell(ui, "Ritmo", None));
            h.col(|ui| header_cell(ui, "Parada", None));
            if show_rating {
                h.col(|ui| header_cell(ui, "Valoración", None));
            }
        })
        .body(|body| {
            body.rows(30.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                let slot = r.slot;
                let in_selection = selection.contains(&slot);
                row.set_selected(in_selection);

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
                    if show_rating {
                        row.col(|ui| {
                            ui.label(RichText::new(stars(s.rating)).color(theme::ACCENT));
                        });
                    }
                } else {
                    for _ in 0..(4 + usize::from(show_rating)) {
                        row.col(|_| {});
                    }
                }

                let response = row.response();
                if response.double_clicked() {
                    action = Some(RowAction::Play(slot));
                } else if response.clicked() {
                    action = Some(RowAction::Click(slot));
                }
                // El menú actúa sobre la selección si la fila está en ella.
                let many = in_selection && selection.len() > 1;
                response.context_menu(|ui| {
                    if ui.button("▶ Reproducir").clicked() {
                        action = Some(RowAction::Play(slot));
                    }
                    let export = if many {
                        format!("⬅ Exportar {} a la carpeta del explorador", selection.len())
                    } else {
                        "⬅ Exportar a la carpeta del explorador".to_owned()
                    };
                    if ui.button(export).clicked() {
                        action = Some(RowAction::Export(slot));
                    }
                    ui.separator();
                    if ui
                        .button(
                            RichText::new(if many {
                                format!("🗑 Borrar {}…", selection.len())
                            } else {
                                "🗑 Borrar…".to_owned()
                            })
                            .color(theme::DANGER),
                        )
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
        Some(RowAction::Click(slot)) => app.request_click(slot, super::click_mods(ui)),
        Some(RowAction::Play(slot)) => {
            app.request_select(Some(slot));
            if app.lib.selected() == Some(slot) {
                app.play(slot, 0.0);
            }
        }
        Some(RowAction::Export(slot)) => {
            if selection.contains(&slot) {
                app.export_selection(ui.ctx());
            } else {
                app.export_to_browser(vec![slot], ui.ctx());
            }
        }
        Some(RowAction::Delete(slot)) => {
            if selection.contains(&slot) {
                app.ask_delete_selection();
            } else {
                app.confirm_delete = Some(vec![slot]);
            }
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
            RichText::new("No hay ninguna tarjeta ni pedal abierto")
                .size(20.0)
                .strong(),
        );
        ui.add_space(4.0);
        ui.weak("Inserta la tarjeta SD del JamMan en el ordenador y pulsa «Detectar»,");
        ui.weak("o ábrela con «Abrir carpeta…». No hace falta conectar el pedal.");
        ui.weak("Si conectas el pedal por USB, también aparece en el selector.");
    });
}
