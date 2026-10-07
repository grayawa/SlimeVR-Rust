use gpui_kit::*;
use gpui_windows::overlay_output::Output;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slimevr_gpui::log_level::LogLevel;
use slimevr_openvr_overlay::{Button, Geometry, Input, Pointer, QUIT, Runtime};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::core::Interface;

pub struct Host {
    // Drop registration before the session; the callback has only a weak link.
    output: Output,
    session: Rc<RefCell<Option<Runtime>>>,
    dimensions: Rc<Cell<[f32; 2]>>,
    submission_error: Rc<RefCell<Option<String>>>,
    pointer: Pointer,
    was_visible: bool,
    active: bool,
    dll: PathBuf,
    title: String,
    width: f32,
    next_attempt: Instant,
    last_frame: Instant,
    pub error: Option<String>,
}
pub struct Tick {
    pub visible: bool,
    pub quit: bool,
    pub inputs: Vec<PlatformInput>,
    pub refresh: bool,
}
impl Host {
    pub fn new(
        window: &Window,
        dll: PathBuf,
        title: String,
        width: f32,
        mirror: bool,
        mut initial: Option<Runtime>,
    ) -> Result<Self, String> {
        let raw = HasWindowHandle::window_handle(window)
            .map_err(|e| e.to_string())?
            .as_raw();
        let RawWindowHandle::Win32(raw) = raw else {
            return Err("Expected a Windows GPUI window".into());
        };
        if let Some(runtime) = &mut initial {
            set_thumbnail(runtime);
        }
        let session = Rc::new(RefCell::new(initial));
        let dimensions = Rc::new(Cell::new([0., 0.]));
        let submission_error = Rc::new(RefCell::new(None));
        let weak = Rc::downgrade(&session);
        let frame_dimensions = dimensions.clone();
        let error = submission_error.clone();
        // SAFETY: window is live on this thread and owns this guard through its
        // root view. Window closure drops the registration with that view.
        let output = unsafe {
            Output::attach(raw.hwnd.get() as usize, mirror, move |texture, w, h| {
                if let Some(session) = weak.upgrade()
                    && let Some(runtime) = session.borrow_mut().as_mut()
                {
                    // SAFETY: renderer keeps the shared texture alive, on this UI thread.
                    let result = runtime.submit(texture.as_raw(), w, h);
                    frame_dimensions.set([w as f32, h as f32]);
                    let message = result.err();
                    if *error.borrow() != message {
                        if let Some(message) = &message {
                            slimevr_gpui::logging::write(
                                LogLevel::Warn,
                                "overlay-texture",
                                message,
                            );
                        }
                        *error.borrow_mut() = message;
                    }
                }
            })
        }
        .map_err(|e| e.to_string())?;
        Ok(Self {
            output,
            session,
            dimensions,
            submission_error,
            pointer: Pointer::default(),
            was_visible: false,
            active: false,
            dll,
            title,
            width,
            next_attempt: Instant::now(),
            last_frame: Instant::now(),
            error: None,
        })
    }
    pub fn pump(&mut self, window: &Window) -> Tick {
        if self.session.borrow().is_none() && Instant::now() >= self.next_attempt {
            self.next_attempt = Instant::now() + Duration::from_secs(2);
            match Runtime::open(
                &self.dll,
                "dev.grayawa.slimevr-rust.dashboard",
                &self.title,
                self.width,
            ) {
                Ok(mut runtime) => {
                    set_thumbnail(&mut runtime);
                    *self.session.borrow_mut() = Some(runtime);
                    self.error = None;
                    slimevr_gpui::logging::write(
                        LogLevel::Info,
                        "overlay",
                        "SteamVR dashboard created",
                    );
                }
                Err(error) => {
                    if self.error.as_ref() != Some(&error) {
                        slimevr_gpui::logging::write(LogLevel::Warn, "overlay", &error);
                    }
                    self.error = Some(error);
                }
            }
        }
        let mut inputs = Vec::new();
        let mut quit = false;
        let mut visible = false;
        let logical = window.viewport_size();
        let geometry = Geometry {
            texture: self.dimensions.get(),
            logical: [logical.width.into(), logical.height.into()],
        };
        if let Some(runtime) = self.session.borrow_mut().as_mut() {
            for _ in 0..128 {
                let Some(event) = runtime.poll() else {
                    break;
                };
                if event.kind == QUIT {
                    quit = true;
                }
                inputs.extend(
                    self.pointer
                        .event(event, geometry)
                        .into_iter()
                        .map(platform_input),
                );
            }
            visible = !quit && runtime.visible();
        }
        // Seed the main texture before the dashboard is first selected.
        let active = visible || (self.session.borrow().is_some() && self.dimensions.get()[0] == 0.);
        let refresh = active != self.active;
        self.active = active;
        self.output.set_active(active);
        if self.was_visible && !visible {
            inputs.extend(self.pointer.cancel().into_iter().map(platform_input));
        }
        self.was_visible = visible;
        Tick {
            visible,
            quit,
            inputs,
            refresh,
        }
    }
    pub fn request_frame(&mut self, visible: bool) {
        if (visible || self.active) && self.last_frame.elapsed() >= Duration::from_millis(33) {
            self.last_frame = Instant::now();
            if let Err(error) = self.output.request_frame() {
                self.error = Some(error.to_string());
            }
        }
    }
    pub fn texture_error(&self) -> Option<String> {
        self.submission_error.borrow().clone()
    }
}
fn set_thumbnail(runtime: &mut Runtime) {
    if let Ok(thumbnail) = image::load_from_memory(include_bytes!(
        "../../../../gui/src-tauri/icons/128x128.png"
    )) {
        let thumbnail = thumbnail.to_rgba8();
        if let Err(error) =
            runtime.thumbnail(thumbnail.as_raw(), thumbnail.width(), thumbnail.height())
        {
            slimevr_gpui::logging::write(LogLevel::Warn, "overlay-thumbnail", &error);
        }
    }
}
fn button(value: Button) -> MouseButton {
    match value {
        Button::Left => MouseButton::Left,
        Button::Right => MouseButton::Right,
        Button::Middle => MouseButton::Middle,
    }
}
fn position(value: [f32; 2]) -> Point<Pixels> {
    point(px(value[0]), px(value[1]))
}
fn platform_input(value: Input) -> PlatformInput {
    match value {
        Input::Move {
            position: p,
            pressed,
        } => PlatformInput::MouseMove(MouseMoveEvent {
            position: position(p),
            pressed_button: pressed.map(button),
            ..Default::default()
        }),
        Input::Down {
            position: p,
            button: b,
        } => PlatformInput::MouseDown(MouseDownEvent {
            position: position(p),
            button: button(b),
            click_count: 1,
            first_mouse: false,
            ..Default::default()
        }),
        Input::Up {
            position: p,
            button: b,
        } => PlatformInput::MouseUp(MouseUpEvent {
            position: position(p),
            button: button(b),
            click_count: 1,
            ..Default::default()
        }),
        Input::Scroll { position: p, delta } => PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: position(p),
            delta: ScrollDelta::Lines(point(delta[0], delta[1])),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        }),
    }
}
