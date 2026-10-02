//! QSPI-Flash als Speicher-Backend für corelibs `Eeprom<S>`.
//!
//! `Eeprom<S>` (persistence.rs) ist hardwareunabhängig und braucht nur ein
//! `EepromTrait` (write_byte, write_page, read_byte, read_data). Hier wird
//! dieses Trait auf ein RAM-Image abgebildet, das verzögert in den Flash
//! geschrieben wird. Profile, DAT, `restore_standard`, `iter_over` usw.
//! bleiben damit unverändert.
//!
//! Aufbau:
//!
//! * `FlashStorage`: Handle, das an `Eeprom::new(..)` übergeben wird. Liest und
//!   schreibt nur das RAM-Image.
//! * `FlashConfig`: Besitzt den `Flash`, lädt das Image beim Start und
//!   schreibt es mit `flush_if_due` / `flush_now` in den Flash.
//!
//! Beide greifen auf dasselbe statische Image zu, weil `Eeprom` seinen Storage
//! besitzt und die Idle-Loop den Flash-Zugriff trotzdem anstoßen muss.
//!
//! Warum die Verzögerung: Jeder Speichervorgang kostet einen Sektor-Erase
//! (~50 ms, max. einige hundert ms) und Lebensdauer (typ. 100k Zyklen). Bei
//! mehreren Änderungen kurz hintereinander (z. B. Encoder) wird nur einmal
//! geschrieben, `FLUSH_DELAY_MS` nach der letzten Änderung.

use core::cell::RefCell;
use core::ops::Range;

use corelib::{CoreError, EepromTrait};

use super::flash::{config_load, config_save, ConfigError, Flash};

/// Größe des Images = Größe des bisherigen EEPROMs (feature `eeprom_size_8192`).
/// `Eeprom` belegt tatsächlich nur ca. die ersten 4,4 KiB. Gespeichert wird
/// nur bis zum letzten Byte != 0xFF.
pub const IMAGE_SIZE: usize = 8192;

/// So lange nach der letzten Änderung wird gewartet, bevor geschrieben wird.
/// `timestamp_ms()` liefert u16 (Überlauf nach ~65 s), daher u16 mit wrapping_sub.
pub const FLUSH_DELAY_MS: u16 = 2000;

struct Image {
    data: [u8; IMAGE_SIZE],
    /// Das Image unterscheidet sich vom Flash-Inhalt.
    dirty: bool,
    /// Seit dem letzten `flush_if_due` wurde geschrieben (Timer neu starten).
    touched: bool,
}

struct IdleOnly<T>(T);

// SAFETY: Das Image wird ausschließlich von `FlashStorage` (steckt in `Eeprom`)
// und `FlashConfig` benutzt, beide gehören der `IdleLoop` und werden nur aus
// dem Idle-Kontext aufgerufen (vorher aus `hw_init`, bevor Interrupts/Tasks
// laufen). Es gibt keine nebenläufigen Zugriffe. `RefCell` fängt Fehler zur
// Laufzeit ab.
unsafe impl<T> Sync for IdleOnly<T> {}

// Initialisiert mit 0 (landet in .bss); `FlashConfig::new` füllt mit 0xFF.
static IMAGE: IdleOnly<RefCell<Image>> = IdleOnly(RefCell::new(Image {
    data: [0; IMAGE_SIZE],
    dirty: false,
    touched: false,
}));

fn checked_range(address: u32, len: usize) -> Result<Range<usize>, CoreError> {
    let start = address as usize;
    let end = start.checked_add(len).ok_or(CoreError::OutOfRange)?;
    if end > IMAGE_SIZE {
        return Err(CoreError::OutOfRange);
    }
    Ok(start..end)
}

/// Backend für `Eeprom::new(storage, ..)`.
pub struct FlashStorage {
    _private: (),
}

impl EepromTrait for FlashStorage {
    fn write_byte(&mut self, address: u32, data: u8) -> Result<(), CoreError> {
        self.write_page(address, &[data])
    }

    fn write_page(&mut self, address: u32, data: &[u8]) -> Result<(), CoreError> {
        let range = checked_range(address, data.len())?;
        let mut img = IMAGE.0.borrow_mut();
        if img.data[range.clone()] != *data {
            img.data[range].copy_from_slice(data);
            img.dirty = true;
            img.touched = true;
        }
        Ok(())
    }

    fn read_byte(&mut self, address: u32) -> Result<u8, CoreError> {
        let mut byte = [0u8; 1];
        self.read_data(address, &mut byte)?;
        Ok(byte[0])
    }

    fn read_data(&mut self, address: u32, data: &mut [u8]) -> Result<(), CoreError> {
        let range = checked_range(address, data.len())?;
        data.copy_from_slice(&IMAGE.0.borrow().data[range]);
        Ok(())
    }
}

/// Besitzt den Flash und schreibt das Image zurück.
pub struct FlashConfig {
    flash: Flash,
    last_change_ms: u16,
}

impl FlashConfig {
    /// Lädt die gespeicherte Konfiguration ins RAM-Image. Ist keine gültige
    /// vorhanden, ist das Image komplett 0xFF (wie ein leeres EEPROM; `Eeprom`
    /// legt dann beim Start selbst die Magic-Number und die DAT an).
    ///
    /// Nur EINMAL aufrufen.
    pub fn new(flash: Flash) -> (Self, FlashStorage) {
        {
            let mut img = IMAGE.0.borrow_mut();
            img.data.fill(0xFF);
            let _ = config_load(&flash, &mut img.data);
            img.dirty = false;
            img.touched = false;
        }
        (
            Self {
                flash,
                last_change_ms: 0,
            },
            FlashStorage { _private: () },
        )
    }

    pub fn is_dirty(&self) -> bool {
        IMAGE.0.borrow().dirty
    }

    /// In der Idle-Loop regelmäßig aufrufen. `feed` füttert den Watchdog und
    /// wird vor und nach dem Schreiben aufgerufen.
    pub fn flush_if_due(&mut self, now_ms: u16, feed: impl FnMut()) -> Result<(), ConfigError> {
        {
            let mut img = IMAGE.0.borrow_mut();
            if !img.dirty {
                return Ok(());
            }
            if img.touched {
                // Es wurde gerade geschrieben: Wartezeit neu starten.
                img.touched = false;
                self.last_change_ms = now_ms;
                return Ok(());
            }
        }
        if now_ms.wrapping_sub(self.last_change_ms) < FLUSH_DELAY_MS {
            return Ok(());
        }

        let result = self.flush_now(feed);
        if result.is_err() {
            // Nicht in jedem Loop-Durchlauf sofort erneut versuchen.
            self.last_change_ms = now_ms;
        }
        result
    }

    /// Sofort schreiben (z. B. vor `install_and_restart()`).
    pub fn flush_now(&mut self, mut feed: impl FnMut()) -> Result<(), ConfigError> {
        {
            let img = IMAGE.0.borrow();
            if !img.dirty {
                return Ok(());
            }
            // Nur bis zum letzten belegten Byte speichern, spart Programmierzeit.
            let used = img
                .data
                .iter()
                .rposition(|&b| b != 0xFF)
                .map_or(0, |i| i + 1);

            feed();
            config_save(&mut self.flash, &img.data[..used])?;
            feed();
        }

        let mut img = IMAGE.0.borrow_mut();
        img.dirty = false;
        img.touched = false;
        Ok(())
    }
}