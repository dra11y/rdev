# better-rdev Changelog

Listen and send keyboard and mouse events on Windows, Linux and MacOS. Forked from rdev (abandoned?) to add cancellation and improve coding practices to use with screen reader.

## [1.0.0] - 2024-08-22

- Forked from `rdev`, updated almost a year ago.
- People have been asking for a way to cancel. `cancel_flag: Arc<AtomicBool>` added on Linux `grab`.
