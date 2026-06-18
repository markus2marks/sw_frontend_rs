use stm32h7xx_hal::{
    gpio::{self, Speed},
    pac,
    rcc::{rec, CoreClocks, ResetEnable},
};

pub struct Ltdc {}

impl Dsihost {
    pub fn init(
        _dsi: pac::DSI,
        prec: rec::Ltdc,
        clocks: &CoreClocks,
    ) -> Self {
        
    }

}