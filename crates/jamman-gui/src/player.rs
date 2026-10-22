//! Reproducción del audio de los bucles.
//!
//! El dispositivo de audio se abre al reproducir por primera vez: si el
//! equipo no tiene salida de audio, el resto de la aplicación funciona igual.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use jamman_core::device::Slot;

/// Reproductor de un bucle cada vez.
pub struct Player {
    device: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
    current: Option<Slot>,
    duration_secs: f64,
    volume: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            device: None,
            player: None,
            current: None,
            duration_secs: 0.0,
            volume: 0.8,
        }
    }
}

impl Player {
    /// Reproduce el audio de `path` desde `start_secs`.
    pub fn play(
        &mut self,
        slot: Slot,
        path: &Path,
        duration_secs: f64,
        start_secs: f64,
    ) -> Result<(), String> {
        if self.device.is_none() {
            let device = rodio::DeviceSinkBuilder::open_default_sink()
                .map_err(|e| format!("no hay salida de audio disponible: {e}"))?;
            self.device = Some(device);
        }
        let Some(device) = &self.device else {
            return Err("no hay salida de audio disponible".into());
        };
        let file = File::open(path).map_err(|e| e.to_string())?;
        let source = rodio::Decoder::try_from(file).map_err(|e| e.to_string())?;
        let player = rodio::Player::connect_new(device.mixer());
        player.set_volume(self.volume);
        player.append(source);
        if start_secs > 0.0 {
            let _ = player.try_seek(Duration::from_secs_f64(start_secs));
        }
        // Al sustituir el reproductor anterior, este se detiene.
        self.player = Some(player);
        self.current = Some(slot);
        self.duration_secs = duration_secs;
        Ok(())
    }

    /// Pausa o reanuda.
    pub fn toggle_pause(&mut self) {
        if let Some(p) = &self.player {
            if p.is_paused() {
                p.play();
            } else {
                p.pause();
            }
        }
    }

    /// Detiene la reproducción.
    pub fn stop(&mut self) {
        self.player = None;
        self.current = None;
    }

    /// Salta a una posición del bucle actual.
    pub fn seek(&mut self, secs: f64) {
        if let Some(p) = &self.player {
            let _ = p.try_seek(Duration::from_secs_f64(secs.clamp(0.0, self.duration_secs)));
        }
    }

    /// Bucle cargado en el reproductor.
    pub fn current(&self) -> Option<Slot> {
        self.current
    }

    /// `true` si está sonando (cargado, no en pausa y sin terminar).
    pub fn is_playing(&self) -> bool {
        self.player
            .as_ref()
            .is_some_and(|p| !p.is_paused() && !p.empty())
    }

    /// `true` si terminó de sonar el bucle cargado.
    pub fn finished(&self) -> bool {
        self.player.as_ref().is_some_and(rodio::Player::empty)
    }

    /// Posición actual en segundos.
    pub fn position_secs(&self) -> f64 {
        self.player
            .as_ref()
            .map_or(0.0, |p| p.get_pos().as_secs_f64().min(self.duration_secs))
    }

    /// Volumen (0 a 1).
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Cambia el volumen.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(p) = &self.player {
            p.set_volume(self.volume);
        }
    }
}
