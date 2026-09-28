use eframe::egui;
use reemote_protocol::{DisplayInfo, FrameChunk, InputEvent, MouseButton as ProtoMouseButton, SpecialKey};
use std::sync::mpsc;

use crate::net::{self, NetToUi, UiToNet};

pub struct App {
    host: String,
    port: String,
    password: String,
    status: String,
    connected: bool,
    fingerprint: Option<String>,
    new_pin_warning: bool,
    displays: Vec<DisplayInfo>,
    texture: Option<egui::TextureHandle>,
    framebuffer: Option<image::RgbaImage>,
    ui_to_net: tokio::sync::mpsc::UnboundedSender<UiToNet>,
    net_to_ui: mpsc::Receiver<NetToUi>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (ui_to_net, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (net_tx, net_to_ui) = mpsc::channel();
        net::spawn_network_thread(cmd_rx, net_tx, cc.egui_ctx.clone());
        Self {
            host: String::new(),
            port: "7723".to_string(),
            password: String::new(),
            status: "Disconnected".to_string(),
            connected: false,
            fingerprint: None,
            new_pin_warning: false,
            displays: Vec::new(),
            texture: None,
            framebuffer: None,
            ui_to_net,
            net_to_ui,
        }
    }

    fn drain_net_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.net_to_ui.try_recv() {
            match event {
                NetToUi::Status(s) => self.status = s,
                NetToUi::Connected {
                    host_name,
                    fingerprint,
                    new_pin,
                } => {
                    self.connected = true;
                    self.status = format!("Connected to {host_name}");
                    self.fingerprint = Some(fingerprint);
                    self.new_pin_warning = new_pin;
                }
                NetToUi::AuthFailed(reason) => {
                    self.status = format!("Authentication failed: {reason}");
                    self.connected = false;
                }
                NetToUi::Displays(displays) => self.displays = displays,
                NetToUi::Frame(chunk) => self.apply_frame(ctx, chunk),
                NetToUi::Disconnected(reason) => {
                    self.connected = false;
                    self.status = format!("Disconnected: {reason}");
                    self.framebuffer = None;
                    self.texture = None;
                }
                NetToUi::Error(e) => {
                    self.status = format!("Error: {e}");
                    self.connected = false;
                }
            }
        }
    }

    fn apply_frame(&mut self, ctx: &egui::Context, chunk: FrameChunk) {
        let needs_alloc = match &self.framebuffer {
            Some(fb) => fb.width() != chunk.full_width || fb.height() != chunk.full_height,
            None => true,
        };
        if needs_alloc {
            self.framebuffer = Some(image::RgbaImage::new(chunk.full_width, chunk.full_height));
        }
        let fb = self.framebuffer.as_mut().unwrap();

        if let Ok(sub) = image::load_from_memory_with_format(&chunk.jpeg, image::ImageFormat::Jpeg)
        {
            let sub_rgba = sub.to_rgba8();
            image::imageops::replace(fb, &sub_rgba, chunk.x as i64, chunk.y as i64);
        }

        let size = [fb.width() as usize, fb.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, fb.as_raw());
        match &mut self.texture {
            Some(tex) => tex.set(color_image, egui::TextureOptions::LINEAR),
            None => {
                self.texture =
                    Some(ctx.load_texture("remote-frame", color_image, egui::TextureOptions::LINEAR));
            }
        }
    }

    fn send_input(&self, event: InputEvent) {
        let _ = self.ui_to_net.send(UiToNet::Input(event));
    }

    fn handle_remote_input(&self, response: &egui::Response, ui: &egui::Ui) {
        if let Some(pos) = response.hover_pos() {
            let local = pos - response.rect.min;
            self.send_input(InputEvent::MouseMove {
                x: local.x,
                y: local.y,
            });
        }

        ui.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::PointerButton {
                        button, pressed, ..
                    } if response.hovered() || !*pressed => {
                        let proto_button = match button {
                            egui::PointerButton::Primary => Some(ProtoMouseButton::Left),
                            egui::PointerButton::Secondary => Some(ProtoMouseButton::Right),
                            egui::PointerButton::Middle => Some(ProtoMouseButton::Middle),
                            _ => None,
                        };
                        if let Some(b) = proto_button {
                            self.send_input(InputEvent::MouseButton {
                                button: b,
                                down: *pressed,
                            });
                        }
                    }
                    egui::Event::MouseWheel { delta, .. } if response.hovered() => {
                        self.send_input(InputEvent::MouseScroll {
                            dx: delta.x,
                            dy: delta.y,
                        });
                    }
                    egui::Event::Text(text) if response.hovered() || response.has_focus() => {
                        self.send_input(InputEvent::Text {
                            chars: text.clone(),
                        });
                    }
                    egui::Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } if response.hovered() || response.has_focus() => {
                        if let Some(special) = map_key(*key) {
                            // Naive modifier bracketing: wrap each keypress with
                            // its held modifiers rather than tracking standalone
                            // modifier press/release (egui doesn't expose those
                            // as key events). Good enough for common shortcuts
                            // like Ctrl+C; doesn't model chords held across
                            // multiple subsequent keys.
                            if *pressed {
                                for m in modifier_keys(modifiers) {
                                    self.send_input(InputEvent::SpecialKey { key: m, down: true });
                                }
                            }
                            self.send_input(InputEvent::SpecialKey {
                                key: special,
                                down: *pressed,
                            });
                            if !*pressed {
                                for m in modifier_keys(modifiers) {
                                    self.send_input(InputEvent::SpecialKey { key: m, down: false });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_net_events(ctx);

        if !self.connected {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Reemote");
                ui.label(&self.status);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Host:");
                    ui.text_edit_singleline(&mut self.host);
                });
                ui.horizontal(|ui| {
                    ui.label("Port:");
                    ui.text_edit_singleline(&mut self.port);
                });
                ui.horizontal(|ui| {
                    ui.label("Password:");
                    ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                });
                ui.add_space(8.0);
                if ui.button("Connect").clicked() {
                    match self.port.trim().parse::<u16>() {
                        Ok(port) => {
                            self.status = "Connecting...".to_string();
                            let _ = self.ui_to_net.send(UiToNet::Connect {
                                host: self.host.trim().to_string(),
                                port,
                                password: self.password.clone(),
                            });
                        }
                        Err(_) => self.status = "Invalid port".to_string(),
                    }
                }
            });
        } else {
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.status);
                    if self.new_pin_warning {
                        if let Some(fp) = &self.fingerprint {
                            ui.label(
                                egui::RichText::new(format!("New host pinned: {fp}"))
                                    .color(egui::Color32::YELLOW),
                            );
                        }
                    }
                    if ui.button("Disconnect").clicked() {
                        let _ = self.ui_to_net.send(UiToNet::Disconnect);
                    }
                });
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(tex) = &self.texture {
                    egui::ScrollArea::both().show(ui, |ui| {
                        let response =
                            ui.add(egui::Image::new(tex).sense(egui::Sense::click_and_drag()));
                        self.handle_remote_input(&response, ui);
                    });
                } else {
                    ui.label("Waiting for first frame...");
                }
            });
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn modifier_keys(modifiers: &egui::Modifiers) -> Vec<SpecialKey> {
    let mut keys = Vec::new();
    if modifiers.shift {
        keys.push(SpecialKey::Shift);
    }
    if modifiers.ctrl {
        keys.push(SpecialKey::Control);
    }
    if modifiers.alt {
        keys.push(SpecialKey::Alt);
    }
    if modifiers.mac_cmd || modifiers.command {
        keys.push(SpecialKey::Meta);
    }
    keys
}

fn map_key(key: egui::Key) -> Option<SpecialKey> {
    use egui::Key as K;
    Some(match key {
        K::Enter => SpecialKey::Enter,
        K::Escape => SpecialKey::Escape,
        K::Backspace => SpecialKey::Backspace,
        K::Tab => SpecialKey::Tab,
        K::Space => SpecialKey::Space,
        K::Delete => SpecialKey::Delete,
        K::ArrowUp => SpecialKey::ArrowUp,
        K::ArrowDown => SpecialKey::ArrowDown,
        K::ArrowLeft => SpecialKey::ArrowLeft,
        K::ArrowRight => SpecialKey::ArrowRight,
        K::Home => SpecialKey::Home,
        K::End => SpecialKey::End,
        K::PageUp => SpecialKey::PageUp,
        K::PageDown => SpecialKey::PageDown,
        K::F1 => SpecialKey::F1,
        K::F2 => SpecialKey::F2,
        K::F3 => SpecialKey::F3,
        K::F4 => SpecialKey::F4,
        K::F5 => SpecialKey::F5,
        K::F6 => SpecialKey::F6,
        K::F7 => SpecialKey::F7,
        K::F8 => SpecialKey::F8,
        K::F9 => SpecialKey::F9,
        K::F10 => SpecialKey::F10,
        K::F11 => SpecialKey::F11,
        K::F12 => SpecialKey::F12,
        K::A => SpecialKey::A,
        K::B => SpecialKey::B,
        K::C => SpecialKey::C,
        K::D => SpecialKey::D,
        K::E => SpecialKey::E,
        K::F => SpecialKey::F,
        K::G => SpecialKey::G,
        K::H => SpecialKey::H,
        K::I => SpecialKey::I,
        K::J => SpecialKey::J,
        K::K => SpecialKey::K,
        K::L => SpecialKey::L,
        K::M => SpecialKey::M,
        K::N => SpecialKey::N,
        K::O => SpecialKey::O,
        K::P => SpecialKey::P,
        K::Q => SpecialKey::Q,
        K::R => SpecialKey::R,
        K::S => SpecialKey::S,
        K::T => SpecialKey::T,
        K::U => SpecialKey::U,
        K::V => SpecialKey::V,
        K::W => SpecialKey::W,
        K::X => SpecialKey::X,
        K::Y => SpecialKey::Y,
        K::Z => SpecialKey::Z,
        K::Num0 => SpecialKey::Num0,
        K::Num1 => SpecialKey::Num1,
        K::Num2 => SpecialKey::Num2,
        K::Num3 => SpecialKey::Num3,
        K::Num4 => SpecialKey::Num4,
        K::Num5 => SpecialKey::Num5,
        K::Num6 => SpecialKey::Num6,
        K::Num7 => SpecialKey::Num7,
        K::Num8 => SpecialKey::Num8,
        K::Num9 => SpecialKey::Num9,
        _ => return None,
    })
}
