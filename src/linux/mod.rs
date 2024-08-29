extern crate libc;
extern crate x11;

mod common;
mod display;
mod gestures;
#[cfg(feature = "unstable_grab")]
pub(crate) mod grab;
mod keyboard;
mod keycodes;
mod listen;
mod simulate;

pub use crate::linux::display::display_size;
pub use crate::linux::keyboard::Keyboard;
pub use crate::linux::listen::listen;
pub use crate::linux::simulate::simulate;
