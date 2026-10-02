//! Externer QSPI-Flash auf dem SoMLabs StarSOM-STM32H757.
//!
//! Zwei Betriebsarten:
//!
//! * Lesen: Memory-Mapped bei 0x9000_0000 (Befehl 0x6B, Quad-Read).
//! * Schreiben: Memory-Mapped ist read-only. Zum Löschen/Programmieren
//!   wird der Memory-Mapped-Modus kurz verlassen (ABORT), die Befehle
//!   laufen im Indirect-Mode, danach wird wieder Memory-Mapped aktiviert.
//!
//! WICHTIG: Während `erase_sector`/`program` läuft, darf niemand (auch
//! kein Interrupt, kein DMA) auf 0x9000_0000 zugreifen, sonst gibt es
//! einen Bus-Fehler.
//!
//! Annahme: Standard-SPI-NOR-Befehlssatz (4-KiB-Sektor, 256-Byte-Seite):
//!   0x06 Write Enable, 0x05 Read Status (Bit0 = WIP, Bit1 = WEL),
//!   0x20 Sector Erase 4 KiB, 0x02 Page Program.
//! Bitte gegen das Datenblatt des verbauten Flash prüfen.
//!
//! Pins (SoMLabs): PF10 CLK, PG6 NCS, PD11 IO0, PD12 IO1, PE2 IO2, PD13 IO3

use stm32h7xx_hal::{
    gpio::{gpiod, gpioe, gpiof, gpiog, Speed},
    pac,
    prelude::*,
    rcc::{rec, CoreClocks},
    xspi::{Qspi, QspiMode},
};

pub const QSPI_MEMORY_ADDRESS: u32 = 0x9000_0000;

/// SoMLabs: 16 MiB externer Flash. FSIZE = 23 => 2^(23 + 1) = 16 MiB.
pub const QSPI_FLASH_SIZE: usize = 16 * 1024 * 1024;
pub const SECTOR_SIZE: usize = 4096;
pub const PAGE_SIZE: usize = 256;

const CMD_WRITE_ENABLE: u8 = 0x06;
const CMD_READ_STATUS: u8 = 0x05;
const CMD_SECTOR_ERASE_4K: u8 = 0x20;
const CMD_PAGE_PROGRAM: u8 = 0x02;

const STATUS_WIP: u8 = 0x01;
const STATUS_WEL: u8 = 0x02;

// Anzahl Status-Abfragen bis Timeout (eine Abfrage dauert ca. 5 us bei 3 MHz).
const ERASE_TIMEOUT_POLLS: u32 = 300_000;
const PROGRAM_TIMEOUT_POLLS: u32 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    OutOfRange,
    Unaligned,
    /// WEL-Bit nach Write Enable nicht gesetzt (z. B. Schreibschutz).
    WriteEnable,
    Timeout,
}

type Regs = pac::quadspi::RegisterBlock;

fn regs() -> &'static Regs {
    unsafe { &*pac::QUADSPI::ptr() }
}

fn wait_idle(r: &Regs) {
    while r.sr.read().busy().bit_is_set() {}
}

/// Memory-Mapped-Modus verlassen.
fn leave_memory_mapped(r: &Regs) {
    r.cr.modify(|_, w| w.abort().set_bit());
    while r.cr.read().abort().bit_is_set() {}
    wait_idle(r);
}

/// Memory-Mapped-Lesen aktivieren:
///   Instruction 0x6B, IMODE 1 Leitung, ADMODE 1 Leitung, ADSIZE 24 bit,
///   kein ABMODE, 8 Dummy Cycles, DMODE 4 Leitungen, FMODE memory mapped,
///   DDR aus, SIOO aus
fn enter_memory_mapped(r: &Regs) {
    wait_idle(r);
    r.ccr.write(|w| unsafe {
        w.instruction()
            .bits(0x6b)
            .imode()
            .bits(1)
            .admode()
            .bits(1)
            .adsize()
            .bits(2) // 24 bit
            .abmode()
            .bits(0)
            .absize()
            .bits(0)
            .dcyc()
            .bits(8)
            .dmode()
            .bits(3) // vier Leitungen
            .fmode()
            .bits(3) // memory mapped
            .sioo()
            .clear_bit()
            .dhhc()
            .clear_bit()
            .ddrm()
            .clear_bit()
    });
}

/// Befehl ohne Adresse und ohne Daten (z. B. Write Enable).
fn command(r: &Regs, instruction: u8) {
    wait_idle(r);
    r.fcr.write(|w| w.ctcf().set_bit());
    r.ccr.write(|w| unsafe {
        w.fmode()
            .bits(0) // indirect write
            .imode()
            .bits(1)
            .admode()
            .bits(0)
            .dmode()
            .bits(0)
            .instruction()
            .bits(instruction)
    });
    // Ohne Adresse und Daten startet der Transfer mit dem CCR-Write.
    while r.sr.read().tcf().bit_is_clear() {}
    r.fcr.write(|w| w.ctcf().set_bit());
    wait_idle(r);
}

fn read_status(r: &Regs) -> u8 {
    wait_idle(r);
    r.fcr.write(|w| w.ctcf().set_bit());
    r.dlr.write(|w| unsafe { w.dl().bits(0) }); // 1 Byte
    r.ccr.write(|w| unsafe {
        w.fmode()
            .bits(1) // indirect read
            .imode()
            .bits(1)
            .admode()
            .bits(0)
            .dmode()
            .bits(1)
            .instruction()
            .bits(CMD_READ_STATUS)
    });
    while r.sr.read().tcf().bit_is_clear() {}
    // Byte-Zugriff auf das Datenregister.
    let value = unsafe { core::ptr::read_volatile(r.dr.as_ptr() as *const u8) };
    r.fcr.write(|w| w.ctcf().set_bit());
    value
}

fn write_enable(r: &Regs) -> Result<(), FlashError> {
    command(r, CMD_WRITE_ENABLE);
    if read_status(r) & STATUS_WEL == 0 {
        Err(FlashError::WriteEnable)
    } else {
        Ok(())
    }
}

fn wait_write_done(r: &Regs, mut polls: u32) -> Result<(), FlashError> {
    loop {
        if read_status(r) & STATUS_WIP == 0 {
            return Ok(());
        }
        if polls == 0 {
            return Err(FlashError::Timeout);
        }
        polls -= 1;
    }
}

fn erase_sector_raw(r: &Regs, address: u32) -> Result<(), FlashError> {
    write_enable(r)?;
    wait_idle(r);
    r.fcr.write(|w| w.ctcf().set_bit());
    r.ccr.write(|w| unsafe {
        w.fmode()
            .bits(0)
            .imode()
            .bits(1)
            .admode()
            .bits(1)
            .adsize()
            .bits(2) // 24 bit
            .dmode()
            .bits(0)
            .instruction()
            .bits(CMD_SECTOR_ERASE_4K)
    });
    // Ohne Daten startet der Transfer mit dem AR-Write.
    r.ar.write(|w| unsafe { w.address().bits(address) });
    while r.sr.read().tcf().bit_is_clear() {}
    r.fcr.write(|w| w.ctcf().set_bit());
    wait_write_done(r, ERASE_TIMEOUT_POLLS)
}

/// Eine Seite programmieren. `data` darf die Seitengrenze nicht überschreiten.
fn program_page_raw(r: &Regs, address: u32, data: &[u8]) -> Result<(), FlashError> {
    write_enable(r)?;
    wait_idle(r);
    r.fcr.write(|w| w.ctcf().set_bit());
    r.dlr.write(|w| unsafe { w.dl().bits(data.len() as u32 - 1) });
    r.ccr.write(|w| unsafe {
        w.fmode()
            .bits(0)
            .imode()
            .bits(1)
            .admode()
            .bits(1)
            .adsize()
            .bits(2) // 24 bit
            .dmode()
            .bits(1) // Daten auf 1 Leitung (funktioniert ohne QE-Bit)
            .instruction()
            .bits(CMD_PAGE_PROGRAM)
    });
    r.ar.write(|w| unsafe { w.address().bits(address) });
    for &byte in data {
        while r.sr.read().ftf().bit_is_clear() {}
        unsafe { core::ptr::write_volatile(r.dr.as_ptr() as *mut u8, byte) };
    }
    while r.sr.read().tcf().bit_is_clear() {}
    r.fcr.write(|w| w.ctcf().set_bit());
    wait_write_done(r, PROGRAM_TIMEOUT_POLLS)
}

pub struct Flash {
    // Hält das Peripheral (und damit die Pin-Konfiguration) besessen.
    _qspi: Qspi<pac::QUADSPI>,
}

impl Flash {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        qspi_peripheral: pac::QUADSPI,
        clocks: &CoreClocks,
        clk: gpiof::PF10,
        ncs: gpiog::PG6,
        io0: gpiod::PD11,
        io1: gpiod::PD12,
        io2: gpioe::PE2,
        io3: gpiod::PD13,
        qspi_prec: rec::Qspi,
    ) -> Self {
        let _ncs = ncs.into_alternate::<10>().speed(Speed::VeryHigh);
        let sck = clk.into_alternate::<9>().speed(Speed::VeryHigh);
        let io0 = io0.into_alternate::<9>().speed(Speed::VeryHigh);
        let io1 = io1.into_alternate::<9>().speed(Speed::VeryHigh);
        let io2 = io2.into_alternate::<9>().speed(Speed::VeryHigh);
        let io3 = io3.into_alternate::<9>().speed(Speed::VeryHigh);

        let mut qspi = qspi_peripheral.bank1(
            (sck, io0, io1, io2, io3),
            3.MHz(),
            clocks,
            qspi_prec,
        );
        qspi.configure_mode(QspiMode::FourBit).unwrap();

        let r = regs();
        wait_idle(r);

        // FIFO-Schwelle 0: FTF ist gesetzt, sobald 1 Byte frei/verfügbar ist.
        r.cr.modify(|_, w| unsafe { w.fthres().bits(0) });

        // SoMLabs: FlashSize = 23, ChipSelectHighTime = 3 Zyklen, ClockMode = 0.
        r.dcr.write(|w| unsafe {
            w.ckmode()
                .clear_bit()
                .csht()
                .bits(2) // CSHT + 1 = 3 Zyklen
                .fsize()
                .bits(23) // 16 MiB
        });

        enter_memory_mapped(r);

        Self { _qspi: qspi }
    }

    pub const fn address() -> u32 {
        QSPI_MEMORY_ADDRESS
    }

    pub const fn size() -> usize {
        QSPI_FLASH_SIZE
    }

    /// Einen 4-KiB-Sektor löschen (dauert typisch ~50 ms, maximal einige
    /// hundert ms; Watchdog beachten!). `offset` muss sektorausgerichtet sein.
    pub fn erase_sector(&mut self, offset: usize) -> Result<(), FlashError> {
        if offset >= QSPI_FLASH_SIZE {
            return Err(FlashError::OutOfRange);
        }
        if offset % SECTOR_SIZE != 0 {
            return Err(FlashError::Unaligned);
        }
        let r = regs();
        leave_memory_mapped(r);
        let result = erase_sector_raw(r, offset as u32);
        enter_memory_mapped(r);
        result
    }

    /// Daten programmieren. Der Bereich muss vorher gelöscht sein (0xFF).
    /// Seitengrenzen (256 Byte) werden automatisch berücksichtigt.
    pub fn program(&mut self, offset: usize, data: &[u8]) -> Result<(), FlashError> {
        if offset
            .checked_add(data.len())
            .map_or(true, |end| end > QSPI_FLASH_SIZE)
        {
            return Err(FlashError::OutOfRange);
        }
        if data.is_empty() {
            return Ok(());
        }

        let r = regs();
        leave_memory_mapped(r);

        let mut result = Ok(());
        let mut address = offset;
        let mut rest = data;
        while !rest.is_empty() {
            let room = PAGE_SIZE - (address % PAGE_SIZE);
            let n = room.min(rest.len());
            result = program_page_raw(r, address as u32, &rest[..n]);
            if result.is_err() {
                break;
            }
            address += n;
            rest = &rest[n..];
        }

        enter_memory_mapped(r);
        result
    }

    pub fn read(&self, offset: usize, buffer: &mut [u8]) {
        assert!(offset
            .checked_add(buffer.len())
            .map_or(false, |end| end <= QSPI_FLASH_SIZE));

        let base = (QSPI_MEMORY_ADDRESS as usize + offset) as *const u8;

        for (i, byte) in buffer.iter_mut().enumerate() {
            *byte = unsafe { core::ptr::read_volatile(base.add(i)) };
        }
    }

    pub fn read_u8(&self, offset: usize) -> u8 {
        assert!(offset < QSPI_FLASH_SIZE);

        let address = (QSPI_MEMORY_ADDRESS as usize + offset) as *const u8;
        unsafe { core::ptr::read_volatile(address) }
    }

    pub fn read_u32(&self, offset: usize) -> u32 {
        assert!(offset
            .checked_add(4)
            .map_or(false, |end| end <= QSPI_FLASH_SIZE));

        let mut bytes = [0u8; 4];
        self.read(offset, &mut bytes);
        u32::from_le_bytes(bytes)
    }
}

// ---------------------------------------------------------------------------
// Konfigurationsablage
// ---------------------------------------------------------------------------
//
// Die Konfiguration liegt als Byte-Block (z. B. postcard-serialisiert) in
// zwei abwechselnd genutzten Slots (je 2 Sektoren) am Ende des Flash. Jeder Slot hat
// einen Header mit Magic, Sequenznummer, Länge und CRC32. Beim Speichern
// wird der ältere Slot gelöscht, zuerst die Daten und ZULETZT der Header
// geschrieben. Bricht die Spannung dazwischen weg, bleibt der andere Slot
// gültig.

const CONFIG_MAGIC: u32 = 0x3147_4643; // "CFG1" (little endian)
const HEADER_SIZE: usize = 16;

/// Jeder Slot besteht aus 2 Sektoren (8 KiB). Beim Speichern werden nur die
/// Sektoren gelöscht, die für die aktuelle Länge nötig sind (meist 1).
const SLOT_SIZE: usize = 2 * SECTOR_SIZE;
const SLOT_A: usize = QSPI_FLASH_SIZE - 2 * SLOT_SIZE;
const SLOT_B: usize = QSPI_FLASH_SIZE - SLOT_SIZE;

/// Maximale Größe eines Konfigurationsblocks in Bytes.
pub const CONFIG_MAX_LEN: usize = SLOT_SIZE - HEADER_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    TooLarge,
    Flash(FlashError),
    /// Nach dem Schreiben war der Slot nicht gültig lesbar.
    VerifyFailed,
}

impl From<FlashError> for ConfigError {
    fn from(e: FlashError) -> Self {
        ConfigError::Flash(e)
    }
}

#[derive(Clone, Copy)]
struct SlotInfo {
    seq: u32,
    len: usize,
}

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// `a` ist neuer als `b` (Überlauf-sicher).
fn newer(a: u32, b: u32) -> bool {
    a.wrapping_sub(b) as i32 > 0
}

fn read_slot(flash: &Flash, base: usize) -> Option<SlotInfo> {
    let mut h = [0u8; HEADER_SIZE];
    flash.read(base, &mut h);

    let magic = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    let seq = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
    let len = u32::from_le_bytes([h[8], h[9], h[10], h[11]]) as usize;
    let stored_crc = u32::from_le_bytes([h[12], h[13], h[14], h[15]]);

    if magic != CONFIG_MAGIC || len > CONFIG_MAX_LEN {
        return None;
    }

    let mut crc = 0xFFFF_FFFF;
    let mut chunk = [0u8; 64];
    let mut pos = 0;
    while pos < len {
        let n = (len - pos).min(chunk.len());
        flash.read(base + HEADER_SIZE + pos, &mut chunk[..n]);
        crc = crc32_update(crc, &chunk[..n]);
        pos += n;
    }

    if !crc != stored_crc {
        return None;
    }
    Some(SlotInfo { seq, len })
}

/// Aktuelle Konfiguration lesen. Gibt die Länge zurück oder `None`, wenn
/// keine gültige Konfiguration vorhanden ist (oder `out` zu klein ist).
pub fn config_load(flash: &Flash, out: &mut [u8]) -> Option<usize> {
    let a = read_slot(flash, SLOT_A);
    let b = read_slot(flash, SLOT_B);

    let (base, info) = match (a, b) {
        (Some(a), Some(b)) => {
            if newer(b.seq, a.seq) {
                (SLOT_B, b)
            } else {
                (SLOT_A, a)
            }
        }
        (Some(a), None) => (SLOT_A, a),
        (None, Some(b)) => (SLOT_B, b),
        (None, None) => return None,
    };

    if info.len > out.len() {
        return None;
    }
    flash.read(base + HEADER_SIZE, &mut out[..info.len]);
    Some(info.len)
}

/// Konfiguration speichern (löscht und beschreibt den älteren Slot).
pub fn config_save(flash: &mut Flash, data: &[u8]) -> Result<(), ConfigError> {
    if data.len() > CONFIG_MAX_LEN {
        return Err(ConfigError::TooLarge);
    }

    let a = read_slot(flash, SLOT_A);
    let b = read_slot(flash, SLOT_B);

    let (target, seq) = match (a, b) {
        (Some(a), Some(b)) => {
            if newer(b.seq, a.seq) {
                (SLOT_A, b.seq.wrapping_add(1))
            } else {
                (SLOT_B, a.seq.wrapping_add(1))
            }
        }
        (Some(a), None) => (SLOT_B, a.seq.wrapping_add(1)),
        (None, Some(b)) => (SLOT_A, b.seq.wrapping_add(1)),
        (None, None) => (SLOT_A, 1),
    };

    let crc = !crc32_update(0xFFFF_FFFF, data);

    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&CONFIG_MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&seq.to_le_bytes());
    header[8..12].copy_from_slice(&(data.len() as u32).to_le_bytes());
    header[12..16].copy_from_slice(&crc.to_le_bytes());

    // Nur so viele Sektoren löschen, wie Header + Daten brauchen (spart Zeit).
    // Alte Reste in nicht gelöschten Sektoren stören nicht, die Länge im
    // Header begrenzt, was gelesen und per CRC geprüft wird.
    let sectors = (HEADER_SIZE + data.len() + SECTOR_SIZE - 1) / SECTOR_SIZE;
    for i in 0..sectors {
        flash.erase_sector(target + i * SECTOR_SIZE)?;
    }
    flash.program(target + HEADER_SIZE, data)?; // erst die Daten ...
    flash.program(target, &header)?; // ... zuletzt der Header (Commit)

    if read_slot(flash, target).is_none() {
        return Err(ConfigError::VerifyFailed);
    }
    Ok(())
}