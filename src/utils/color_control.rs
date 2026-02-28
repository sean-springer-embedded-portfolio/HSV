use embedded_hal::digital::OutputPin;
use rtt_target::rprint;

use super::hsv_rgb_convert::{Hsv, Rgb};

use crate::BluePinType;
use crate::ColorTimer;
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

    timer: ColorTimer,
    remaining_frames: u32,
}

impl ColorControler {
    const STEPS_PER_FRAME: u32 = 100; // 100 steps at 100us means takes 10ms to make a color
    const DURATION_PER_STEP_US: u32 = 100; //100 us
    const TICKS_PER_US: u32 = ColorTimer::TICKS_PER_SECOND / 1000 / 1000; //should be 1
    const BRIGHTNESS_STEPS: f32 = 100.0;

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

    fn _clamp(value: f32) -> f32 {
        value.clamp(0.0, 1.0)
    }

    fn round(number: f32) -> f32 {
        if number < 0.0 {
            panic!("Round Function cast to u32 so must be positive!");
        }

        let scaled_number = number * ColorControler::BRIGHTNESS_STEPS;
        let mut integer = scaled_number as u32;
        let remainder = scaled_number - (integer as f32);
        if remainder > 0.5 {
            integer += 1;
        }

        integer as f32 / ColorControler::BRIGHTNESS_STEPS
    }

    fn find_min_nonzero(rgb: &Rgb) -> f32 {
        let mut min = 1.1;

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

    fn subtract_rgb(&mut self, value: f32) {
        self.cur_color.r = ColorControler::_clamp(self.cur_color.r - value);
        self.cur_color.g = ColorControler::_clamp(self.cur_color.g - value);
        self.cur_color.b = ColorControler::_clamp(self.cur_color.b - value);
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
        if self.remaining_frames == 0 {
            self.cur_color = self.base_color.to_rgb();
            self.cur_color.r = ColorControler::round(self.cur_color.r);
            self.cur_color.g = ColorControler::round(self.cur_color.g);
            self.cur_color.b = ColorControler::round(self.cur_color.b);

            self.remaining_frames = ColorControler::STEPS_PER_FRAME;
        }

        let rgb = self.cur_color;
        let min_val = ColorControler::find_min_nonzero(&rgb);

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

        let mut steps = (min_val * ColorControler::STEPS_PER_FRAME as f32) as u32;
        if steps == 0 {
            steps = self.remaining_frames;
        }

        let duration_us = steps * ColorControler::DURATION_PER_STEP_US;
        let clock_cycles = ColorControler::TICKS_PER_US * duration_us;

        self.remaining_frames -= steps;
        self.subtract_rgb(min_val);

        self.timer.start(clock_cycles); //round down makes sense bc all this takes time
    }
}
