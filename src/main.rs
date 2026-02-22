#![no_std]
#![no_main]

mod utils;

use panic_rtt_target as _;
use rtt_target::{rprintln, rtt_init_print};

use cortex_m_rt::entry;
use microbit::{
    board::Board,
    display::nonblocking::{Display, GreyscaleImage},
    hal::{
        Timer,
        gpio::{
            Floating, Input, Level, Output, PushPull,
            p0::{P0_04, P0_09, P0_10},
            p1::P1_02,
        },
        gpiote::Gpiote,
    },
    pac::{Interrupt, NVIC, TIMER0, TIMER1, TIMER2, interrupt},
};

use crate::utils::color_control::ColorControler;
use crate::utils::hsv_display::HSVDisplay;
use critical_section_lock_mut::LockMut;

/// types
type RedPinType = P0_10<Output<PushPull>>; //e08
type GreenPinType = P0_09<Output<PushPull>>; //e09
type BluePinType = P1_02<Output<PushPull>>; //e16
type PotType = P0_04<Input<Floating>>; //e02
type COLOR_TIMER = Timer<TIMER2>;

/// globals
const DEBOUNCE_TIME: u32 = 100 * 1_000_000 / 1000; // 100ms at 1MHz count rate.

static GPIOTE_PERIPHERAL: LockMut<Gpiote> = LockMut::new();
static DEBOUNCE_TIMER: LockMut<Timer<TIMER1>> = LockMut::new();
static DISPLAY: LockMut<HSVDisplay<TIMER0>> = LockMut::new();

#[interrupt]
fn TIMER0() {
    DISPLAY.with_lock(|display| {
        display.handle_display_event();
    });
}

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
    let color_timer = Timer::new(board.TIMER2);
    let red: RedPinType = board.edge.e08.into_push_pull_output(Level::High); //High means off for the LED
    let green: GreenPinType = board.edge.e09.into_push_pull_output(Level::High); //High means off for the LED
    let blue: BluePinType = board.edge.e16.into_push_pull_output(Level::High);

    // setup the pot A2D
    let pot: PotType = board.edge.e02.into_floating_input();

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
        NVIC::unmask(Interrupt::GPIOTE);
        NVIC::unmask(Interrupt::TIMER0);
    }; // allow NVIC to handle GPIOTE signals
    NVIC::unpend(Interrupt::GPIOTE); //clear any currently pending GPIOTE state
    NVIC::unpend(Interrupt::TIMER0); //clear any currently pending GPIOTE state

    DISPLAY.with_lock(|display| {
        display.render();
    });
    loop {}
}
