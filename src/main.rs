//! Main.rs
//! Copyright © 2026 Sean Springer
//! [This program is licensed under the "MIT License"]
//! Please see the file LICENSE in the source distribution of this software for license terms.
//!
//! Control the HSV of a RGB LED to allow for full range of colors to be emitted.
//!
//! The Hue (H), Saturation (S), and Value (V) option can be toggled using the A or B buttons on the MB2.
//! Once selected, the HSV parameter can be adjusted via the 10k potentiometer.
//!
//! This program requires the MB2 be connected to the Micro:bit GPIO edge connector board, a potentiometer connected to
//! ADC pins, and a RGB LED. A small bread board was used to make these connections.
//!
//! The following code assumes the following connections:
//! 1. Red LED connected to P0_10 (e08)
//! 2. Green LED connected to P0_09 (e09)
//! 3. Blue LED connected to P1_02 (e16)
//! 4. Pot output connected to P0_04 (e16)
//!
//! Note: the adc is sampled at ~40usecs and is averaged to a 100msec refresh rate. Most interactions are handled via
//! interrupts while the main event loop accumulates and averages the pot ADC value.
//!
//! The RGB physical color is controled by a custom-made, Timer-based pulse width modulation (PWM) of each RGB pin voltage

#![no_std]
#![no_main]

mod utils;

use panic_rtt_target as _;
use rtt_target::rtt_init_print;
//use rtt_target::rprintln;
use cortex_m_rt::entry;
use microbit::{
    board::Board,
    display::nonblocking::Display,
    hal::{
        Timer,
        gpio::{
            Floating, Input, Level, Output, PushPull,
            p0::{P0_04, P0_09, P0_10},
            p1::P1_02,
        },
        gpiote::Gpiote,
        saadc,
        saadc::{Saadc, SaadcConfig},
    },
    pac::{Interrupt, NVIC, TIMER0, TIMER1, TIMER2, TIMER3, interrupt},
};

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};

use crate::utils::color_control::{ColorControler, STARTING_HSV};
use crate::utils::hsv_display::{HSVDisplay, HSVPage};
use critical_section_lock_mut::LockMut;

/// Type definitions - the top 4 definitions are used in color_control.rs while
/// the last (PotType) is referenced here just for convience in assigning the hardware
type RedPinType = P0_10<Output<PushPull>>; //e08
type GreenPinType = P0_09<Output<PushPull>>; //e09
type BluePinType = P1_02<Output<PushPull>>; //e16
type ColorTimer = Timer<TIMER2>;
type PotType = P0_04<Input<Floating>>; //e02

/// Globals Constants
const DEBOUNCE_TIME: u32 = 100 * 1_000_000 / 1000; // 100ms at 1MHz count rate.
const MAX_ADC_VALUE: i16 = (1_i16 << 14) - 1_i16; // max value of the ADC output
const MAX_ADC_THRESHOLD: f32 = MAX_ADC_VALUE as f32 * 0.98; // 16,053; clamp upper ADC bound slightly below max (98%)
const MIN_ADC_THRESHOLD: f32 = 10f32; // clamp lower ADC bound to 10
const REFRESH_RATE_MS: u32 = 100; // update rate of the ADC
const TIMER_TICKS_PER_MS: u32 = 1_000_000u32 / 1000; // TIMER peripheral clock rate in msecs
const REFRESH_RATE_TICKS: u32 = TIMER_TICKS_PER_MS * REFRESH_RATE_MS; // 100ms in TIMER clock ticks

// Global Mutexes for interupt handlers
static GPIOTE_PERIPHERAL: LockMut<Gpiote> = LockMut::new(); // GPIOTE for button presses
static DEBOUNCE_TIMER: LockMut<Timer<TIMER1>> = LockMut::new(); // Debounce TIMER to protect button presses
static ADC_ACC_TIMER: LockMut<Timer<TIMER3>> = LockMut::new(); // ADC accumulator timer - indicates when to stop co-adding and to average
static DISPLAY: LockMut<HSVDisplay<TIMER0>> = LockMut::new(); // non-blocking display update timer
static COLOR_CONTROLER: LockMut<ColorControler> = LockMut::new(); // set the RGB pin states based upon the HSV parameter and ADC result
static ADC_ACCUMULATOR_VALUE: AtomicU32 = AtomicU32::new(0); // ADC co-adding sum: can accumulate max adc value for more than 5 seconds at 20us sample rate before overflow
static ADC_READY_READ: AtomicBool = AtomicBool::new(false); // indicator to main loop that ADC is ready to be averaged and update HSV

/// TIMER0 Interupt handler (nrf52833 Peripheral Vecotr Table Entry #8)
///
/// Handles the Non-Blocking Display Timer interrupt. This timeout is set internally by the display::nonblocking::Display module.
/// HSVDisplay<T>::display() fn is a simple wrapper around the display::nonblocking::Display::handle_display_event fn.
#[interrupt]
fn TIMER0() {
    DISPLAY.with_lock(|display| {
        display.handle_display_event();
    });
}

/// TIMER2 Interupt handler (nrf52833 Peripheral Vecotr Table Entry #10)
///
/// Handles the ColorControler timer interrupt which changes the RGB LED color at the 100ms refresh rate
#[interrupt]
fn TIMER2() {
    COLOR_CONTROLER.with_lock(|color_controler| {
        color_controler.render();
    });
}

/// TIMER3 Interupt handler (nrf52833 Peripheral Vecotr Table Entry #26)
///
/// When TIMER3 interrupts, it indicates that the ADC Accumulator time has completed and so
/// it is time to finish adding the ADC results and to average the accumulation to a final value.
/// The ADC_READY_READ atomic is set to true which will signal the main loop to average and pass the
/// final ADC result to the ColorControler instance
#[interrupt]
fn TIMER3() {
    ADC_ACC_TIMER.with_lock(|adc_acc_timer| {
        ADC_READY_READ.store(true, SeqCst);
        adc_acc_timer.start(REFRESH_RATE_TICKS);
    });
}

/// GPIOTE Interrupt handler (nrf52833 Peripheral Vector Table Entry #6)
///
/// Handles interrupts originating from either the A or B btn press with anti-bouncing logic.
/// First, this interupt handler checks that the debouncer timer has cooled down and, if so, will
/// update the 5x5 LED matrix on the MB2 to represent the HSV setting
#[interrupt]
fn GPIOTE() {
    // check for bouncing using a 100ms timer based coolddown:
    let mut debounced = false;
    DEBOUNCE_TIMER.with_lock(|debounce_timer| {
        if debounce_timer.read() == 0 {
            debounced = true;
            debounce_timer.start(DEBOUNCE_TIME);
        }
    });

    // grab a mutable reference to the Gpiote instance, determine which button sent the signal,
    // reset the interrupt, and update the LED display HSV if debounced timer as timed out
    GPIOTE_PERIPHERAL.with_lock(|gpiote| {
        if gpiote.channel0().is_event_triggered() {
            //A button press
            gpiote.channel0().reset_events();
            if debounced {
                DISPLAY.with_lock(|display| {
                    display.left();
                    display.render();
                });
            }
        } else if gpiote.channel1().is_event_triggered() {
            //B button press
            gpiote.channel1().reset_events();
            if debounced {
                DISPLAY.with_lock(|display| {
                    display.right();
                    display.render();
                });
            }
        }
    });
}

/// fn init() is called once immediately prior to the main event loop to initialize the
/// global MUTEX instances.
///  
/// 1. initialize the 5x5 LED display to H
/// 2. initialize the ColorControler instance physical pin states to illuminate the RGB LED
/// 3. initialize the ADC accumulator timer
fn init() {
    DISPLAY.with_lock(|display| {
        display.render();
    });

    COLOR_CONTROLER.with_lock(|color_controler| {
        color_controler.render();
    });

    ADC_ACC_TIMER.with_lock(|adc_acc_timer| {
        adc_acc_timer.start(REFRESH_RATE_TICKS);
    });
}

/// Entry point
///
/// Set up the peripherals to be used,initialize the GPIO Events to trigger, setup the NVIC,
/// and accumulates the ADC results and then averages them when the ADC_ACC_TIMER has signaled (via ADC_READY_READ atomic)
/// that the refresh rate time has elapsed.
///
/// 1. Setup the Non-Blocking 5x5 LED Display on the MB2
/// 2. Setup the RGB LED pins and ColorControler struct
/// 3. Setup the ADC sampling of the pot voltage
/// 4. Setup the A/B Buttons with GPIOTE interrupts
/// 5. Setup and clear the NVIC states
/// 6. Start main event loop - accumulate pot ADC results and average when triggered, passing the averaged result
///    to the ColorControler struct to change the rgb pin states
#[entry]
fn main() -> ! {
    rtt_init_print!();

    let board = Board::take().unwrap();

    // setup display
    let display = Display::new(board.TIMER0, board.display_pins);
    let mut debounce_timer = Timer::new(board.TIMER1);
    let display = HSVDisplay::new(display);
    DISPLAY.init(display);
    debounce_timer.enable_interrupt(); //setup debounce timer interupts
    debounce_timer.reset_event();
    DEBOUNCE_TIMER.init(debounce_timer);

    // setup RGB pins
    let color_timer: ColorTimer = Timer::new(board.TIMER2);
    let red: RedPinType = board.edge.e08.into_push_pull_output(Level::High); //High means off for the LED
    let green: GreenPinType = board.edge.e09.into_push_pull_output(Level::High); //High means off for the LED
    let blue: BluePinType = board.edge.e16.into_push_pull_output(Level::High);
    let color_controler: ColorControler =
        ColorControler::new(STARTING_HSV, color_timer, red, green, blue);
    COLOR_CONTROLER.init(color_controler);

    // setup the pot A2D
    let mut pot: PotType = board.edge.e02.into_floating_input();
    let adc_config = SaadcConfig {
        time: saadc::Time::_40US,
        ..Default::default()
    };
    let mut adc = Saadc::new(board.ADC, adc_config);
    let mut adc_accumulator_timer = Timer::new(board.TIMER3);
    adc_accumulator_timer.enable_interrupt();
    adc_accumulator_timer.reset_event();
    ADC_ACC_TIMER.init(adc_accumulator_timer);

    // setup buttons
    let a_btn = board.buttons.button_a.into_floating_input().degrade();
    let b_btn = board.buttons.button_b.into_floating_input().degrade();

    //setup gpiote interupts
    let gpiote = Gpiote::new(board.GPIOTE);
    let channel0 = gpiote.channel0(); //a_btn 
    let channel1 = gpiote.channel1(); //b_btn
    channel0.input_pin(&a_btn).hi_to_lo().enable_interrupt();
    channel0.reset_events();
    channel1.input_pin(&b_btn).hi_to_lo().enable_interrupt();
    channel1.reset_events();

    GPIOTE_PERIPHERAL.init(gpiote);

    // Set up the NVIC to handle interrupts.
    unsafe {
        NVIC::unmask(Interrupt::GPIOTE); // btns
        NVIC::unmask(Interrupt::TIMER0); // non-blockign display timer
        NVIC::unmask(Interrupt::TIMER2); // color change timer
        NVIC::unmask(Interrupt::TIMER3); // adc accumulator
    }; // allow NVIC to handle GPIOTE signals
    //clear any currently pending GPIOTE state
    NVIC::unpend(Interrupt::GPIOTE);
    NVIC::unpend(Interrupt::TIMER0);
    NVIC::unpend(Interrupt::TIMER2);
    NVIC::unpend(Interrupt::TIMER3);

    init();

    let mut adc_counter: u32 = 0; //count co-adds used to accumulate ADC_ACCUMULATOR_VALUE, for averaging
    loop {
        // read raw ADC result, with non-negative bounds
        let mut raw_value = adc.read_channel(&mut pot).unwrap();
        if raw_value < 0 {
            raw_value = 0;
        }

        // add ADC result to the accumulating sum
        ADC_ACCUMULATOR_VALUE.fetch_add(raw_value as u32, SeqCst);
        adc_counter += 1;

        // if ADC_READY_READ atomic is set, then average the ADC accumulator vale and update the ColorControler HSV
        if ADC_READY_READ.load(SeqCst) {
            let total = ADC_ACCUMULATOR_VALUE.load(SeqCst);
            let mut average = total as f32 / adc_counter as f32;
            average = average.clamp(MIN_ADC_THRESHOLD, MAX_ADC_THRESHOLD);

            let percentage =
                (average - MIN_ADC_THRESHOLD) / (MAX_ADC_THRESHOLD - MIN_ADC_THRESHOLD); //scale so [0-1]

            // get which HSV setting we are currently on
            let mut display_page = HSVPage::H;
            DISPLAY.with_lock(|display| {
                display_page = display.get_page();
            });

            // update the H,S, or V value with the new ADC averaged result
            COLOR_CONTROLER.with_lock(|color_controler| match display_page {
                HSVPage::H => color_controler.update_hue(percentage),
                HSVPage::S => color_controler.update_sat(percentage),
                HSVPage::V => color_controler.update_value(percentage),
            });

            // reset things for next iteration
            adc_counter = 0;
            ADC_READY_READ.store(false, SeqCst);
            ADC_ACCUMULATOR_VALUE.store(0, SeqCst);
        }
    }
}
