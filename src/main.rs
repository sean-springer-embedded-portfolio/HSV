#![no_std]
#![no_main]

mod utils;

use panic_rtt_target as _;
use rtt_target::{rtt_init_print};
//use rtt_target::rprintln;
use cortex_m_rt::entry;
use microbit::{
    board::Board,
    display::nonblocking::{Display},
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

use core::{
    sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst},
};

use crate::utils::color_control::{ColorControler, STARTING_HSV};
use crate::utils::hsv_display::{HSVDisplay, HSVPage};
use critical_section_lock_mut::LockMut;

/// types
type RedPinType = P0_10<Output<PushPull>>; //e08
type GreenPinType = P0_09<Output<PushPull>>; //e09
type BluePinType = P1_02<Output<PushPull>>; //e16
type PotType = P0_04<Input<Floating>>; //e02
type ColorTimer = Timer<TIMER2>;

/// globals
const DEBOUNCE_TIME: u32 = 100 * 1_000_000 / 1000; // 100ms at 1MHz count rate.
const MAX_ADC_VALUE: i16 = (1_i16 << 14) - 1_i16;
const MAX_ADC_THRESHOLD: f32 = MAX_ADC_VALUE as f32 * 0.98; //16,053
const MIN_ADC_THRESHOLD: f32 = 10f32; //for clamping
const REFRESH_RATE_MS: u32 = 100;
const TIMER_TICKS_PER_MS: u32 = 1_000_000u32 / 1000;
const REFRESH_RATE_TICKS: u32 = TIMER_TICKS_PER_MS * REFRESH_RATE_MS;

static GPIOTE_PERIPHERAL: LockMut<Gpiote> = LockMut::new();
static DEBOUNCE_TIMER: LockMut<Timer<TIMER1>> = LockMut::new();
static ADC_ACC_TIMER: LockMut<Timer<TIMER3>> = LockMut::new();
static DISPLAY: LockMut<HSVDisplay<TIMER0>> = LockMut::new();
static COLOR_CONTROLER: LockMut<ColorControler> = LockMut::new();
static ADC_ACCUMULATOR_VALUE: AtomicU32 = AtomicU32::new(0); //can accumulate max adc value for more than 5 seconds at 20us sample rate before overflow
static ADC_READY_READ: AtomicBool = AtomicBool::new(false);

/// Non-Blocking Display Timer event handler
#[interrupt]
fn TIMER0() {
    DISPLAY.with_lock(|display| {
        display.handle_display_event();
    });
}

/// RGB LED color change event handler
#[interrupt]
fn TIMER2() {
    COLOR_CONTROLER.with_lock(|color_controler| {
        color_controler.render();
    });
}

/// ADC Accumulator Timer event handler
#[interrupt]
fn TIMER3() {
    ADC_ACC_TIMER.with_lock(|adc_acc_timer| {
        ADC_READY_READ.store(true, SeqCst);
        adc_acc_timer.start(REFRESH_RATE_TICKS);
    });
}

/// Buttons event handler
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
    // reset the interrupt, and update the RESOULTION atomic if debounced timer as timed out
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

#[entry]
fn main() -> ! {
    rtt_init_print!();

    let board = Board::take().unwrap();

    // setup display
    let display = Display::new(board.TIMER0, board.display_pins);
    let mut debounce_timer = Timer::new(board.TIMER1);
    let display = HSVDisplay::new(display);
    DISPLAY.init(display);

    // setup buttons
    let a_btn = board.buttons.button_a.into_floating_input().degrade();
    let b_btn = board.buttons.button_b.into_floating_input().degrade();

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

    //setup gpiote interupts
    let gpiote = Gpiote::new(board.GPIOTE);
    let channel0 = gpiote.channel0(); //a_btn 
    let channel1 = gpiote.channel1(); //b_btn
    channel0.input_pin(&a_btn).hi_to_lo().enable_interrupt();
    channel0.reset_events();
    channel1.input_pin(&b_btn).hi_to_lo().enable_interrupt();
    channel1.reset_events();

    GPIOTE_PERIPHERAL.init(gpiote);

    //setup timer interupts
    debounce_timer.enable_interrupt();
    debounce_timer.reset_event();
    DEBOUNCE_TIMER.init(debounce_timer);

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

    let mut adc_counter: u32 = 0;
    loop {
        let mut raw_value = adc.read_channel(&mut pot).unwrap();
        if raw_value < 0 {
            raw_value = 0;
        }

        ADC_ACCUMULATOR_VALUE.fetch_add(raw_value as u32, SeqCst);
        adc_counter += 1;

        if ADC_READY_READ.load(SeqCst) {
            let total = ADC_ACCUMULATOR_VALUE.load(SeqCst);
            let mut average = total as f32 / adc_counter as f32;
            average = average.clamp(MIN_ADC_THRESHOLD, MAX_ADC_THRESHOLD);
            
            let percentage =
                (average - MIN_ADC_THRESHOLD) / (MAX_ADC_THRESHOLD - MIN_ADC_THRESHOLD); //scale so [0-1]
            
            let mut display_page = HSVPage::H;
            DISPLAY.with_lock(|display| {
                display_page = display.get_page();
            });

            COLOR_CONTROLER.with_lock(|color_controler| match display_page {
                HSVPage::H => color_controler.update_hue(percentage),
                HSVPage::S => color_controler.update_sat(percentage),
                HSVPage::V => color_controler.update_value(percentage),
            });

            // reset things
            adc_counter = 0;
            ADC_READY_READ.store(false, SeqCst);
            ADC_ACCUMULATOR_VALUE.store(0, SeqCst);
        }
    }
}
