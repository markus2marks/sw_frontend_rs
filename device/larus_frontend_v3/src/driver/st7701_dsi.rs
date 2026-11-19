use defmt::error;
use embedded_display_controller::dsi::{DsiHostCtrlIo, DsiWriteCommand};
use embedded_hal::blocking::delay::DelayMs;

/// Kurze Pause nach jedem normalen Befehl. Das Original hat hier 150 ms gewartet.
/// Falls das Display mit dem kleineren Wert Probleme macht, testweise wieder erhöhen.
const CMD_DELAY_MS: u32 = 10;

/// Wie oft ein fehlgeschlagener DSI-Schreibzugriff wiederholt wird
const RETRIES: u8 = 3;

pub struct St7701;

/// Hilfsstruktur: sendet Befehle, wiederholt sie bei Fehlern und zählt die Fehler
struct Tx<'a, D: DsiHostCtrlIo, L: DelayMs<u32>> {
    dsi: &'a mut D,
    delay: &'a mut L,
    errs: u32,
}

impl<'a, D: DsiHostCtrlIo, L: DelayMs<u32>> Tx<'a, D, L> {
    /// Befehl ohne Parameter, z. B. 0x01, 0x11, 0x29.
    /// Wird wie in der Original-Datei als DcsShortP1 mit data = 0x00 gesendet,
    /// weil die HAL DcsShortP0 nicht unterstützt (Panic).
    fn p0(&mut self, cmd: u8, wait_ms: u32) {
        let mut ok = false;
        for _ in 0..RETRIES {
            if self
                .dsi
                .write(DsiWriteCommand::DcsShortP1 { arg: cmd, data: 0x00 })
                .is_ok()
            {
                ok = true;
                break;
            }
            self.delay.delay_ms(1);
        }
        if !ok {
            self.errs += 1;
            error!("st7701: write p0 {=u8:#x} failed", cmd);
        }
        self.delay.delay_ms(wait_ms);
    }

    /// Befehl mit einem Parameter (DCS Short Write, 2 Byte)
    fn p1(&mut self, cmd: u8, data: u8) {
        let mut ok = false;
        for _ in 0..RETRIES {
            if self
                .dsi
                .write(DsiWriteCommand::DcsShortP1 { arg: cmd, data })
                .is_ok()
            {
                ok = true;
                break;
            }
            self.delay.delay_ms(1);
        }
        if !ok {
            self.errs += 1;
            error!("st7701: write p1 {=u8:#x} failed", cmd);
        }
        self.delay.delay_ms(CMD_DELAY_MS);
    }

    /// Befehl mit mehreren Parametern (DCS Long Write)
    fn long(&mut self, cmd: u8, data: &[u8]) {
        let mut ok = false;
        for _ in 0..RETRIES {
            if self
                .dsi
                .write(DsiWriteCommand::DcsLongWrite { arg: cmd, data })
                .is_ok()
            {
                ok = true;
                break;
            }
            self.delay.delay_ms(1);
        }
        if !ok {
            self.errs += 1;
            error!("st7701: write long {=u8:#x} failed", cmd);
        }
        self.delay.delay_ms(CMD_DELAY_MS);
    }

    /// Command-Bank umschalten (0x00 = User, 0x10 = BK0, 0x11 = BK1, 0x13 = BK3)
    fn bank(&mut self, bank: u8) {
        self.long(0xFF, &[0x77, 0x01, 0x00, 0x00, bank]);
    }
}

impl St7701 {
    pub fn new() -> Self {
        Self {}
    }

    /// Initialisiert das Panel. Rückgabe: Anzahl der Befehle, die auch nach
    /// Wiederholungen nicht gesendet werden konnten (0 = alles in Ordnung).
    pub fn init<D: DsiHostCtrlIo, L: DelayMs<u32>>(&mut self, dsi: &mut D, delay: &mut L) -> u32 {
        let mut t = Tx {
            dsi,
            delay,
            errs: 0,
        };

        // =========================================================
        // SOFTWARE RESET (Hardware-Reset ist extern)
        // =========================================================
        t.p0(0x01, 150);

        // =========================================================
        // SLEEP OUT (danach mindestens 120 ms warten)
        // =========================================================
        t.p0(0x11, 150);

        // =========================================================
        // BK13
        // =========================================================
        t.bank(0x13);
        t.p1(0xEF, 0x08);
        t.p1(0x3A, 0x60);

        // =========================================================
        // BK10
        // =========================================================
        t.bank(0x10);
        t.long(0xC0, &[0x3B, 0x00]);
        t.long(0xC1, &[0x0A, 0x0A]);
        t.long(0xC2, &[0x37, 0x02]);
        t.p1(0xC6, 0x21);
        t.p1(0xCC, 0x30);
        t.long(
            0xB0,
            &[
                0xC0, 0x54, 0x5C, 0x0D, 0x51, 0x06, 0x09, 0x08, 0x07, 0x24, 0x03, 0x11, 0x0F,
                0xAC, 0xB5, 0x7F,
            ],
        );
        t.long(
            0xB1,
            &[
                0xC0, 0x54, 0x5C, 0x0E, 0x11, 0x07, 0x0A, 0x09, 0x08, 0x24, 0x04, 0x51, 0x10,
                0xAD, 0x75, 0x7F,
            ],
        );

        // =========================================================
        // BK11
        // =========================================================
        t.bank(0x11);
        t.p1(0xB0, 0x7D);
        t.p1(0xB1, 0x33);
        t.p1(0xB2, 0x87);
        t.p1(0xB3, 0x80);
        t.p1(0xB5, 0x45);
        t.p1(0xB7, 0x87);
        t.p1(0xB8, 0x33);
        t.p1(0xB9, 0x10);
        t.p1(0xBB, 0x03);
        t.p1(0xC0, 0x03);
        t.p1(0xC1, 0x78);
        t.p1(0xC2, 0x78);
        t.p1(0xD0, 0x88);

        t.long(0xE0, &[0x00, 0x18, 0x00, 0x00, 0x00, 0x20]);
        t.long(
            0xE1,
            &[
                0x05, 0xA0, 0x00, 0xA0, 0x04, 0x0A, 0x00, 0xA0, 0x00, 0x44, 0x44,
            ],
        );
        t.long(
            0xE2,
            &[
                0x11, 0x11, 0x44, 0x44, 0xEA, 0xA0, 0x00, 0x00, 0xE9, 0xA0, 0x00, 0x00,
            ],
        );
        t.long(0xE3, &[0x00, 0x00, 0x11, 0x11]);
        t.long(0xE4, &[0x44, 0x44]);
        t.long(
            0xE5,
            &[
                0x06, 0xE5, 0xD8, 0xA0, 0x08, 0xE7, 0xD8, 0xA0, 0x0A, 0xE9, 0xD8, 0xA0, 0x0C,
                0xEB, 0xD8, 0xA0,
            ],
        );
        t.long(0xE6, &[0x00, 0x00, 0x11, 0x11]);
        t.long(0xE7, &[0x44, 0x44]);
        t.long(
            0xE8,
            &[
                0x05, 0xE4, 0xD8, 0xA0, 0x07, 0xE6, 0xD8, 0xA0, 0x09, 0xE8, 0xD8, 0xA0, 0x0B,
                0xEA, 0xD8, 0xA0,
            ],
        );
        t.long(0xEB, &[0x02, 0x00, 0xE4, 0xE4, 0x88, 0x00, 0x10]);
        t.long(0xEC, &[0x3D, 0x02, 0x00]);
        t.long(
            0xED,
            &[
                0x20, 0x76, 0x54, 0x98, 0xBA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xAB, 0x89,
                0x45, 0x67, 0x02,
            ],
        );
        t.long(0xEF, &[0x08, 0x08, 0x08, 0x45, 0x3F, 0x54]);

        // =========================================================
        // ZURÜCK AUF USER-BANK
        // =========================================================
        t.bank(0x00);
        t.delay.delay_ms(20);

        // =========================================================
        // DISPLAY ON
        // =========================================================
        t.p0(0x29, 120);

        t.errs
    }
}
