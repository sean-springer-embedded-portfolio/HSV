//! hsv_display.rs
//! Copyright © 2026 Sean Springer
//! [This program is licensed under the "MIT License"]
//! Please see the file LICENSE in the source distribution of this software for license terms.
//!
//! The hsv_display module contains the HSVDisplay<T> struct which is a wrapper around the
//! microbit::display::nonblocking::Display module and is used to control the MB2 5x5 LED array
//! display (displays either an H, S, or V depending upon the A/B button presses). HSVDisplay<T>
//! also maintains state of the HSV display option (as an HSVPage enum) and provides getters to
//! retrieve the current page (used by main.rs event loop)

use microbit::{
    display::nonblocking::{Display, GreyscaleImage},
    hal::timer::Instance,
};

/// Constants
pub const LED_SIZE: usize = 5; // MB2 LED is 5x5 grid
pub type LEDState = [[u8; LED_SIZE]; LED_SIZE]; // convenience typedef

/// C-style enum which tracks whether we are currently on the Hue, Saturation, or
/// Value setting.
#[derive(Clone, Copy)]
pub enum HSVPage {
    H = 0,
    S = 1,
    V = 2,
}

/// HSVDisplay<T> struct declaration: Note all fields are private
///
/// <T> template contains the TIMER instance used by the nonblocking Display.
/// Note that the nonblocking display requires a TIMER peripheral which must have interrupts enabled
/// both within the TIMER peripheral and via the NVIC
///
/// 1. page: HSVPage enum representing the state of the current HSV setting
/// 2. display: display::nonblocking::Display struct containing TIMER peripheral <T>
/// 3. image: the current GreyscaleImage being rendered by the nonblocking display
pub struct HSVDisplay<T>
where
    T: Instance,
{
    page: HSVPage,
    display: Display<T>,
    image: GreyscaleImage,
}

/// Impl HSVDisplay<T>
///
/// Contains methods for initializing a new HSVDisplay<T> instance, changing the displayed page,
/// rendering the display to the LEDs, and retrieving the HSVPage enum representing with HSV option
/// is currently selected.
impl<T> HSVDisplay<T>
where
    T: Instance,
{
    /// PUBLIC
    /// Generate a new HSVDisplay<T> instance. the display field should be a display::nonblocking::Display struct
    /// instance where <T> is the TIMER peripheral used to initialize the nonblocking Display. The HSV dispaly is
    /// initialized to the Hue (H) setting.
    pub fn new(display: Display<T>) -> Self {
        HSVDisplay {
            page: HSVPage::H,
            display,
            image: GreyscaleImage::new(HSVDisplay::<T>::render_h()),
        }
    }

    /// PUBLIC
    /// Rotate the displayed HSV page to the left, with wrap-around.
    /// This function is called by GPIOTE interrupt in main.rs - A button click
    pub fn left(&mut self) {
        match &self.page {
            HSVPage::H => self.page = HSVPage::V,
            HSVPage::S => self.page = HSVPage::H,
            HSVPage::V => self.page = HSVPage::S,
        };
    }

    /// PUBLIC
    /// Rotate the displayed HSV page to the right, with wrap-around.
    /// This function is called by GPIOTE interrupt in main.rs - B button click
    pub fn right(&mut self) {
        match &self.page {
            HSVPage::H => self.page = HSVPage::S,
            HSVPage::S => self.page = HSVPage::V,
            HSVPage::V => self.page = HSVPage::H,
        };
    }

    /// PRIVATE
    /// statically allocated 5x5 array letter H
    fn render_h() -> &'static LEDState {
        &[
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
            [9, 9, 9, 9, 9],
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
        ]
    }

    /// PRIVATE
    /// statically allocated 5x5 array letter S
    fn render_s() -> &'static LEDState {
        &[
            [9, 9, 9, 9, 9],
            [9, 0, 0, 0, 0],
            [9, 9, 9, 9, 9],
            [0, 0, 0, 0, 9],
            [9, 9, 9, 9, 9],
        ]
    }

    /// PRIVATE
    /// statically allocated 5x5 array letter V
    fn render_v() -> &'static LEDState {
        &[
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
            [0, 9, 0, 9, 0],
            [0, 0, 9, 0, 0],
        ]
    }

    /// PUBLIC
    /// Updates the self.image (GreyscaleImage) with a new H, S, or V 5x5 array and passes
    /// the new GreyscaleImage to the nonblocking Display.show() method for rendering
    pub fn render(&mut self) {
        let leds = match &self.page {
            HSVPage::H => GreyscaleImage::new(HSVDisplay::<T>::render_h()),
            HSVPage::S => GreyscaleImage::new(HSVDisplay::<T>::render_s()),
            HSVPage::V => GreyscaleImage::new(HSVDisplay::<T>::render_v()),
        };

        self.image = leds;
        self.display.show(&self.image);
    }

    /// PUBLIC
    /// Thin wrapper around the nonblocking Display::handle_display_event() method which must be
    /// called on the nonblocking Display timer interrupt to physically updated the LED pin voltage states
    /// and display the image.
    pub fn handle_display_event(&mut self) {
        self.display.handle_display_event();
    }

    /// PUBLIC
    /// return the HSVPage enum instance (Copy) representing the current HSV setting. This function is called
    /// by main.rs event loop
    pub fn get_page(&self) -> HSVPage {
        self.page
    }
}
