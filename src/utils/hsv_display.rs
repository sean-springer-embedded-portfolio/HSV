use microbit::{
    display::nonblocking::{Display, GreyscaleImage},
    hal::{Timer, timer::Instance},
};

enum HSVPage {
    H = 0,
    S = 1,
    V = 2,
}

pub struct HSVDisplay<T>
where
    T: Instance,
{
    page: HSVPage,
    display: Display<T>,
    image: GreyscaleImage,
}

pub const LED_SIZE: usize = 5;
pub type LEDState = [[u8; LED_SIZE]; LED_SIZE];

impl<T> HSVDisplay<T>
where
    T: Instance,
{
    pub fn new(display: Display<T>) -> Self {
        HSVDisplay {
            page: HSVPage::H,
            display,
            image: GreyscaleImage::new(HSVDisplay::<T>::render_h()),
        }
    }

    pub fn left(&mut self) {
        match &self.page {
            HSVPage::H => self.page = HSVPage::V,
            HSVPage::S => self.page = HSVPage::H,
            HSVPage::V => self.page = HSVPage::S,
        };
    }

    pub fn right(&mut self) {
        match &self.page {
            HSVPage::H => self.page = HSVPage::S,
            HSVPage::S => self.page = HSVPage::V,
            HSVPage::V => self.page = HSVPage::H,
        };
    }

    fn render_h() -> &'static LEDState {
        &[
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
            [9, 9, 9, 9, 9],
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
        ]
    }

    fn render_s() -> &'static LEDState {
        &[
            [9, 9, 9, 9, 9],
            [9, 0, 0, 0, 0],
            [9, 9, 9, 9, 9],
            [0, 0, 0, 0, 9],
            [9, 9, 9, 9, 9],
        ]
    }

    fn render_v() -> &'static LEDState {
        &[
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
            [9, 0, 0, 0, 9],
            [0, 9, 0, 9, 0],
            [0, 0, 9, 0, 0],
        ]
    }

    pub fn render(&mut self) {
        let leds = match &self.page {
            HSVPage::H => GreyscaleImage::new(HSVDisplay::<T>::render_h()),
            HSVPage::S => GreyscaleImage::new(HSVDisplay::<T>::render_s()),
            HSVPage::V => GreyscaleImage::new(HSVDisplay::<T>::render_v()),
        };

        self.image = leds;
        self.display.show(&self.image);
    }

    pub fn handle_display_event(&mut self) {
        self.display.handle_display_event();
    }
}
