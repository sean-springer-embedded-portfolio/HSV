//! color_control.rs
//! Copyright © 2026 Sean Springer
//! [This program is licensed under the "MIT License"]
//! Please see the file LICENSE in the source distribution of this software for license terms.
//!
//! The color_control module contains the ColorControler struct which is a wrapper around the RGB LED
//! state and pin control, conviently allowing for pulse width modulation (PWM) of the R, G, and B pin states
//! to generate the requested color via the HSV setting. The PWM is updated at a 100usec refresh rate.

use embedded_hal::digital::OutputPin;
//use rtt_target::rprint;

use super::hsv_rgb_convert::{Hsv, Rgb};

use crate::BluePinType;
use crate::ColorTimer;
use crate::GreenPinType;
use crate::RedPinType;

/// Recommended starting HSV state, represnting the color magenta
pub const STARTING_HSV: Hsv = Hsv {
    h: 0.9167,
    s: 0.75,
    v: 0.8,
}; //magenta

/// ColorControler struct declaration. Note, all fields are private - use the impl methods for controlling these parameters.
///
/// 1. base_color: the base Hsv color as determined by the ADC result. Updated from main.rs event loop
/// 2. cur_color: Rgb color first converted from the base_color Hsv and then mutated as the PWM has evolved
/// 3. red_pin: instance to the red RGB pin connection point on the MB2 (see main.rs types)
/// 4. green_pin: instance to the green RGB pin connection point on the MB2 (see main.rs types)
/// 5. blue_pin: instance to the blue RGB pin connection point on the MB2 (see main.rs types)
/// 6. timer: PWM timer used to toggle the states of the RGB pin voltages
/// 7. remaining_frames: record of the frames left to render for the current base_color
pub struct ColorControler {
    base_color: Hsv,
    cur_color: Rgb,

    red_pin: RedPinType,
    green_pin: GreenPinType,
    blue_pin: BluePinType,

    timer: ColorTimer,
    remaining_frames: u32,
}

/// Impl ColorControler
///
/// Provides mutator and helper functions for controlling the ColorControler state. See Doc comments below
/// for more details
impl ColorControler {
    const STEPS_PER_FRAME: u32 = 100; // 100 steps at 100us means takes 10ms to make a color
    const DURATION_PER_STEP_US: u32 = 100; // 100 us PWM update rate
    const TICKS_PER_US: u32 = ColorTimer::TICKS_PER_SECOND / 1000 / 1000; // should be 1
    const BRIGHTNESS_STEPS: f32 = 100.0; // Limit each RGB value to 100 bins

    /// Generate a new ColorControler struct. Requires the following parameters:
    /// 1. color: a starting Hsv color
    /// 2. timer: a TIMER peripheral from the MB2
    /// 3. red_pin: a pin on the MB2 which connects to the red LED
    /// 4. green_pin: a pin on the MB2 which connects to the green LED
    /// 5. blue_pin: a pin on the MB2 which connects to the blue LED
    pub fn new(
        color: Hsv,
        mut timer: ColorTimer,
        red_pin: RedPinType,
        green_pin: GreenPinType,
        blue_pin: BluePinType,
    ) -> Self {
        ColorControler::clamp(&mut color.clone());
        timer.enable_interrupt();
        timer.reset_event();

        ColorControler {
            base_color: color,
            cur_color: color.to_rgb(),

            red_pin,
            green_pin,
            blue_pin,

            timer,

            remaining_frames: ColorControler::STEPS_PER_FRAME,
        }
    }

    /// PRIVATE
    /// Thin wrapper around the f32::clamp method which clamps the value (intende for either an Hsv or Rgb single value)
    /// to the appropriate range of [0,1].
    fn _clamp(value: f32) -> f32 {
        value.clamp(0.0, 1.0)
    }

    /// PRIVATE
    /// Custom round implementation which rounds an f32 to the neareset 1/100th decimal (the 1/100th place rounding is
    /// dictated by the ColorControler::BRIGHTNESS_STEPS parameter)
    fn round(number: f32) -> f32 {
        let scaled_number = number * ColorControler::BRIGHTNESS_STEPS;
        let mut integer = scaled_number as u32;
        let remainder = scaled_number - (integer as f32);
        if remainder > 0.5 {
            integer += 1;
        }

        integer as f32 / ColorControler::BRIGHTNESS_STEPS
    }

    /// PRIVATE
    /// Determines the minimum value in the Rgb struct that is NOT zero. This is value is used to determine the duration of the
    /// current PWM step. Note that this function should only ever return 0 if all three red, green, and blue values are currently 0.
    fn find_min_nonzero(rgb: &Rgb) -> f32 {
        let mut min = 1.1; // a number greater than what any of the r,g,b values can be

        if rgb.r < min && rgb.r > 0.0 {
            min = rgb.r;
        }
        if rgb.g < min && rgb.g > 0.0 {
            min = rgb.g;
        }
        if rgb.b < min && rgb.b > 0.0 {
            min = rgb.b;
        }

        // if min is > 1 then all rgb values are 0
        if min > 1.0 { 0.0 } else { min }
    }

    /// PRIVATE
    /// Subtracts value from all the self.cur_color r,g,b components with clamping and rounding. After each
    /// PWM step, self.cur_color is updated to subtract the percentage of time spent at the current PWM step
    /// from each self.cur_color rgb.
    fn subtract_rgb(&mut self, value: f32) {
        self.cur_color.r = ColorControler::round(ColorControler::_clamp(self.cur_color.r - value));
        self.cur_color.g = ColorControler::round(ColorControler::_clamp(self.cur_color.g - value));
        self.cur_color.b = ColorControler::round(ColorControler::_clamp(self.cur_color.b - value));
    }

    /// PUBLIC
    /// Convience function for clamping all parameters of the Hsv struct to [0,1] range
    pub fn clamp(hsv: &mut Hsv) {
        hsv.h = ColorControler::_clamp(hsv.h);
        hsv.s = ColorControler::_clamp(hsv.s);
        hsv.v = ColorControler::_clamp(hsv.v);
    }

    /// PUBLIC
    /// update self.base_color's hue component. Called by main.rs event loop with the ADC result
    pub fn update_hue(&mut self, hue: f32) {
        self.base_color.h = ColorControler::_clamp(hue);
    }

    /// PUBLIC
    /// update self.base_color's saturation component. Called by main.rs event loop with the ADC result
    pub fn update_sat(&mut self, sat: f32) {
        self.base_color.s = ColorControler::_clamp(sat);
    }

    /// PUBLIC
    /// update self.base_color's value component. Called by main.rs event loop with the ADC result
    pub fn update_value(&mut self, value: f32) {
        self.base_color.v = ColorControler::_clamp(value);
    }

    /// PUBLIC
    /// Render the RGB color by setting each RGB pin state and set up the new PWM interval by starting the self.timer duration.
    /// This function is called by the TIMER2() interrupt handler in main.rs
    pub fn render(&mut self) {
        // if self.remaining_frames == 0, then a total frame has completed so update self.cur_color (the color to be rendered on the
        // RGB LED) during this frame with the value currently stored in self.base_color.
        if self.remaining_frames == 0 {
            self.cur_color = self.base_color.to_rgb();
            self.cur_color.r = ColorControler::round(self.cur_color.r);
            self.cur_color.g = ColorControler::round(self.cur_color.g);
            self.cur_color.b = ColorControler::round(self.cur_color.b);

            // reset the frame duration to 10msec
            self.remaining_frames = ColorControler::STEPS_PER_FRAME;
        }

        let rgb = self.cur_color;
        let min_val = ColorControler::find_min_nonzero(&rgb); //dicates the duration of this PWM step

        if rgb.r > 0.0 {
            self.red_pin.set_low(); //turn on
        } else {
            self.red_pin.set_high(); // turn off
        }

        if rgb.g > 0.0 {
            self.green_pin.set_low(); //turn on
        } else {
            self.green_pin.set_high(); //turn off
        }

        if rgb.b > 0.0 {
            self.blue_pin.set_low(); //turn on
        } else {
            self.blue_pin.set_high(); //turn off
        }

        // number of 100usec steps to wait at these pin states
        let mut steps = (min_val * ColorControler::STEPS_PER_FRAME as f32) as u32; //round down makes sense bc all this takes time

        // if steps == 0 then all RGB pins should be off (set_high) and the duration of the frame will
        // have the LED completely off
        if steps == 0 {
            steps = self.remaining_frames;
        }

        let duration_us = steps * ColorControler::DURATION_PER_STEP_US;
        let clock_cycles = ColorControler::TICKS_PER_US * duration_us; //PWM duration in clock cycles

        self.remaining_frames -= steps;
        self.subtract_rgb(min_val); // indicate the "new color" for the next PWM cycle

        // clock_cycles should never be 0, but this is provided just-in-case: If self.timer is passed 0 then the
        // timer will never interrupt and the LED is essentially stuck
        if clock_cycles == 0 {
            self.timer.start(2);
        } else {
            self.timer.start(clock_cycles); //round down makes sense bc all this takes time    
        }
    }
}
