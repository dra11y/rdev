use crate::linux::common::Display;
use crate::linux::gestures::TouchEventLoop;
use crate::linux::keyboard::Keyboard;
use crate::rdev::{Button, Event, EventType, GrabError, Key, KeyboardState};
use epoll::ControlOptions::{EPOLL_CTL_ADD, EPOLL_CTL_DEL};
use evdev_rs::enums::{EV_ABS, EV_MSC, EV_SW, EV_SYN};
use evdev_rs::{
    enums::{EventCode, EV_KEY, EV_REL},
    Device as EVDevice, InputEvent, UInputDevice,
};
use evdev_rs::{DeviceWrapper, GrabMode};
use inotify::{Inotify, WatchMask};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{read_dir, File};
use std::os::unix::{
    ffi::OsStrExt,
    fs::FileTypeExt,
    io::{AsRawFd, RawFd},
};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use std::{io, thread};

/// TODO The x, y coordinates are currently wrong !! Is there mouse acceleration
/// to take into account ??

macro_rules! convert_keys {
    ($($ev_key:ident, $rdev_key:ident),*) => {
        //TODO: make const when rust lang issue #49146 is fixed
        #[allow(unreachable_patterns)]
        fn evdev_key_to_rdev_key(key: &EV_KEY) -> Option<Key> {
            match key {
                $(
                    EV_KEY::$ev_key => Some(Key::$rdev_key),
                )*
                _ => None,
            }
        }

        // //TODO: make const when rust lang issue #49146 is fixed
        // fn rdev_key_to_evdev_key(key: &Key) -> Option<EV_KEY> {
        //     match key {
        //         $(
        //             Key::$rdev_key => Some(EV_KEY::$ev_key),
        //         )*
        //         _ => None
        //     }
        // }
    };
}

macro_rules! convert_buttons {
    ($($ev_key:ident, $rdev_key:ident),*) => {
        //TODO: make const when rust lang issue #49146 is fixed
        fn evdev_key_to_rdev_button(key: &EV_KEY) -> Option<Button> {
            match key {
                $(
                    EV_KEY::$ev_key => Some(Button::$rdev_key),
                )*
                _ => None,
            }
        }

        // //TODO: make const when rust lang issue #49146 is fixed
        // fn rdev_button_to_evdev_key(event: &Button) -> Option<EV_KEY> {
        //     match event {
        //         $(
        //             Button::$rdev_key => Some(EV_KEY::$ev_key),
        //         )*
        //         _ => None
        //     }
        // }
    };
}

#[rustfmt::skip]
convert_buttons!(
    BTN_LEFT, Left,
    BTN_RIGHT, Right,
    BTN_MIDDLE, Middle
);

//TODO: IntlBackslash, kpDelete
#[rustfmt::skip]
convert_keys!(
    KEY_ESC, Escape,
    KEY_1, Num1,
    KEY_2, Num2,
    KEY_3, Num3,
    KEY_4, Num4,
    KEY_5, Num5,
    KEY_6, Num6,
    KEY_7, Num7,
    KEY_8, Num8,
    KEY_9, Num9,
    KEY_0, Num0,
    KEY_MINUS, Minus,
    KEY_EQUAL, Equal,
    KEY_BACKSPACE, Backspace,
    KEY_TAB, Tab,
    KEY_Q, KeyQ,
    KEY_W, KeyW,
    KEY_E, KeyE,
    KEY_R, KeyR,
    KEY_T, KeyT,
    KEY_Y, KeyY,
    KEY_U, KeyU,
    KEY_I, KeyI,
    KEY_O, KeyO,
    KEY_P, KeyP,
    KEY_LEFTBRACE, LeftBracket,
    KEY_RIGHTBRACE, RightBracket,
    KEY_ENTER, Return,
    KEY_LEFTCTRL, ControlLeft,
    KEY_A, KeyA,
    KEY_S, KeyS,
    KEY_D, KeyD,
    KEY_F, KeyF,
    KEY_G, KeyG,
    KEY_H, KeyH,
    KEY_J, KeyJ,
    KEY_K, KeyK,
    KEY_L, KeyL,
    KEY_SEMICOLON, SemiColon,
    KEY_APOSTROPHE, Quote,
    KEY_GRAVE, BackQuote,
    KEY_LEFTSHIFT, ShiftLeft,
    KEY_BACKSLASH, BackSlash,
    KEY_Z, KeyZ,
    KEY_X, KeyX,
    KEY_C, KeyC,
    KEY_V, KeyV,
    KEY_B, KeyB,
    KEY_N, KeyN,
    KEY_M, KeyM,
    KEY_COMMA, Comma,
    KEY_DOT, Dot,
    KEY_SLASH, Slash,
    KEY_RIGHTSHIFT, ShiftRight,
    KEY_KPASTERISK, KpMultiply,
    KEY_LEFTALT, AltLeft,
    KEY_RIGHTALT, AltRight,
    KEY_SPACE, Space,
    KEY_CAPSLOCK, CapsLock,
    KEY_F1, F1,
    KEY_F2, F2,
    KEY_F3, F3,
    KEY_F4, F4,
    KEY_F5, F5,
    KEY_F6, F6,
    KEY_F7, F7,
    KEY_F8, F8,
    KEY_F9, F9,
    KEY_F10, F10,
    KEY_NUMLOCK, NumLock,
    KEY_SCROLLLOCK, ScrollLock,
    KEY_KP7, Kp7,
    KEY_KP8, Kp8,
    KEY_KP9, Kp9,
    KEY_KPMINUS, KpMinus,
    KEY_KP4, Kp4,
    KEY_KP5, Kp5,
    KEY_KP6, Kp6,
    KEY_KPPLUS, KpPlus,
    KEY_KP1, Kp1,
    KEY_KP2, Kp2,
    KEY_KP3, Kp3,
    KEY_KP0, Kp0,
    KEY_F11, F11,
    KEY_F12, F12,
    KEY_KPENTER, KpReturn,
    KEY_RIGHTCTRL, ControlRight,
    KEY_KPSLASH, KpDivide,
    KEY_RIGHTALT, AltGr,
    KEY_HOME, Home,
    KEY_UP, UpArrow,
    KEY_PAGEUP, PageUp,
    KEY_LEFT, LeftArrow,
    KEY_RIGHT, RightArrow,
    KEY_END, End,
    KEY_DOWN, DownArrow,
    KEY_PAGEDOWN, PageDown,
    KEY_INSERT, Insert,
    KEY_DELETE, Delete,
    KEY_PAUSE, Pause,
    KEY_LEFTMETA, MetaLeft,
    KEY_RIGHTMETA, MetaRight,
    KEY_PRINT, PrintScreen,
    // KpDelete behaves like normal Delete most of the time
    KEY_DELETE, KpDelete,
    // Linux doesn't have an IntlBackslash key
    KEY_BACKSLASH, IntlBackslash
);

fn evdev_event_to_rdev_event(
    event: &InputEvent,
    device: &Device,
    x: &mut i32,
    y: &mut i32,
    w: i32,
    h: i32,
) -> Option<EventType> {
    let event_type = match &device.device_type {
        DeviceType::Keyboard => match &event.event_code {
            EventCode::EV_KEY(key) => match evdev_key_to_rdev_key(key) {
                Some(key) => match event.value {
                    0 => Some(EventType::KeyRelease(key)),
                    _ => Some(EventType::KeyPress(key)),
                },
                None => None,
            },
            _ => None,
        },
        DeviceType::Mouse => match &event.event_code {
            EventCode::EV_KEY(button) => match evdev_key_to_rdev_button(button) {
                Some(button) => match event.value {
                    0 => Some(EventType::ButtonRelease(button)),
                    _ => Some(EventType::ButtonPress(button)),
                },
                None => None,
            },
            EventCode::EV_REL(mouse) => match mouse {
                EV_REL::REL_X => {
                    let dx = event.value;
                    *x += dx;
                    if *x < 0 {
                        *x = 0;
                    }
                    if *x > w {
                        *x = w;
                    }
                    Some(EventType::MouseMove { x: *x, y: *y })
                }
                EV_REL::REL_Y => {
                    let dy = event.value;
                    *y += dy;
                    if *y < 0 {
                        *y = 0;
                    }
                    if *y > h {
                        *y = h;
                    }
                    Some(EventType::MouseMove { x: *x, y: *y })
                }
                EV_REL::REL_HWHEEL => Some(EventType::Wheel {
                    delta_x: event.value,
                    delta_y: 0,
                }),
                EV_REL::REL_HWHEEL_HI_RES => Some(EventType::WheelHires {
                    delta_x: event.value,
                    delta_y: 0,
                }),
                EV_REL::REL_WHEEL => Some(EventType::Wheel {
                    delta_x: 0,
                    delta_y: event.value,
                }),
                EV_REL::REL_WHEEL_HI_RES => Some(EventType::WheelHires {
                    delta_x: 0,
                    delta_y: event.value,
                }),
                _ => None,
            },
            _ => None,
        },
        DeviceType::Touchpad => None,
        // DeviceType::Touchpad => match &event.event_code {
        //     EventCode::EV_KEY(key) => match key {
        //         EV_KEY::BTN_LEFT => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::ButtonUp(Button::Left))),
        //             _ => Some(EventType::Touch(TouchEvent::ButtonDown(Button::Left))),
        //         },
        //         EV_KEY::BTN_RIGHT => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::ButtonUp(Button::Right))),
        //             _ => Some(EventType::Touch(TouchEvent::ButtonDown(Button::Right))),
        //         },
        //         EV_KEY::BTN_TOOL_FINGER => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::FingerCount(0))),
        //             _ => Some(EventType::Touch(TouchEvent::FingerCount(1))),
        //         },
        //         EV_KEY::BTN_TOOL_DOUBLETAP => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::FingerCount(0))),
        //             _ => Some(EventType::Touch(TouchEvent::FingerCount(2))),
        //         },
        //         EV_KEY::BTN_TOOL_TRIPLETAP => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::FingerCount(0))),
        //             _ => Some(EventType::Touch(TouchEvent::FingerCount(3))),
        //         },
        //         EV_KEY::BTN_TOOL_QUADTAP => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::FingerCount(0))),
        //             _ => Some(EventType::Touch(TouchEvent::FingerCount(4))),
        //         },
        //         EV_KEY::BTN_TOUCH => match event.value {
        //             0 => Some(EventType::Touch(TouchEvent::TouchActive(false))),
        //             _ => Some(EventType::Touch(TouchEvent::TouchActive(true))),
        //         },
        //         _ => None,
        //     },
        //     EventCode::EV_SYN(syn) => match syn {
        //         EV_SYN::SYN_REPORT => Some(EventType::Touch(TouchEvent::Sync(false))),
        //         EV_SYN::SYN_MT_REPORT => Some(EventType::Touch(TouchEvent::Sync(true))),
        //         _ => None,
        //     },
        //     EventCode::EV_ABS(touchpad) => match touchpad {
        //         EV_ABS::ABS_X => Some(EventType::Touch(TouchEvent::AbsX(event.value))),
        //         EV_ABS::ABS_Y => Some(EventType::Touch(TouchEvent::AbsY(event.value))),
        //         EV_ABS::ABS_MT_TRACKING_ID => {
        //             Some(EventType::Touch(TouchEvent::TrackingId(event.value)))
        //         }
        //         EV_ABS::ABS_MT_SLOT => Some(EventType::Touch(TouchEvent::Slot(event.value))),
        //         EV_ABS::ABS_MT_POSITION_X => Some(EventType::Touch(TouchEvent::X(event.value))),
        //         EV_ABS::ABS_MT_POSITION_Y => Some(EventType::Touch(TouchEvent::Y(event.value))),
        //         _ => {
        //             println!("TOUCHPAD MISS: {:?}", touchpad);
        //             None
        //         }
        //     },
        //     _ => None,
        // },
        DeviceType::LidSwitch => {
            println!("DeviceType::LidSwitch value: {}", event.value);
            Some(EventType::LidSwitch(event.value))
        }
    };

    if event_type.is_none() {
        let print_event = match &event.event_code {
            EventCode::EV_SYN(_) => false,
            // EventCode::EV_KEY(_) => false,
            EventCode::EV_MSC(msc) => match msc {
                EV_MSC::MSC_SERIAL => false,
                EV_MSC::MSC_PULSELED => false,
                EV_MSC::MSC_GESTURE => true,
                EV_MSC::MSC_RAW => true,
                EV_MSC::MSC_SCAN => false,
                EV_MSC::MSC_TIMESTAMP => false,
                EV_MSC::MSC_MAX => true,
            },
            _ => true,
        };

        // if print_event {
        //     println!(
        //         "UNHANDLED EVENT: device_type: {:?}\nevent: {:?}\ndevice: {:?}",
        //         &device.device_type, &event, &device,
        //     );
        // }
    }

    event_type
}

// fn rdev_event_to_evdev_event(event: &EventType, time: &TimeVal) -> Option<InputEvent> {
//     match event {
//         EventType::KeyPress(key) => {
//             let key = rdev_key_to_evdev_key(&key)?;
//             Some(InputEvent::new(&time, &EventCode::EV_KEY(key), 1))
//         }
//         EventType::KeyRelease(key) => {
//             let key = rdev_key_to_evdev_key(&key)?;
//             Some(InputEvent::new(&time, &EventCode::EV_KEY(key), 0))
//         }
//         EventType::ButtonPress(button) => {
//             let button = rdev_button_to_evdev_key(&button)?;
//             Some(InputEvent::new(&time, &EventCode::EV_KEY(button), 1))
//         }
//         EventType::ButtonRelease(button) => {
//             let button = rdev_button_to_evdev_key(&button)?;
//             Some(InputEvent::new(&time, &EventCode::EV_KEY(button), 0))
//         }
//         EventType::MouseMove { x, y } => {
//             let (x, y) = (*x as i32, *y as i32);
//             //TODO allow both x and y movements simultaneously
//             if x != 0 {
//                 Some(InputEvent::new(&time, &EventCode::EV_REL(EV_REL::REL_X), x))
//             } else {
//                 Some(InputEvent::new(&time, &EventCode::EV_REL(EV_REL::REL_Y), y))
//             }
//         }
//         EventType::Wheel { delta_x, delta_y } => {
//             let (x, y) = (*delta_x as i32, *delta_y as i32);
//             //TODO allow both x and y movements simultaneously
//             if x != 0 {
//                 Some(InputEvent::new(
//                     &time,
//                     &EventCode::EV_REL(EV_REL::REL_HWHEEL),
//                     x,
//                 ))
//             } else {
//                 Some(InputEvent::new(
//                     &time,
//                     &EventCode::EV_REL(EV_REL::REL_WHEEL),
//                     y,
//                 ))
//             }
//         }
//     }
// }

pub type GrabReturn = (Option<Event>, GrabStatus);

pub fn grab<T>(callback: T, cancel_rx: &Receiver<()>) -> Result<(), GrabError>
where
    T: Fn(Event, &EVDevice) -> GrabReturn + 'static,
{
    let mut kb = Keyboard::new().ok_or(GrabError::KeyboardError)?;
    let display = Display::new().ok_or(GrabError::MissingDisplayError)?;
    let (width, height) = display.get_size().ok_or(GrabError::MissingDisplayError)?;
    let (current_x, current_y) = display
        .get_mouse_pos()
        .ok_or(GrabError::MissingDisplayError)?;
    let mut x = current_x as i32;
    let mut y = current_y as i32;
    let w = width as i32;
    let h = height as i32;

    let mut touch_event_loop = Arc::new(Mutex::new(TouchEventLoop::new()));

    filter_map_events(cancel_rx, move |event, device| {
        let mut touch_event_loop = touch_event_loop
            .lock()
            .expect("Failed to lock touch_event_loop Mutex");
        if let Some(gesture) = touch_event_loop.add_event(event.time, event.event_code, event.value)
        {
            // println!("Detected gesture: {:?}, event: {:?}", gesture, event);
            let rdev_event = Event {
                time: SystemTime::now(),
                char: None,
                event_type: EventType::Touch(gesture),
            };
            return match callback(rdev_event, &device.ev_device) {
                (None, grab_status) => (None, grab_status),
                (Some(_), grab_status) => (Some(event), grab_status),
            };
        }
        let event_type = match evdev_event_to_rdev_event(&event, device, &mut x, &mut y, w, h) {
            Some(rdev_event) => rdev_event,
            // If we can't convert event, simulate it
            None => return (Some(event), GrabStatus::Continue),
        };
        let char: Option<String> = kb.add(&event_type);
        let rdev_event = Event {
            time: SystemTime::now(),
            char,
            event_type,
        };
        match callback(rdev_event, &device.ev_device) {
            (None, grab_status) => (None, grab_status),
            (Some(_), grab_status) => (Some(event), grab_status),
        }
    })?;
    Ok(())
}

// #[derive(Clone, Copy, Default)]
// struct Point {
//     x: i32,
//     y: i32,
// }

// #[derive(Clone, Copy, Default)]
// struct TouchState {
//     slot: Option<usize>,
//     down: [bool; 5],
//     point: [Point; 5],
// }

// impl TouchState {
//     fn update(&mut self, slot: Option<usize>, down: Option<bool>, point: Option<Point>) {
//         if let Some(slot) = slot {
//             self.slot = Some(slot);
//         }
//         let Some(slot) = self.slot else {
//             return;
//         };
//         if let Some(down) = down {
//             self.down[slot] = down;
//         }
//         if let Some(point) = point {
//             self.point[slot] = point;
//         }
//     }
// }

fn force_release_keys(output_device: &UInputDevice) {
    println!("force_release_keys output_device: {:?}", output_device);

    sleep(Duration::from_millis(10));

    let keys = [
        EV_KEY::KEY_LEFTMETA,
        EV_KEY::KEY_RIGHTMETA,
        EV_KEY::KEY_LEFTCTRL,
        EV_KEY::KEY_RIGHTCTRL,
        EV_KEY::KEY_LEFTALT,
        EV_KEY::KEY_RIGHTALT,
        EV_KEY::KEY_LEFTSHIFT,
        EV_KEY::KEY_RIGHTSHIFT,
        // Add any other keys that might get stuck
    ];

    for key in keys.iter() {
        let event = InputEvent::new(&evdev_rs::TimeVal::new(0, 0), &EventCode::EV_KEY(*key), 0);
        sleep(Duration::from_millis(10));
        output_device.write_event(&event).ok();
        output_device
            .write_event(&InputEvent::new(
                &evdev_rs::TimeVal::new(0, 0),
                &EventCode::EV_SYN(EV_SYN::SYN_REPORT),
                0,
            ))
            .ok();
    }
}

pub fn filter_map_events<F>(cancel_rx: &Receiver<()>, mut callback: F) -> io::Result<()>
where
    F: FnMut(InputEvent, &Device) -> (Option<InputEvent>, GrabStatus),
{
    let (epoll_fd, mut devices) = setup_devices()?;

    // let ev_devices: Vec<EVDevice> = devices.iter().map(|dev| dev.device).collect();
    // let device_paths: Vec<PathBuf> = devices.iter().map(|dev| dev.path).collect();
    // let output_devices: Vec<UInputDevice> = devices.iter().map(|dev| dev.output_device).collect();

    for device in &mut devices {
        // force_release_keys(&device.output_device);
        device.ev_device.grab(GrabMode::Grab)?;
    }

    let mut inotify = setup_inotify(epoll_fd, &devices)?;

    // create buffer for epoll to fill
    let mut epoll_buffer = [epoll::Event::new(epoll::Events::empty(), 0); 4];
    let mut inotify_buffer = vec![0_u8; 4096];

    'outer_loop: loop {
        match cancel_rx.try_recv() {
            Ok(_) => break 'outer_loop,
            Err(error) => match error {
                std::sync::mpsc::TryRecvError::Empty => (),
                std::sync::mpsc::TryRecvError::Disconnected => break 'outer_loop,
            },
        };

        let num_events = epoll::wait(epoll_fd, -1, &mut epoll_buffer)?;

        // Map and simulate events
        'inner_loop: for event in &epoll_buffer[0..num_events] {
            match cancel_rx.try_recv() {
                Ok(_) => break 'outer_loop,
                Err(error) => match error {
                    std::sync::mpsc::TryRecvError::Empty => (),
                    std::sync::mpsc::TryRecvError::Disconnected => break 'outer_loop,
                },
            };

            // new device file created
            if event.data == INOTIFY_DATA {
                for event in inotify.read_events(&mut inotify_buffer)? {
                    assert!(
                        event.mask.contains(inotify::EventMask::CREATE),
                        "inotify is listening for events other than file creation"
                    );
                    // add_device_to_epoll_from_inotify_event(epoll_fd, event, &mut devices)?;
                }
                // TODO: add_device_to_epoll_from_inotify_event
                panic!("TODO: add_device_to_epoll_from_inotify_event");
            } else {
                // Input device recieved event
                let device_idx = event.data as usize;

                let Some(mut device) = devices.get(device_idx) else {
                    println!("Cannot get device at index: {}", device_idx);
                    thread::sleep(Duration::from_millis(1000));
                    continue;
                };

                let ev_device = &device.ev_device;

                while ev_device.has_event_pending() {
                    //TODO: deal with EV_SYN::SYN_DROPPED
                    let (_, event) = match ev_device.next_event(evdev_rs::ReadFlag::NORMAL) {
                        Ok(event) => event,
                        Err(_) => {
                            let device_fd = ev_device.file().as_raw_fd();
                            let empty_event = epoll::Event::new(epoll::Events::empty(), 0);
                            epoll::ctl(epoll_fd, EPOLL_CTL_DEL, device_fd, empty_event)?;
                            continue 'inner_loop;
                        }
                    };

                    // if let DeviceType::Touchpad(mut current, mut previous) = device.device_type {
                    //     previous = current.clone();
                    //     let mut slot: Option<usize> = None;
                    //     let mut down: Option<bool> = None;
                    //     let mut x: Option<i32> = None;
                    //     let mut y: Option<i32> = None;
                    //     if let Some(current) = current {
                    //         if let Some(slot) = current.slot {
                    //             down = current.down.get(slot).cloned();
                    //         }
                    //         if let Some(point) = current.point.get(slot) {
                    //             (x, y) = (Some(point.x), Some(point.y));
                    //         }
                    //     }
                    //     match event.event_code {
                    //         EventCode::EV_KEY(EV_KEY::BTN_TOUCH) => {
                    //             down = Some(event.value == 1);
                    //         }
                    //         EventCode::EV_ABS(EV_ABS::ABS_MT_SLOT) => {
                    //             slot = Some(event.value.unsigned_abs() as usize);
                    //         }
                    //         EventCode::EV_ABS(EV_ABS::ABS_MT_POSITION_X) => {
                    //             x = Some(event.value);
                    //         }
                    //         EventCode::EV_ABS(EV_ABS::ABS_MT_POSITION_Y) => {
                    //             y = Some(event.value);
                    //         }
                    //         _ => {}
                    //     }

                    //     // device.device_type = DeviceType::Touchpad(current, previous);
                    // }

                    // println!(
                    //     "EVENT: {:?}, {:?}, {:}: {}",
                    //     ev_device,
                    //     event.event_type(),
                    //     event.event_code,
                    //     event.value
                    // );

                    let (event, grab_status) = callback(event, device);

                    if let (Some(event), out_device) = (event, &device.output_device) {
                        out_device.write_event(&event)?;
                    }
                    if grab_status == GrabStatus::Stop {
                        break 'outer_loop;
                    }
                }
            }
        }
    }

    for device in devices.iter_mut() {
        //ungrab devices, ignore errors
        device.ev_device.grab(evdev_rs::GrabMode::Ungrab).ok();
    }

    epoll::close(epoll_fd)?;
    Ok(())
}

static DEV_PATH: &str = "/dev/input";
const INOTIFY_DATA: u64 = u64::MAX;
const EPOLLIN: epoll::Events = epoll::Events::EPOLLIN;

/// Whether to continue grabbing events or to stop
/// Used in `filter_map_events` (and others)
#[derive(Debug, Eq, PartialEq, Hash)]
pub enum GrabStatus {
    /// Stop grabbing
    Continue,
    /// ungrab events
    Stop,
}

fn epoll_watch_all<'a, T>(device_files: T) -> io::Result<RawFd>
where
    T: Iterator<Item = &'a File>,
{
    let epoll_fd = epoll::create(true)?;
    // add file descriptors to epoll
    for (file_idx, file) in device_files.enumerate() {
        let epoll_event = epoll::Event::new(EPOLLIN, file_idx as u64);
        epoll::ctl(epoll_fd, EPOLL_CTL_ADD, file.as_raw_fd(), epoll_event)?;
    }
    Ok(epoll_fd)
}

fn inotify_devices(device_paths: Vec<PathBuf>) -> io::Result<Inotify> {
    let inotify = Inotify::init()?;
    for path in device_paths {
        println!("adding inotify watch for path: {:?}", path);
        inotify.watches().add(path, WatchMask::CREATE)?;
    }
    Ok(inotify)
}

// fn add_device_to_epoll_from_inotify_event(
//     epoll_fd: RawFd,
//     event: inotify::Event<&OsStr>,
//     devices: &mut Vec<Device>,
// ) -> io::Result<()> {
//     let mut device_path = OsString::from(DEV_PATH);
//     device_path.push(OsString::from("/"));
//     device_path.push(event.name.unwrap());
//     // new plug events
//     let file = File::open(device_path)?;
//     let fd = file.as_raw_fd();
//     let device = EVDevice::new_from_file(file)?;
//     let event = epoll::Event::new(EPOLLIN, devices.len() as u64);
//     devices.push(device);
//     epoll::ctl(epoll_fd, EPOLL_CTL_ADD, fd, event)?;
//     Ok(())
// }

#[derive(Debug)]
pub struct Device {
    pub device_type: DeviceType,
    pub ev_device: EVDevice,
    pub file: File,
    pub path: PathBuf,
    pub output_device: UInputDevice,
}

#[derive(Debug, Clone, Copy)]
pub enum DeviceType {
    Keyboard,
    Mouse,
    Touchpad,
    LidSwitch,
}

impl DeviceType {
    fn event_code(&self) -> EventCode {
        match self {
            DeviceType::Keyboard => EventCode::EV_KEY(EV_KEY::KEY_LEFTMETA),
            DeviceType::Mouse => EventCode::EV_REL(EV_REL::REL_WHEEL),
            DeviceType::Touchpad => EventCode::EV_ABS(EV_ABS::ABS_MT_POSITION_Y),
            DeviceType::LidSwitch => EventCode::EV_SW(EV_SW::SW_LID),
        }
    }
}

/// Returns tuple of epoll_fd, all devices, and uinput devices, where
/// uinputdevices is the same length as devices, and each uinput device is
/// a libevdev copy of its corresponding device.The epoll_fd is level-triggered
/// on any available data in the original devices.
fn setup_devices() -> io::Result<(RawFd, Vec<Device>)> {
    // let device_files_and_paths = get_device_files_and_paths(DEV_PATH)?;

    let mut devices = Vec::<Device>::new();

    let mut device_types = HashMap::<&str, DeviceType>::new();
    device_types.insert(
        "Framework Laptop 16 Keyboard Module - ANSI Keyboard",
        DeviceType::Keyboard,
    );
    device_types.insert(
        "Apple Inc. Magic Keyboard with Numeric Keypad",
        DeviceType::Keyboard,
    );
    device_types.insert("Logitech MX Master 3S", DeviceType::Mouse);
    device_types.insert("Lid Switch", DeviceType::LidSwitch);
    device_types.insert("Touchpad", DeviceType::Touchpad);

    for entry in read_dir(DEV_PATH)? {
        let entry = entry?;
        // /dev/input files are character devices
        if !entry.file_type()?.is_char_device() {
            continue;
        }

        let path = entry.path();
        let file_name_bytes = match path.file_name() {
            Some(file_name) => file_name.as_bytes(),
            None => continue, // file_name was "..", should be impossible
        };

        // skip filenames matching "mouse.* or mice".
        // these files don't play nice with libevdev, not sure why
        // see: https://askubuntu.com/questions/1043832/difference-between-dev-input-mouse0-and-dev-input-mice
        if file_name_bytes == OsStr::new("mice").as_bytes()
            || file_name_bytes
                .get(0..=1)
                .map(|s| s == OsStr::new("js").as_bytes())
                .unwrap_or(false)
            || file_name_bytes
                .get(0..=4)
                .map(|s| s == OsStr::new("mouse").as_bytes())
                .unwrap_or(false)
        {
            continue;
        }

        let file = File::open(&path)?;
        let device_file = file.try_clone()?;

        let ev_device = EVDevice::new_from_file(device_file)?;

        // println!("DEVICE: {:?}", ev_device);

        let device_name = ev_device.name().unwrap_or_default();

        let Some(device_type) = device_types
            .iter()
            .find(|(&name, device_type)| {
                device_name.contains(name) && ev_device.has_event_code(&device_type.event_code())
            })
            .map(|(_, &device_type)| device_type)
        else {
            continue;
        };

        // println!("INCLUDE DEVICE: {:?}", ev_device);

        let output_device = UInputDevice::create_from_device(&ev_device)?;

        devices.push(Device {
            device_type,
            ev_device,
            file,
            path,
            output_device,
        });
    }

    let epoll_fd = epoll_watch_all(devices.iter().map(|dev| &dev.file))?;

    Ok((epoll_fd, devices))
}

/// Creates an inotify instance looking at /dev/input and adds it to an epoll instance.
/// Ensures devices isnt too long, which would make the epoll data ambigious.
fn setup_inotify(epoll_fd: RawFd, devices: &Vec<Device>) -> io::Result<Inotify> {
    //Ensure there is space for inotify at last epoll index.
    if devices.len() as u64 >= INOTIFY_DATA {
        eprintln!("number of devices: {}", devices.len());
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "too many device files!",
        ));
    }
    // Set up inotify to listen for new devices being plugged in
    let inotify = inotify_devices(devices.iter().map(|dev| dev.path.clone()).collect())?;
    let epoll_event = epoll::Event::new(EPOLLIN, INOTIFY_DATA);
    epoll::ctl(epoll_fd, EPOLL_CTL_ADD, inotify.as_raw_fd(), epoll_event)?;
    Ok(inotify)
}
