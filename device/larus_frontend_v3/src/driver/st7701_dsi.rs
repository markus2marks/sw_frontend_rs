use embedded_display_controller::dsi::{DsiHostCtrlIo, DsiWriteCommand};
use embedded_hal::blocking::delay::DelayMs;

pub struct St7701;

impl St7701 {
    pub fn new() -> Self {
        Self {}
    }

    fn write_cmd<D: DsiHostCtrlIo>(dsi: &mut D, cmd: u8, data: &[u8]) {
        let _ = dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: cmd,
            data,
        });
    }

    fn write_short<D: DsiHostCtrlIo>(dsi: &mut D, cmd: u8, data: u8) {
        let _ = dsi.write(DsiWriteCommand::DcsShortP1 {
            arg: cmd,
            data,
        });
    }
    
    pub fn init<D: DsiHostCtrlIo>(&mut self, dsi: &mut D, delay: &mut impl DelayMs<u32>,  ) 
    {
        // =========================================================
        // RESET (extern!)
        // =========================================================
              dsi.write(DsiWriteCommand::DcsShortP1 {
            arg: 0x01,
            data: 0x00,
        }).ok();
        delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 {
            arg: 0x11,
            data: 0x00,
        }).ok();
        delay.delay_ms(150);

        // =========================================================
        // SLEEP OUT
        // =========================================================
delay.delay_ms(150);
        //         dsi.write(DsiWriteCommand::DcsShortP1 {
        //     arg: 0x23,
        //     data: 0x00,
        // }).ok();
        // delay.delay_ms(150);
        // =========================================================
        // BK13
        // =========================================================
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xFF,
            data: &[0x77, 0x01, 0x00, 0x00, 0x13],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xEF, data: 0x08 });
        delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0x3A, data: 0x60 });
delay.delay_ms(150);
        // =========================================================
        // BK10
        // =========================================================
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xFF,
            data: &[0x77, 0x01, 0x00, 0x00, 0x10],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xC0,
            data: &[0x3B, 0x00],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xC1,
            data: &[0x0A, 0x0A],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xC2,
            data: &[0x37, 0x02],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xC6, data: 0x21 });
        delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xCC, data: 0x30 });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xB0,
            data: &[
                0xC0, 0x54, 0x5C, 0x0D,
                0x51, 0x06, 0x09, 0x08,
                0x07, 0x24, 0x03, 0x11,
                0x0F, 0xAC, 0xB5, 0x7F,
            ],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xB1,
            data: &[
                0xC0, 0x54, 0x5C, 0x0E,
                0x11, 0x07, 0x0A, 0x09,
                0x08, 0x24, 0x04, 0x51,
                0x10, 0xAD, 0x75, 0x7F,
            ],
        });
delay.delay_ms(150);
        // =========================================================
        // BK11
        // =========================================================
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xFF,
            data: &[0x77, 0x01, 0x00, 0x00, 0x11],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB0, data: 0x7D });
        delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB1, data: 0x33 });
        delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB2, data: 0x87 });
        delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB3, data: 0x80 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB5, data: 0x45 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB7, data: 0x87 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB8, data: 0x33 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xB9, data: 0x10 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xBB, data: 0x03 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xC0, data: 0x03 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xC1, data: 0x78 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xC2, data: 0x78 });
        dsi.write(DsiWriteCommand::DcsShortP1 { arg: 0xD0, data: 0x88 });

        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE0,
            data: &[0x00, 0x18, 0x00, 0x00, 0x00, 0x20],
        });

        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE1,
            data: &[
                0x05, 0xA0, 0x00, 0xA0,
                0x04, 0x0A, 0x00, 0xA0,
                0x00, 0x44, 0x44,
            ],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE2,
            data: &[
                0x11, 0x11, 0x44, 0x44,
                0xEA, 0xA0, 0x00, 0x00,
                0xE9, 0xA0, 0x00, 0x00,
            ],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE3,
            data: &[0x00, 0x00, 0x11, 0x11],
        });

        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE4,
            data: &[0x44, 0x44],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE5,
            data: &[
                0x06, 0xE5, 0xD8, 0xA0,
                0x08, 0xE7, 0xD8, 0xA0,
                0x0A, 0xE9, 0xD8, 0xA0,
                0x0C, 0xEB, 0xD8, 0xA0,
            ],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE6,
            data: &[0x00, 0x00, 0x11, 0x11],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE7,
            data: &[0x44, 0x44],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xE8,
            data: &[
                0x05, 0xE4, 0xD8, 0xA0,
                0x07, 0xE6, 0xD8, 0xA0,
                0x09, 0xE8, 0xD8, 0xA0,
                0x0B, 0xEA, 0xD8, 0xA0,
            ],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xEB,
            data: &[0x02, 0x00, 0xE4, 0xE4, 0x88, 0x00, 0x10],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xEC,
            data: &[0x3D, 0x02, 0x00],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xED,
            data: &[
                0x20, 0x76, 0x54, 0x98,
                0xBA, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF, 0xFF, 0xAB,
                0x89, 0x45, 0x67, 0x02,
            ],
        });
delay.delay_ms(150);
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xEF,
            data: &[
                0x08, 0x08, 0x08,
                0x45, 0x3F, 0x54,
            ],
        });
delay.delay_ms(150);
        // =========================================================
        // BACK TO USER BANK
        // =========================================================
        dsi.write(DsiWriteCommand::DcsLongWrite {
            arg: 0xFF,
            data: &[0x77, 0x01, 0x00, 0x00, 0x00],
        });
delay.delay_ms(150);
        delay.delay_ms(20);

        // =========================================================
        // DISPLAY ON
        // =========================================================
        dsi.write(DsiWriteCommand::DcsShortP1 {
            arg: 0x29,
            data: 0x00,
        });

        delay.delay_ms(120);
    }
}