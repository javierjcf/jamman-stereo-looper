//! Panel derecho: ajustes del bucle seleccionado y acciones.

use eframe::egui::{self, Color32, DragValue, RichText, TextEdit};
use jamman_core::card::options::GENRES;
use jamman_core::card::{RhythmType, StopMode};
use jamman_core::device::{SLOTS_PER_STORAGE, Slot};

use crate::app::JamApp;
use crate::theme;

pub fn show(app: &mut JamApp, ui: &mut egui::Ui) {
    let Some(slot) = app.lib.draft.as_ref().map(|d| d.slot) else {
        return;
    };
    let on_pedal = app.lib.pedal().is_some();
    let summary = app
        .lib
        .row(slot)
        .map(|r| {
            format!(
                "{} · {}",
                crate::fmt::duration(r.duration_secs),
                crate::fmt::bytes(r.audio_bytes)
            )
        })
        .unwrap_or_default();
    let mut target = match app.move_target {
        (Some(s), t) if s == slot => t,
        _ => slot.get(),
    };
    let Some(draft) = app.lib.draft.as_mut() else {
        return;
    };
    let (mut save, mut discard, mut export, mut delete, mut move_to) =
        (false, false, false, false, None);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(8.0);
        ui.heading(format!("Bucle {:02}", slot.get()));
        ui.weak(summary);
        ui.add_space(6.0);
        let s = &mut draft.edited;

        egui::Grid::new("loop_settings")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Nombre");
                ui.add(TextEdit::singleline(&mut s.name).desired_width(f32::INFINITY));
                ui.end_row();

                ui.label("Tempo");
                ui.add(
                    DragValue::new(&mut s.bpm)
                        .range(40.0..=240.0)
                        .speed(0.1)
                        .max_decimals(2)
                        .suffix(" BPM"),
                )
                .on_hover_text("Arrastra o escribe un valor entre 40 y 240");
                ui.end_row();

                ui.label("Compás");
                ui.add(
                    DragValue::new(&mut s.beats_per_measure)
                        .range(2..=15)
                        .suffix(" tiempos"),
                )
                .on_hover_text("Tiempos por compás (2 a 15): 4 = 4/4, 7 = 7/8…");
                ui.end_row();

                ui.label("Reproducción");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.is_loop, true, "🔁 Bucle");
                    ui.radio_value(&mut s.is_loop, false, "1× Una vez");
                });
                ui.end_row();

                ui.label("Invertido");
                ui.checkbox(&mut s.is_reversed, "Reproducir al revés");
                ui.end_row();

                ui.label("Parada");
                let current = StopMode::from_xml(&s.stop_mode)
                    .map_or_else(|| s.stop_mode.clone(), |m| m.label().to_owned());
                egui::ComboBox::from_id_salt("stop_mode")
                    .selected_text(current)
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        for m in StopMode::ALL {
                            ui.selectable_value(&mut s.stop_mode, m.xml().to_owned(), m.label());
                        }
                    });
                ui.end_row();

                ui.label("Ritmo");
                let current = RhythmType::from_xml(&s.rhythm_type)
                    .map_or_else(|| s.rhythm_type.clone(), |r| r.label().to_owned());
                egui::ComboBox::from_id_salt("rhythm")
                    .selected_text(current)
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        for r in RhythmType::ALL {
                            ui.selectable_value(&mut s.rhythm_type, r.xml().to_owned(), r.label());
                        }
                    });
                ui.end_row();
            });

        ui.add_space(10.0);
        ui.label(RichText::new("Catálogo").strong());
        egui::Grid::new("catalog")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Artista");
                ui.add(TextEdit::singleline(&mut s.artist).desired_width(f32::INFINITY));
                ui.end_row();

                ui.label("Género");
                egui::ComboBox::from_id_salt("genre")
                    .selected_text(s.genre.clone())
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        for g in GENRES {
                            ui.selectable_value(&mut s.genre, g.to_owned(), g);
                        }
                    });
                ui.end_row();

                ui.label("Valoración");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for i in 1..=5u8 {
                        let filled = s.rating >= i;
                        let star = RichText::new(if filled { "★" } else { "☆" })
                            .size(18.0)
                            .color(theme::ACCENT);
                        if ui.add(egui::Button::new(star).frame(false)).clicked() {
                            s.rating = if s.rating == i { 0 } else { i };
                        }
                    }
                });
                ui.end_row();

                ui.label("Etiquetas");
                ui.add(TextEdit::singleline(&mut s.tags).desired_width(f32::INFINITY));
                ui.end_row();
            });
        ui.label("Descripción");
        ui.add(
            TextEdit::multiline(&mut s.description)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(10.0);
        let dirty = draft.is_dirty();
        ui.horizontal(|ui| {
            let save_button =
                egui::Button::new(RichText::new("💾 Guardar").strong().color(if dirty {
                    Color32::BLACK
                } else {
                    ui.visuals().weak_text_color()
                }))
                .fill(if dirty {
                    theme::ACCENT
                } else {
                    ui.visuals().faint_bg_color
                });
            if ui
                .add_enabled(dirty, save_button)
                .on_hover_text("Ctrl+S")
                .clicked()
            {
                save = true;
            }
            if ui
                .add_enabled(dirty, egui::Button::new("Descartar"))
                .clicked()
            {
                discard = true;
            }
        });

        ui.add_space(14.0);
        ui.separator();
        ui.label(RichText::new("Acciones").strong());
        // El pedal no admite renombrar carpetas: mover solo en tarjetas del PC.
        if !on_pedal {
            ui.horizontal(|ui| {
                ui.label("Mover a la posición")
                    .on_hover_text("También puedes arrastrar un bucle sobre otro en la tabla");
                ui.add(DragValue::new(&mut target).range(1..=SLOTS_PER_STORAGE));
                if ui
                    .add_enabled(target != slot.get(), egui::Button::new("Mover"))
                    .on_hover_text("Si la posición está ocupada, se intercambian")
                    .clicked()
                {
                    move_to = Slot::new(target).ok();
                }
            });
        }
        if ui
            .button("⬅ Exportar a la carpeta del explorador")
            .on_hover_text("Copia el WAV de este bucle a la carpeta abierta en el explorador")
            .clicked()
        {
            export = true;
        }
        if ui
            .button(RichText::new("🗑 Borrar bucle…").color(theme::DANGER))
            .clicked()
        {
            delete = true;
        }
    });

    app.move_target = (Some(slot), target);
    if save {
        app.save(ui.ctx());
    }
    if discard {
        app.lib.discard_draft();
    }
    if export {
        app.export_to_browser(vec![slot], ui.ctx());
    }
    if delete {
        app.confirm_delete = Some(vec![slot]);
    }
    if let Some(to) = move_to {
        app.move_or_swap(slot, to);
    }
}
