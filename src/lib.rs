use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

lazy_static! {
    static ref GLOBAL_SENDER: Mutex<Option<Sender<MTTouch>>> = Mutex::new(None);
}

#[link(name = "MultitouchSupport", kind = "framework")]
extern "C" {
    fn MTDeviceCreateDefault() -> MTDeviceRef;
    fn MTRegisterContactFrameCallback(_: MTDeviceRef, _: MTContactCallbackFunction);
    fn MTDeviceStart(_: MTDeviceRef, _: i32);
    fn MTDeviceIsBuiltIn(_: MTDeviceRef) -> bool;
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct MTPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct MTVector {
    pub pos: MTPoint,
    pub vel: MTPoint,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub enum MTTouchState {
    NotTracking = 0,
    StartInRange = 1,
    HoverInRange = 2,
    MakeTouch = 3,
    Touching = 4,
    BreakTouch = 5,
    LingerInRange = 6,
    OutOfRange = 7,
}

/// https://gist.github.com/rmhsilva/61cc45587ed34707da34818a76476e11
/// https://web.archive.org/web/20151012175118/http://steike.com/code/multitouch/
/// https://hci.rwth-aachen.de/guide-trackpad
/// http://www.iphonesmartapps.org/aladino/?a=multitouch
/// https://chuck.cs.princeton.edu/release/files/examples/chuck-embed/core/util_hid.cpp maybe useful
/// https://github.com/JitouchApp/Jitouch/blob/3b5018e4bc839426a6ce0917cea6df753d19da10/Application/Gesture.m#L2930
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct MTTouch {
    pub frame: i32,          // The current frame
    pub timestamp: f64,      // Event timestamp
    pub identifier: i32,     // identifier unique for life of a touch
    pub state: MTTouchState, // the current state
    pub finger_id: i32,
    pub hand_id: i32,
    pub normalized: MTVector, // the normalized position and vector of the touch (0,0 to 1,1)
    pub z_total: f32,         // ZTotal?
    pub unknown3: i32,        // Always 0?
    pub angle: f32,           // angle of the touch in radian
    pub major_axis: f32,      // ellipsoid (you can track the angle of each finger)
    pub minor_axis: f32,
    pub absolute: MTVector, // Absolute position and velocity?
    pub unknown4: i32,      // Always 0?
    pub unknown5: i32,      // Always 0?
    pub z_density: f32,
}

type MTDeviceRef = *mut std::ffi::c_void;
type MTContactCallbackFunction =
    Option<unsafe extern "C" fn(i32, *mut MTTouch, i32, f64, i32) -> i32>;

/// # Safety
/// Unsafe as this is the actual callback function
unsafe extern "C" fn callback(
    mut _device: i32,
    data: *mut MTTouch,
    fingers: i32,
    mut _timestamp: f64,
    mut _frame: i32,
) -> i32 {
    let mut i = 0;
    while i < fingers {
        let f: *mut MTTouch = &mut *data.offset(i as isize) as *mut MTTouch;
        i += 1;
        {
            GLOBAL_SENDER
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .send(*f)
                .unwrap();
        }
    }
    0
}

pub fn init_listener() -> Result<Receiver<MTTouch>> {
    let (sx, rx) = channel();
    GLOBAL_SENDER
        .lock()
        .map_err(|_| anyhow!("Err: Poisoned Mutex"))?
        .replace(sx);

    unsafe {
        let dev: MTDeviceRef = MTDeviceCreateDefault();
        if MTDeviceIsBuiltIn(dev) {
            println!("primary");
        } else {
            println!("secondary");
        }
        MTRegisterContactFrameCallback(dev, Some(callback));
        MTDeviceStart(dev, 0);
    }

    Ok(rx)
}
