use anyhow::Result;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use reemote_protocol::{InputEvent, MouseButton as ProtoMouseButton, SpecialKey};

pub struct InputInjector {
    enigo: Enigo,
}

impl InputInjector {
    pub fn new() -> Result<Self> {
        let enigo =
            Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("failed to init input injector: {e}"))?;
        Ok(Self { enigo })
    }

    pub fn apply(&mut self, event: InputEvent) -> Result<()> {
        match event {
            InputEvent::MouseMove { x, y } => {
                self.enigo
                    .move_mouse(x as i32, y as i32, Coordinate::Abs)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            InputEvent::MouseButton { button, down } => {
                let btn = match button {
                    ProtoMouseButton::Left => Button::Left,
                    ProtoMouseButton::Right => Button::Right,
                    ProtoMouseButton::Middle => Button::Middle,
                };
                let dir = if down { Direction::Press } else { Direction::Release };
                self.enigo.button(btn, dir).map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            InputEvent::MouseScroll { dx, dy } => {
                if dy != 0.0 {
                    self.enigo
                        .scroll(dy as i32, Axis::Vertical)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                }
                if dx != 0.0 {
                    self.enigo
                        .scroll(dx as i32, Axis::Horizontal)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                }
            }
            InputEvent::SpecialKey { key, down } => {
                let key = map_special_key(key);
                let dir = if down { Direction::Press } else { Direction::Release };
                self.enigo.key(key, dir).map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            InputEvent::Text { chars } => {
                self.enigo.text(&chars).map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        }
        Ok(())
    }
}

fn map_special_key(key: SpecialKey) -> Key {
    match key {
        SpecialKey::Enter => Key::Return,
        SpecialKey::Escape => Key::Escape,
        SpecialKey::Backspace => Key::Backspace,
        SpecialKey::Tab => Key::Tab,
        SpecialKey::Space => Key::Space,
        SpecialKey::Delete => Key::Delete,
        SpecialKey::ArrowUp => Key::UpArrow,
        SpecialKey::ArrowDown => Key::DownArrow,
        SpecialKey::ArrowLeft => Key::LeftArrow,
        SpecialKey::ArrowRight => Key::RightArrow,
        SpecialKey::Home => Key::Home,
        SpecialKey::End => Key::End,
        SpecialKey::PageUp => Key::PageUp,
        SpecialKey::PageDown => Key::PageDown,
        SpecialKey::Shift => Key::Shift,
        SpecialKey::Control => Key::Control,
        SpecialKey::Alt => Key::Alt,
        SpecialKey::Meta => Key::Meta,
        SpecialKey::F1 => Key::F1,
        SpecialKey::F2 => Key::F2,
        SpecialKey::F3 => Key::F3,
        SpecialKey::F4 => Key::F4,
        SpecialKey::F5 => Key::F5,
        SpecialKey::F6 => Key::F6,
        SpecialKey::F7 => Key::F7,
        SpecialKey::F8 => Key::F8,
        SpecialKey::F9 => Key::F9,
        SpecialKey::F10 => Key::F10,
        SpecialKey::F11 => Key::F11,
        SpecialKey::F12 => Key::F12,
        SpecialKey::A => Key::Unicode('a'),
        SpecialKey::B => Key::Unicode('b'),
        SpecialKey::C => Key::Unicode('c'),
        SpecialKey::D => Key::Unicode('d'),
        SpecialKey::E => Key::Unicode('e'),
        SpecialKey::F => Key::Unicode('f'),
        SpecialKey::G => Key::Unicode('g'),
        SpecialKey::H => Key::Unicode('h'),
        SpecialKey::I => Key::Unicode('i'),
        SpecialKey::J => Key::Unicode('j'),
        SpecialKey::K => Key::Unicode('k'),
        SpecialKey::L => Key::Unicode('l'),
        SpecialKey::M => Key::Unicode('m'),
        SpecialKey::N => Key::Unicode('n'),
        SpecialKey::O => Key::Unicode('o'),
        SpecialKey::P => Key::Unicode('p'),
        SpecialKey::Q => Key::Unicode('q'),
        SpecialKey::R => Key::Unicode('r'),
        SpecialKey::S => Key::Unicode('s'),
        SpecialKey::T => Key::Unicode('t'),
        SpecialKey::U => Key::Unicode('u'),
        SpecialKey::V => Key::Unicode('v'),
        SpecialKey::W => Key::Unicode('w'),
        SpecialKey::X => Key::Unicode('x'),
        SpecialKey::Y => Key::Unicode('y'),
        SpecialKey::Z => Key::Unicode('z'),
        SpecialKey::Num0 => Key::Unicode('0'),
        SpecialKey::Num1 => Key::Unicode('1'),
        SpecialKey::Num2 => Key::Unicode('2'),
        SpecialKey::Num3 => Key::Unicode('3'),
        SpecialKey::Num4 => Key::Unicode('4'),
        SpecialKey::Num5 => Key::Unicode('5'),
        SpecialKey::Num6 => Key::Unicode('6'),
        SpecialKey::Num7 => Key::Unicode('7'),
        SpecialKey::Num8 => Key::Unicode('8'),
        SpecialKey::Num9 => Key::Unicode('9'),
    }
}
