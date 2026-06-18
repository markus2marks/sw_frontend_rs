use crate::{driver::*, idle_loop::OutputPins, DevController, DevView, IdleLoop, DEVICE_CONST};
use corelib::{
    basic_config::{MAX_RX_FRAMES, MAX_TX_FRAMES, VDA},
    spsc_queue, CanDispatch, CoreModel, QIdleEvents, QPersistenceItems, QRxFrames, QTxFrames, QTxIrqFrames,
};
use cortex_m::peripheral::Peripherals as CorePeripherals;
use defmt::*;
use heapless::{mpmc::MpMcQueue, spsc::Queue};
use stm32h7xx_hal::{
    adc,
    dma::dma::StreamsTuple,
    independent_watchdog::IndependentWatchdog,
    pac::Peripherals as DevicePeripherals,
    prelude::*,
    rcc::{rec, rec::AdcClkSel, ResetEnable},
};
use stm32h7xx_hal::dsi::{
    DsiCmdModeTransmissionKind, DsiHost, DsiInterrupts, DsiMode, DsiPhyTimers,
    DsiVideoMode, LaneCount,
};
use stm32h7xx_hal::dsi::{ColorCoding, DsiChannel, DsiConfig, DsiPllConfig};

use embedded_display_controller::DisplayConfiguration;
use crate::driver::st7701_dsi::St7701;
use embedded_display_controller::DisplayController;
use stm32h7xx_hal::ltdc as hal_ltdc;
use stm32h7xx_hal::rcc::PllConfigStrategy;
// Remember to use correct display controller orientation, Portrait in this case
pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 480;

pub const DISPLAY_CONFIGURATION: DisplayConfiguration = DisplayConfiguration {
    active_width: WIDTH as _,
    active_height: HEIGHT as _,
    h_back_porch: 34,
    h_front_porch: 34,
    v_back_porch: 15,
    v_front_porch: 16,
    h_sync: 2,
    v_sync: 1,
    h_sync_pol: false,
    v_sync_pol: false,
    not_data_enable_pol: false,
    pixel_clock_pol: false,
};

pub type DevCanDispatch = CanDispatch<VDA, 8, MAX_TX_FRAMES, MAX_RX_FRAMES, DevRng>;

pub const H7_HCLK: u32 = 200_000_000;

pub fn hw_init(
    dp: DevicePeripherals,
    mut cp: CorePeripherals,
) -> (
    DevCanDispatch,
    CanRx,
    CanTx<MAX_TX_FRAMES>,
    CoreModel,
    DevController,
    IdleLoop,
    Keyboard,
    MonoTimer,
    NmeaRx,
    NmeaTx,
    Sound,
) {
    // Setup ----------> the queues

    // This queue transports the can bus frames from the can dispatcher to the irq routine.
    let (p_tx_irq_frames, c_tx_irq_frames) = spsc_queue!(QTxIrqFrames<MAX_TX_FRAMES>);
    // This queue transports the can bus frames from the can dispatcher to the controller.
    let (p_rx_frames, c_rx_frames) = spsc_queue!(QRxFrames<MAX_RX_FRAMES>);
    // This queue transports the can bus frames from the controller to the can dispatcher.
    let (p_tx_frames, c_tx_frames) = spsc_queue!(QTxFrames<MAX_TX_FRAMES>);
    // This queue routes the StorageItems from the controller to the idle loop.
    let (p_idle_events, c_idle_events) = spsc_queue!(QIdleEvents);
    // This queue routes the PersistenceItems from the idle_loop to the controller loop.
    let (p_persistence_items, c_persistence_items) = spsc_queue!(QPersistenceItems);

    // This queue routes the events to the controller.
    static Q_EVENTS: QEvents = MpMcQueue::new();

    // Unfortantly adc does not work on vos3
    // Constrain and freeze power, save a little bit power, optimum is at vos3 / 200 MHz
    // let pwrcfg = dp.PWR.constrain().vos3().freeze();
    // Constrain and Freeze power
    let pwrcfg = dp.PWR.constrain().freeze();
    let ltdc_freq = 18_400.kHz();   
    let mut ccdr = dp
        .RCC
        .constrain()
        .use_hse(16.MHz())
         .sys_ck(400.MHz())
        // FMC will run at 100MHz, as this clock is further divided by 2
         .pll2_p_ck(200.MHz())
         .pll2_q_ck(200.MHz() / 2)
        .pll2_r_ck(100.MHz())
        .pll2_strategy(PllConfigStrategy::Iterative)
        // LTDC
        .pll3_p_ck(92.MHz())
        .pll3_q_ck(23.MHz())
        .pll3_r_ck(ltdc_freq)
        .pll3_strategy(PllConfigStrategy::Iterative)
        .freeze(pwrcfg, &dp.SYSCFG);

    // Enable cortex m7 cache and cyclecounter
    cp.SCB.enable_icache();
    cp.DWT.enable_cycle_counter();

    ccdr.peripheral.kernel_adc_clk_mux(AdcClkSel::Per);

    // Setup ----------> system timer
    let mono = MonoTimer::new(dp.TIM2, ccdr.peripheral.TIM2, &ccdr.clocks);
    let mut delay = Delay {};

    // Take ownership of GPIO ports
    let gpioa = dp.GPIOA.split(ccdr.peripheral.GPIOA);
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);
    let gpiod = dp.GPIOD.split(ccdr.peripheral.GPIOD);
    let gpiog = dp.GPIOG.split(ccdr.peripheral.GPIOG);
    let gpioh = dp.GPIOH.split(ccdr.peripheral.GPIOH);
    let gpioi = dp.GPIOI.split(ccdr.peripheral.GPIOI);
    let gpioj = dp.GPIOJ.split(ccdr.peripheral.GPIOJ);
    let gpiok = dp.GPIOK.split(ccdr.peripheral.GPIOK);

    // Switch LCD Backlight off
    let mut backlight_control = gpioc.pc6.into_push_pull_output();
    backlight_control.set_high();



    // Setup ----------> Frame buffer, Display

       
        let mut ltdc_2 = hal_ltdc::Ltdc::new(dp.LTDC, ccdr.peripheral.LTDC, &ccdr.clocks);
        ltdc_2.init(DISPLAY_CONFIGURATION);

        let dsi_pll_config = unsafe { DsiPllConfig::manual(32, 1, 1, 4) };

        let hse_freq = 16.MHz();
        let dsi_config: DsiConfig = DsiConfig {
            mode: DsiMode::Video {
            // mode: DsiVideoMode::NonBurstWithSyncEvents,
            mode: DsiVideoMode::Burst,
            },
            lane_count: LaneCount::SingleLane,
            channel: DsiChannel::Ch0,
            hse_freq,
            ltdc_freq,
            interrupts: DsiInterrupts::None,
            color_coding_host: ColorCoding::EighteenBitsConfig1,
            color_coding_wrapper: ColorCoding::EighteenBitsConfig2,
            lp_size: 0, // for OTM8009A
            vlp_size: 0,
        };

        let mut dsi_host = DsiHost::init(
            dsi_pll_config,
            DISPLAY_CONFIGURATION,
            dsi_config,
            dp.DSIHOST,
            ccdr.peripheral.DSI,
            &ccdr.clocks,
        ).expect("DSI host failed to init");

        dsi_host.set_command_mode_transmission_kind(
            DsiCmdModeTransmissionKind::AllInLowPower,
        );
        dsi_host.start();
        dsi_host.enable_bus_turn_around(); // Must be before read attempts

        dsi_host.configure_phy_timers(DsiPhyTimers {
            dataline_hs2lp: 16,
            dataline_lp2hs: 26,
            clock_hs2lp: 29,
            clock_lp2hs: 34,
            dataline_max_read_time: 2,
            stop_wait_time: 0,
        });


        let mut glass_ctrl= St7701::new();
        glass_ctrl.init(&mut dsi_host, &mut delay);

    
     // Setup ----------> The front key interface
    let keyboard = {
        let keyboard_pins = KeyboardPins::new(gpioa.pa3);
        let input_pins = InputPins::new(gpiob.pb12, gpiob.pb13, gpiob.pb14, gpioh.ph7);
        let enc1_res = Enc1Res::new(ccdr.peripheral.TIM5, dp.TIM5, gpioa.pa0, gpioa.pa1);
        let enc2_res = Enc2Res::new(ccdr.peripheral.TIM3, dp.TIM3, gpiob.pb4.into(), gpioc.pc7);
        Keyboard::new(
            keyboard_pins, 
            input_pins, 
            enc1_res, 
            enc2_res, 
            &Q_EVENTS
        )
    };

    // Setup ----------> Canbus
    let (tx_can, rx_can) = {
        let fdcan_prec = ccdr
            .peripheral
            .FDCAN
            .kernel_clk_mux(rec::FdcanClkSel::Pll1Q);
        let fdcan_1 = dp.FDCAN1;
        init_can(fdcan_prec, fdcan_1, gpiob.pb8, gpiob.pb9, c_tx_irq_frames)
    };

    let rng = dp.RNG.constrain(ccdr.peripheral.RNG, &ccdr.clocks);
    let rnd = DevRng::new(rng);

    let mut can_dispatch = CanDispatch::new(rnd, p_tx_irq_frames, p_rx_frames, c_tx_frames);
    can_dispatch.set_legacy_filter(0x100, 0x11f).unwrap();
    can_dispatch.set_legacy_filter(0x282, 0x282).unwrap(); // Vario display master device avg_climb_rates
    let _ = can_dispatch.set_object_id_filter(2); // Sensorbox
    let _ = can_dispatch.set_object_id_filter(3); // Gps

    // Setup ----------> CoreModel
    let mut core_model = CoreModel::new(&&DEVICE_CONST, uuid());       

    
    // Setup ----------> controller
    let mut dev_controller = {
        let mut adc1 = adc::Adc::adc1(
            dp.ADC1,
            4.MHz(),
            &mut delay,
            ccdr.peripheral.ADC12,
            &ccdr.clocks,
        );
        adc1.calibrate();

        DevController::new(
            &mut core_model,
            &Q_EVENTS,
            p_idle_events,
            c_persistence_items,
            p_tx_frames,
            c_rx_frames,
            adc1.enable(),
            gpioa.pa6,
            gpiob.pb1,
        )
    };

    // Setup ----------> Idleloop
    let idle_loop = {
        let mut wp = gpioc.pc5.into_push_pull_output();
        wp.set_low(); // Always enable writing to the eeprom

        let scl = gpiob.pb6.into_alternate_open_drain();
        let sda = gpiob.pb7.into_alternate_open_drain();
        let i2c = dp
            .I2C1
            .i2c((scl, sda), 400.kHz(), ccdr.peripheral.I2C1, &ccdr.clocks);

        let pins = SdcardPins(
            gpioc.pc12,
            gpiod.pd2,
            gpioc.pc8,
            gpioc.pc9,
            gpioc.pc10,
            gpioc.pc11,
            gpioa.pa15.into(),
        );

        // Init filesystem if sdcard available
        let _ = FileSys::new(pins, dp.SDMMC1, ccdr.peripheral.SDMMC1, &ccdr.clocks).ok();

        // Init reset watch and create entry in PANIC.LOG if watchdog reset
        ResetWatch::new();

        let watchdog = IndependentWatchdog::new(dp.IWDG1);

        let output_pins = OutputPins::new(gpiog.pg2, gpiok.pk2);

        let idle_loop = IdleLoop::new(
            output_pins,
            i2c,
            watchdog,
            c_idle_events,
            p_persistence_items,
            &Q_EVENTS,
            &mut core_model,
            &mut dev_controller,
        );

        // switch LCD backlight on, after eventually firmware update (avoids flickering)
        backlight_control.set_low();

        idle_loop
    };

    let streams = StreamsTuple::new(dp.DMA1, ccdr.peripheral.DMA1);

    // Setup ----------> Sound
    let sound = {
        let dac = dp.DAC.dac(gpioa.pa4, ccdr.peripheral.DAC12);
        let dac = dac.calibrate_buffer(&mut delay);
        let _ = ccdr.peripheral.TIM6.enable();
        Sound::new(dac, dp.TIM6, streams.0)
    };

    // Setup ----------> Nmea
    let (nmea_tx, nmea_rx) = NmeaTxRx::new(
        streams.1,
        streams.2,
        gpioa.pa9,
        gpiob.pb15,
        dp.USART1,
        ccdr.peripheral.USART1,
        &ccdr.clocks,
    );

    // set time of controller to current time
    dev_controller.set_ms(timestamp_ms());

    info!("Larus init finished");

    (
        can_dispatch,
        rx_can,
        tx_can,
        core_model,
        dev_controller,
        idle_loop,
        keyboard,
        mono,
        nmea_rx,
        nmea_tx,
        sound,
    )
}
