#![allow(dead_code)]

use std::collections::VecDeque;
use std::path::Path;

use time::Duration;
pub use tcod::{self, Color, Console, FontLayout, FontType, RootConsole};
pub use tcod::input::{Key, KeyCode};
// use rustbox::{self, RustBox};

use point::Point;


pub struct Display {
    root: RootConsole,
    // rustbox: RustBox,
    fade: Option<(u8, Color)>,
}

impl Display {
    // fn new(root: RootConsole, rustbox: RustBox) -> Display {
    fn new(root: RootConsole) -> Display {
        Display {
            root: root,
            // rustbox: rustbox,
            fade: None,
        }
    }

    pub fn within_bounds(&self, pos: Point) -> bool {
        pos >= (0, 0) && pos < self.size()
    }

    pub fn draw_char(&mut self, pos: Point, c: char,
                     foreground: Color, background: Color) {
        // self.rustbox.print(x as usize, y as usize,
        //                    rustbox::RB_NORMAL, rustbox::Color::White, rustbox::Color::Default,
        //                    &format!("{}", c));
        if self.within_bounds(pos) {
            self.set_background(pos, background);
            self.root.put_char_ex(pos.x, pos.y, c, foreground, background);
        }
    }

    pub fn write_text<P: Into<Point>>(&mut self, text: &str, pos: P,
                      foreground: Color, background: Color) {
        // self.rustbox.print(x as usize, y as usize,
        //                    rustbox::RB_NORMAL, rustbox::Color::White, rustbox::Color::Default,
        //                    text);
        let pos = pos.into();
        for (i, chr) in text.char_indices() {
            self.draw_char(pos + (i as i32, 0), chr, foreground, background);
        }
    }

    pub fn set_background(&mut self, pos: Point, color: Color) {
        if self.within_bounds(pos) {
            self.root.set_char_background(pos.x, pos.y, color, tcod::BackgroundFlag::Set);
        }
    }

    pub fn get_background(&self, pos: Point) -> Color {
        self.root.get_char_background(pos.x, pos.y)
    }

    pub fn size(&self) -> Point {
        (self.root.width(), self.root.height()).into()
    }

    /// `fade_percentage` is from <0f32 to 100f32>.
    /// 0% means no fade, 100% means screen is completely filled with the `color`
    pub fn fade(&mut self, fade_percentage: f32, color: Color) {
        let fade = (fade_percentage * 255.0) as u8;
        self.fade = Some((fade, color));
    }
}

pub struct Engine {
    pub display: Display,
    pub keys: VecDeque<Key>,
}

impl Engine {
    pub fn new(display_size: Point, default_background: Color,
               window_title: &str, font_path: &Path) -> Engine {
        let mut root = RootConsole::initializer()
            .title(window_title)
            .size(display_size.x, display_size.y)
            .font(font_path, FontLayout::Tcod)
            .font_type(FontType::Greyscale)
            .init();
        root.set_default_background(default_background);

        // let rustbox = RustBox::init(Default::default()).expect(
        //     "Failed to initialise rustbox!");
        // let terminal_size = (rustbox.width() as i32, rustbox.height() as i32);
        // if (terminal_size.0 < width) || (terminal_size.1 < height) {
        //     drop(rustbox);
        //     panic!("The terminal size is too small. Current size: {:?}, required size: {:?}",
        //              terminal_size, (width, height));
        // }

        Engine {
            // display: Display::new(root, rustbox),
            display: Display::new(root),
            keys: VecDeque::new(),
        }
    }

    pub fn main_loop<T>(&mut self, mut state: T, update: fn(T, dt: Duration, &mut Engine) -> Option<T>) {
        let default_fg = Color{r: 255, g: 255, b: 255};
        while !self.display.root.window_closed() {
            // self.display.rustbox.present();
            loop {
                match self.display.root.check_for_keypress(tcod::input::KEY_PRESSED) {
                    None => break,
                    Some(key) => {
                        self.keys.push_back(key);
                    }
                }
            }
            self.display.root.set_default_foreground(default_fg);
            self.display.root.clear();
            self.display.fade = None;

            match update(state,
                         Duration::microseconds((tcod::system::get_last_frame_length() * 1_000_000.0) as i64),
                         self) {
                Some(new_state) => {
                    state = new_state;
                }
                None => break,
            }
            match self.display.fade {
                Some((amount, color)) => {
                    self.display.root.set_fade(amount, color);
                }
                None => {
                    // NOTE: Colour doesn't matter here, value 255 means no fade:
                    self.display.root.set_fade(255, Color{r: 0, g: 0, b: 0});
                }
            }
            self.display.root.flush();
        }
    }


    pub fn fps(&self) -> i32 {
        tcod::system::get_fps()
    }

    /// Return true if the given key is located anywhere in the event buffer.
    pub fn key_pressed(&self, key_code: KeyCode) -> bool {
        for &pressed_key in self.keys.iter() {
            if pressed_key.code == key_code {
                return true;
            }
        }
        false
    }

    /// Consumes the first occurence of the given key in the buffer.
    ///
    /// This is useful when we have a multiple keys in the queue but we want to
    /// check for a presence of a key which should be processed immediately.
    ///
    /// Returns `true` if the key has been in the buffer.
    ///
    /// TODO: investigate using a priority queue instead.
    pub fn read_key(&mut self, key: KeyCode) -> bool {
        let mut len = self.keys.len();
        let mut processed = 0;
        let mut found = false;
        while processed < len {
            match self.keys.pop_front() {
                Some(pressed_key) if !found && pressed_key.code == key => {
                    len -= 1;
                    found = true;
                }
                Some(pressed_key) => {
                    self.keys.push_back(pressed_key);
                }
                None => return false
            }
            processed += 1;
        }
        return found;
    }

    pub fn toggle_fullscreen(&mut self) {
        let current_fullscreen_value = self.display.root.is_fullscreen();
        self.display.root.set_fullscreen(!current_fullscreen_value);
    }
}