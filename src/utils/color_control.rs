use embedded_hal::digital::OutputPin;
use microbit::hal::{
    Timer,
    gpio::{
        Floating, Input, Level, Output, PushPull,
        p0::{P0_04, P0_09, P0_10},
        p1::P1_02,
    },
    timer::Instance,
};

use super::hsv_rgb_convert::{Hsv, Rgb};

use crate::BluePinType;
use crate::COLOR_TIMER;
use crate::GreenPinType;
use crate::RedPinType;

pub const STARTING_HSV: Hsv = Hsv {
    h: 0.9167,
    s: 0.75,
    v: 0.8,
}; //magenta

pub struct ColorControler {
    base_color: Hsv,
    cur_color: Rgb,

    red_pin: RedPinType,
    green_pin: GreenPinType,
    blue_pin: BluePinType,

    timer: COLOR_TIMER,
    remaining_frames: f32,
}

impl ColorControler {
    const STEPS_PER_FRAME: f32 = 100.0; // 100 steps at 100us means takes 10ms to make a color
    const DURATION_PER_STEP_MS: f32 = 0.1;
    const TICKS_PER_MS: f32 = (COLOR_TIMER::TICKS_PER_SECOND / 1000) as f32;

    pub fn new(
        color: Hsv,
        mut timer: COLOR_TIMER,
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

    fn _clamp(value: f32) -> f32 {
        let mut ret = value;
        if ret > 1.0 {
            ret = 1.0;
        } else if ret < 0.0 {
            ret = 0.0;
        }

        ret
    }

    fn find_min(rgb: &Rgb) -> f32 {
        let mut min = rgb.r;

        if rgb.g < min {
            min = rgb.g;
        }
        if rgb.b < min {
            min = rgb.b;
        }

        min
    }

    fn round(number: f32) -> u32 {
        let mut integer: u32 = number as u32; //round down
        let remainder: f32 = number - (integer as f32); //get decimal remainder
        if remainder >= 0.5 {
            integer += 1;
        }
        integer
    }

    fn subtract_rgb(&mut self, value: f32) {
        self.cur_color.r -= value;
        self.cur_color.g -= value;
        self.cur_color.b -= value;
    }

    pub fn clamp(hsv: &mut Hsv) {
        hsv.h = ColorControler::_clamp(hsv.h);
        hsv.s = ColorControler::_clamp(hsv.s);
        hsv.v = ColorControler::_clamp(hsv.v);
    }

    pub fn update_hue(&mut self, hue: f32) {
        self.base_color.h = ColorControler::_clamp(hue);
    }

    pub fn update_sat(&mut self, sat: f32) {
        self.base_color.s = ColorControler::_clamp(sat);
    }

    pub fn update_value(&mut self, value: f32) {
        self.base_color.v = ColorControler::_clamp(value);
    }

    pub fn render(&mut self) {
        if self.remaining_frames <= 0.0 {
            self.cur_color = self.base_color.to_rgb();
            self.remaining_frames = ColorControler::STEPS_PER_FRAME;
        }

        let rgb = self.cur_color;
        let min_val = ColorControler::find_min(&rgb);

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

        let steps = min_val * ColorControler::STEPS_PER_FRAME;
        let duration_ms = steps * ColorControler::DURATION_PER_STEP_MS;
        let clock_cycles = ColorControler::TICKS_PER_MS * duration_ms;

        self.remaining_frames -= steps;
        self.subtract_rgb(min_val);

        self.timer.start(ColorControler::round(clock_cycles));
    }
}
