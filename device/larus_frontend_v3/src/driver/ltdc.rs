use super::CLUT_COLORS;
use stm32h7xx_hal::{
    gpio::{self, Speed},
    pac,
    rcc::{rec, CoreClocks, ResetEnable},
};


pub struct Ltdc {}

impl Ltdc {
    pub fn init(
        _ltdc: pac::LTDC,
        prec: rec::Ltdc,
        clocks: &CoreClocks,
    ) -> Self {
        // The pll3_r_ck (== pixel clock) of 9 Mhz leads to a frame rate of slightly more
        // than 30 Hz
        clocks.pll3_r_ck().expect("PLL3-R not configured"); // pll3 must run
        prec.enable().reset(); // enable peripheral

        // unsafe is unavoidable and ok during initialization of the hardware
        //
        // the details are taken from
        // https://github.com/larus-breeze/sw_frontend/tree/hw_frontend_stm32h743_hal_lqfp100_rgb18_st7701
        unsafe {
            // configure ltdc peripheral
            let ltdc = &(*pac::LTDC::ptr());
            ltdc.sscr.write(|w| w.vsh().bits(0x05));
            ltdc.sscr.modify(|_, w| w.hsw().bits(0x13));

            ltdc.bpcr.write(|w| w.avbp().bits(0x0F));
            ltdc.bpcr.modify(|_, w| w.ahbp().bits(0x17));

            ltdc.awcr.write(|w| w.aah().bits(0x01eF));
            ltdc.awcr.modify(|_, w| w.aaw().bits(0x01F7));

            ltdc.twcr.write(|w| w.totalh().bits(0x01f9));
            ltdc.twcr.modify(|_, w| w.totalw().bits(0x021F));

            ltdc.ier
                .modify(|_, w| w.terrie().set_bit().fuie().set_bit());

            ltdc.gcr.write(|w| w.hspol().set_bit());
            ltdc.gcr.modify(|_, w| w.vspol().set_bit());
            ltdc.gcr.modify(|_, w| w.depol().clear_bit());
            ltdc.gcr.modify(|_, w|w.pcpol().clear_bit());
            ltdc.gcr.modify(|_, w| w.ltdcen().set_bit());
            Ltdc {}
        }
    }

    pub fn init_layer(&mut self, frame_bauffer: *const u8) {
        unsafe {
            // write color lookup table
            let ltdc = &(*pac::LTDC::ptr());
            for clut_entry in CLUT_COLORS {
                ltdc.layer1.clutwr.write(|w| w.bits(clut_entry));
            }
            ltdc.layer1.cr.modify(|_, w| w.cluten().enabled()); // enable clut
            ltdc.srcr.write(|w| w.imr().set_bit());

            // configure the layer used
            ltdc.layer1.whpcr.write(|w| w.bits(0x01f7_0018));
            ltdc.layer1.wvpcr.write(|w| w.bits(0x01ed_000e));
            ltdc.layer1.pfcr.write(|w| w.bits(0x05));
            ltdc.layer1.bfcr.write(|w| w.bits(0x0000_0405));
            ltdc.layer1.cfblr.write(|w| w.bits(0x01e0_01e7));
            ltdc.layer1.cfblnr.write(|w| w.bits(0x0000_01e0));

            ltdc.layer1
                .cfbar
                .write(|w| w.cfbadd().bits(frame_bauffer as u32));
            ltdc.layer1.cr.modify(|_, w| w.len().enabled()); // enable layer 1
            ltdc.srcr.write(|w| w.imr().set_bit());

            // activate ltdc peripheral
            ltdc.gcr.modify(|_, w| w.ltdcen().set_bit()); // enable ltdc
        }
    }

    pub fn set_frame_buffer(&mut self, frame_bauffer: *const u8) {
        unsafe {
            let ltdc = &(*pac::LTDC::ptr());
            ltdc.layer1
                .cfbar
                .write(|w| w.cfbadd().bits(frame_bauffer as u32));
            ltdc.srcr.write(|w| w.vbr().set_bit());
        }
    }
}
