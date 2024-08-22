extern crate libc;
extern crate x11;

use crate::linux::common::{convert, FALSE};
use crate::linux::keyboard::Keyboard;
use crate::rdev::{Event, ListenError};
use std::convert::TryInto;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ulong};
use std::ptr::null;
use x11::xlib;
use x11::xrecord;

struct Recorder {
    keyboard: Option<Keyboard>,
    callback: Box<dyn FnMut(Event)>,
    record_all_clients: c_ulong,
}

impl Recorder {
    fn new<T>(callback: T) -> Result<Self, ListenError>
    where
        T: FnMut(Event) + 'static,
    {
        let keyboard = Keyboard::new().ok_or(ListenError::KeyboardError)?;

        Ok(Self {
            keyboard: Some(keyboard),
            callback: Box::new(callback),
            record_all_clients: xrecord::XRecordAllClients,
        })
    }
}

pub fn listen<T>(callback: T) -> Result<(), ListenError>
where
    T: FnMut(Event) + 'static,
{
    let mut recorder = Recorder::new(callback)?;

    // Open displays
    let dpy_control = unsafe { xlib::XOpenDisplay(null()) };
    if dpy_control.is_null() {
        return Err(ListenError::MissingDisplayError);
    }

    let extension_name =
        CStr::from_bytes_with_nul(b"RECORD\0").map_err(|_| ListenError::XRecordExtensionError)?;
    let extension = unsafe { xlib::XInitExtension(dpy_control, extension_name.as_ptr()) };
    if extension.is_null() {
        return Err(ListenError::XRecordExtensionError);
    }

    // Prepare record range
    let mut record_range: xrecord::XRecordRange = unsafe { *xrecord::XRecordAllocRange() };
    record_range.device_events.first = xlib::KeyPress as c_uchar;
    record_range.device_events.last = xlib::MotionNotify as c_uchar;

    let mut record_range_ptr: *mut xrecord::XRecordRange = &mut record_range;
    let record_range_ptr_ptr: *mut *mut xrecord::XRecordRange = &mut record_range_ptr;

    // Create context
    let context = unsafe {
        xrecord::XRecordCreateContext(
            dpy_control,
            0,
            &mut recorder.record_all_clients as *mut c_ulong,
            1,
            record_range_ptr_ptr,
            1,
        )
    };

    if context == 0 {
        return Err(ListenError::RecordContextError);
    }

    unsafe { xlib::XSync(dpy_control, FALSE) };

    // Run
    let result = unsafe {
        xrecord::XRecordEnableContext(
            dpy_control,
            context,
            Some(record_callback),
            &mut recorder as *mut Recorder as *mut _,
        )
    };

    if result == 0 {
        return Err(ListenError::RecordContextEnablingError);
    }

    Ok(())
}

#[repr(C)]
struct XRecordDatum {
    type_: u8,
    code: u8,
    _rest: u64,
    _1: bool,
    _2: bool,
    _3: bool,
    root_x: i16,
    root_y: i16,
    event_x: i16,
    event_y: i16,
    state: u16,
}

unsafe extern "C" fn record_callback(
    _null: *mut c_char,
    raw_data: *mut xrecord::XRecordInterceptData,
) {
    let data = raw_data.as_ref().unwrap();
    if data.category != xrecord::XRecordFromServer {
        return;
    }

    debug_assert!(data.data_len * 4 >= std::mem::size_of::<XRecordDatum>().try_into().unwrap());
    // Cast binary data
    #[allow(clippy::cast_ptr_alignment)]
    let xdatum = (data.data as *const XRecordDatum).as_ref().unwrap();

    let code: c_uint = xdatum.code.into();
    let type_: c_int = xdatum.type_.into();

    let x = xdatum.root_x as f64;
    let y = xdatum.root_y as f64;

    let recorder = &mut *(raw_data as *mut Recorder);

    if let Some(event) = convert(&mut recorder.keyboard, code, type_, x, y) {
        (recorder.callback)(event);
    }
    xrecord::XRecordFreeData(raw_data);
}
