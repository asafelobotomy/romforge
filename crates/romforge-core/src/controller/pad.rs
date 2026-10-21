//! The pads themselves, through gilrs (evdev on Linux, SDL's controller
//! database for the button layout, hotplug).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use gilrs::{Axis, EventType, Gilrs};

use super::{Directions, PadEvent, PadId, action_of, parse_ignore, usable};
use crate::Error;
use crate::hotkeys::PadButton;

/// Listen to the pads on a thread of its own, calling `on` with what they
/// do. An error when they can't be read at all (no udev, no access to
/// `/dev/input`).
pub fn listen(mut on: impl FnMut(PadEvent) + Send + 'static) -> Result<(), Error> {
    let (ready, started) = mpsc::channel();
    std::thread::Builder::new()
        .name("romforge-pads".into())
        .spawn(move || {
            let mut g = match Gilrs::new() {
                Ok(g) => {
                    let _ = ready.send(Ok(()));
                    g
                }
                Err(e) => {
                    let _ = ready.send(Err(e.to_string()));
                    return;
                }
            };
            // Steam sets these for the games it starts; honour them too.
            let ignore: Vec<(u16, u16)> = ["SDL_GAMECONTROLLER_IGNORE_DEVICES", "SDL_GAMEPAD_IGNORE_DEVICES"]
                .iter()
                .filter_map(|v| std::env::var(v).ok())
                .flat_map(|v| parse_ignore(&v))
                .collect();
            let mut dirs = Directions::default();
            let mut right = (0.0f32, 0.0f32);
            loop {
                if let Some(ev) = g.next_event_blocking(Some(Duration::from_millis(30))) {
                    let pads: Vec<PadId> = g
                        .gamepads()
                        .map(|(id, p)| PadId {
                            id: id.into(),
                            vendor: p.vendor_id(),
                            product: p.product_id(),
                        })
                        .collect();
                    let id: usize = ev.id.into();
                    let name = g.gamepad(ev.id).name().to_owned();
                    let listened = usable(&pads, &ignore).contains(&id);
                    let now = Instant::now();
                    match ev.event {
                        EventType::Connected if listened => on(PadEvent::Connected { name }),
                        EventType::Disconnected => on(PadEvent::Disconnected { name }),
                        _ if !listened => {}
                        EventType::ButtonPressed(b, _) => {
                            if let Some(button) = PadButton::from_gilrs(&format!("{b:?}")) {
                                on(PadEvent::Button { button, down: true });
                            }
                            if let Some(a) = action_of(&format!("{b:?}")) {
                                let a = if Directions::is_direction(a) {
                                    dirs.press(a, now)
                                } else {
                                    a
                                };
                                on(PadEvent::Action { action: a });
                            }
                        }
                        EventType::ButtonReleased(b, _) => {
                            if let Some(button) = PadButton::from_gilrs(&format!("{b:?}")) {
                                on(PadEvent::Button { button, down: false });
                            }
                            if let Some(a) =
                                action_of(&format!("{b:?}")).filter(|a| Directions::is_direction(*a))
                            {
                                dirs.release(a);
                            }
                        }
                        EventType::AxisChanged(axis @ (Axis::LeftStickX | Axis::LeftStickY), v, _) => {
                            if let Some(a) = dirs.stick(axis == Axis::LeftStickX, v, now) {
                                on(PadEvent::Action { action: a });
                            }
                        }
                        EventType::AxisChanged(axis @ (Axis::RightStickX | Axis::RightStickY), v, _) => {
                            if axis == Axis::RightStickX {
                                right.0 = v;
                            } else {
                                right.1 = v;
                            }
                            on(PadEvent::Stick {
                                x: right.0,
                                y: right.1,
                            });
                        }
                        _ => {}
                    }
                }
                if let Some(a) = dirs.tick(Instant::now()) {
                    on(PadEvent::Action { action: a });
                }
                on(PadEvent::Tick);
            }
        })?;
    match started.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(Error::Launch(format!("game pads can't be read: {e}"))),
        Err(_) => Err(Error::Launch("game pads: the listener stopped".into())),
    }
}
