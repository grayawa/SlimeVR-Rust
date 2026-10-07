//! Coordinates are OpenVR pixels with a bottom-left origin; UI coordinates are
//! logical pixels with a top-left origin. No GPUI or backend ownership here.
#[cfg(windows)]
mod runtime;
#[cfg(windows)]
pub use runtime::Runtime;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Event {
    pub kind: u32,
    pub button: u32,
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
}
pub const MOVE: u32 = 1;
pub const DOWN: u32 = 2;
pub const UP: u32 = 3;
pub const SCROLL: u32 = 4;
pub const SHOWN: u32 = 5;
pub const HIDDEN: u32 = 6;
pub const QUIT: u32 = 7;
pub const LEAVE: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}
impl Button {
    fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            1 => Some(Self::Left),
            2 => Some(Self::Right),
            4 => Some(Self::Middle),
            _ => None,
        }
    }
    fn bits(self) -> u32 {
        match self {
            Self::Left => 1,
            Self::Right => 2,
            Self::Middle => 4,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub texture: [f32; 2],
    pub logical: [f32; 2],
}
impl Geometry {
    pub fn position(self, x: f32, y: f32) -> Option<[f32; 2]> {
        if ![x, y].iter().all(|v| v.is_finite())
            || !self
                .texture
                .iter()
                .chain(self.logical.iter())
                .all(|v| v.is_finite() && *v > 0.)
        {
            return None;
        }
        let p = [
            x / self.texture[0] * self.logical[0],
            (1. - y / self.texture[1]) * self.logical[1],
        ];
        p.iter().all(|v| v.is_finite()).then_some(p)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Move {
        position: [f32; 2],
        pressed: Option<Button>,
    },
    Down {
        position: [f32; 2],
        button: Button,
    },
    Up {
        position: [f32; 2],
        button: Button,
    },
    Scroll {
        position: [f32; 2],
        delta: [f32; 2],
    },
}
#[derive(Default)]
pub struct Pointer {
    position: Option<[f32; 2]>,
    buttons: u32,
}
impl Pointer {
    /// Releases outside the UI to cancel, rather than click, an interrupted press.
    pub fn cancel(&mut self) -> Vec<Input> {
        let position = [-10_000., -10_000.];
        let mut events = vec![Input::Move {
            position,
            pressed: None,
        }];
        for button in [Button::Left, Button::Right, Button::Middle] {
            if self.buttons & button.bits() != 0 {
                events.push(Input::Up { position, button });
            }
        }
        self.buttons = 0;
        self.position = None;
        events
    }
    pub fn event(&mut self, event: Event, geometry: Geometry) -> Vec<Input> {
        if matches!(event.kind, HIDDEN | QUIT | LEAVE) {
            return self.cancel();
        }
        if event.kind == SCROLL {
            return match self.position {
                Some(position) if [event.dx, event.dy].iter().all(|v| v.is_finite()) => {
                    vec![Input::Scroll {
                        position,
                        delta: [event.dx, event.dy],
                    }]
                }
                _ => vec![],
            };
        }
        if !matches!(event.kind, MOVE | DOWN | UP) {
            return vec![];
        }
        let Some(position) = geometry.position(event.x, event.y) else {
            return vec![];
        };
        self.position = Some(position);
        if event.kind == MOVE {
            let pressed = [Button::Left, Button::Right, Button::Middle]
                .into_iter()
                .find(|b| self.buttons & b.bits() != 0);
            return vec![Input::Move { position, pressed }];
        }
        let Some(button) = Button::from_bits(event.button) else {
            return vec![];
        };
        let held = self.buttons & button.bits() != 0;
        if event.kind == DOWN && !held {
            self.buttons |= button.bits();
            vec![
                Input::Move {
                    position,
                    pressed: None,
                },
                Input::Down { position, button },
            ]
        } else if event.kind == UP && held {
            self.buttons &= !button.bits();
            vec![Input::Up { position, button }]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const GEOMETRY: Geometry = Geometry {
        texture: [1536., 960.],
        logical: [1024., 640.],
    };
    #[test]
    fn dpi_scaled_input_flips_y_and_preserves_outside_positions() {
        assert_eq!(GEOMETRY.position(0., 960.), Some([0., 0.]));
        assert_eq!(GEOMETRY.position(768., 480.), Some([512., 320.]));
        assert_eq!(GEOMETRY.position(-15., 975.), Some([-10., -10.]));
        assert_eq!(GEOMETRY.position(f32::NAN, 0.), None);
        assert_eq!(
            Geometry {
                texture: [0., 0.],
                ..GEOMETRY
            }
            .position(0., 0.),
            None
        );
    }
    #[test]
    fn repeated_down_or_up_does_not_duplicate_a_click() {
        let mut pointer = Pointer::default();
        let down = Event {
            kind: DOWN,
            button: 1,
            x: 300.,
            y: 600.,
            ..Default::default()
        };
        assert_eq!(pointer.event(down, GEOMETRY).len(), 2);
        assert!(pointer.event(down, GEOMETRY).is_empty());
        assert_eq!(pointer.event(Event { kind: UP, ..down }, GEOMETRY).len(), 1);
        assert!(
            pointer
                .event(Event { kind: UP, ..down }, GEOMETRY)
                .is_empty()
        );
    }
    #[test]
    fn hiding_during_a_drag_releases_outside_and_resets_scroll_position() {
        let mut pointer = Pointer::default();
        pointer.event(
            Event {
                kind: DOWN,
                button: 1,
                x: 768.,
                y: 480.,
                ..Default::default()
            },
            GEOMETRY,
        );
        let events = pointer.event(
            Event {
                kind: HIDDEN,
                ..Default::default()
            },
            GEOMETRY,
        );
        assert!(matches!(
            events[1],
            Input::Up {
                position: [-10_000., -10_000.],
                ..
            }
        ));
        assert!(
            pointer
                .event(
                    Event {
                        kind: SCROLL,
                        dy: 1.,
                        ..Default::default()
                    },
                    GEOMETRY
                )
                .is_empty()
        );
        assert!(
            pointer
                .event(
                    Event {
                        kind: UP,
                        button: 1,
                        x: 768.,
                        y: 480.,
                        ..Default::default()
                    },
                    GEOMETRY
                )
                .is_empty()
        );
    }
    #[test]
    fn scroll_uses_the_last_pointer_location_and_rejects_invalid_values() {
        let mut pointer = Pointer::default();
        pointer.event(
            Event {
                kind: MOVE,
                x: 768.,
                y: 480.,
                ..Default::default()
            },
            GEOMETRY,
        );
        assert_eq!(
            pointer.event(
                Event {
                    kind: SCROLL,
                    dy: 1.,
                    ..Default::default()
                },
                GEOMETRY
            ),
            vec![Input::Scroll {
                position: [512., 320.],
                delta: [0., 1.]
            }]
        );
        assert!(
            pointer
                .event(
                    Event {
                        kind: SCROLL,
                        dy: f32::INFINITY,
                        ..Default::default()
                    },
                    GEOMETRY
                )
                .is_empty()
        );
    }
}
