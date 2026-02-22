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
    remaining_frames: f32,
}

impl ColorControler {
    const STEPS_PER_FRAME: f32 = 100.0; // 100 steps at 100us means takes 10ms to make a color
    const DURATION_PER_STEP_MS: f32 = 0.1; //100 us
    const TICKS_PER_MS: f32 = (ColorTimer::TICKS_PER_SECOND / 1000) as f32;

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
        let mut ret = value;
        if ret > 1.0 {
            ret = 1.0;
        } else if ret < 0.0 {
            ret = 0.0;
        }

        ret
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

        let mut steps = min_val * ColorControler::STEPS_PER_FRAME;
        if steps <= 0.0 {
            steps = self.remaining_frames;
        }

        let duration_ms = steps * ColorControler::DURATION_PER_STEP_MS;
        let mut clock_cycles = ColorControler::TICKS_PER_MS * duration_ms;

        self.remaining_frames -= steps;
        self.subtract_rgb(min_val);

        if clock_cycles < 1.0 {
            //this means less than 1 us remaining, and can't pass a 0 to timer otherwise it will hang forever
            clock_cycles = 1.0;
            self.remaining_frames = 0.0; //triggers that to rebuild self.cur_color at top of render()
        }

        rprint!("{}", clock_cycles);
        self.timer.start(clock_cycles as u32); //round down makes sense bc all this takes time
    }
}
