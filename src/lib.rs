use anyhow::{bail, Result};
use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use std::{ffi::c_void, fmt::Debug, panic::catch_unwind};

#[link(name = "MultitouchSupport", kind = "framework")]
extern "C" {
    fn MTDeviceCreateList() -> CFArrayRef;
    fn MTDeviceCreateDefault() -> MTDeviceRef;
    fn MTRegisterContactFrameCallback(_: MTDeviceRef, _: MTContactCallbackFunction);
    fn MTRegisterContactFrameCallbackWithRefcon(
        _: MTDeviceRef,
        _: MTContactCallbackFunction,
        extra: *mut c_void,
    );
    fn MTDeviceStart(_: MTDeviceRef, _: i32);
    fn MTDeviceStop(_: MTDeviceRef);
    fn MTDeviceRelease(_: MTDeviceRef);
    fn MTDeviceIsBuiltIn(_: MTDeviceRef) -> bool;
    fn MTDeviceGetFamilyID(_: MTDeviceRef, _: *mut i32);
    fn MTDeviceGetDeviceID(_: MTDeviceRef, _: *mut i32);
    fn MTDeviceIsRunning(_: MTDeviceRef) -> bool;
    fn MTDeviceIsOpaqueSurface(_: MTDeviceRef) -> bool;
    /// Divide x and y by 100 to get the value in centimeters
    fn MTDeviceGetSensorSurfaceDimensions(device: MTDeviceRef, x: *mut i32, y: *mut i32);
    fn MTDeviceGetSensorDimensions(device: MTDeviceRef, rows: *mut i32, cols: *mut i32);
    fn MTDeviceGetDriverType(device: MTDeviceRef, _: *mut i32);
}

#[derive(Debug, Clone, Copy)]
pub enum DeviceType {
    /// Builtin Internal Trackpad
    InternalTrackpad,
    /// External Trackpad
    ExternalTrackpad,
    /// External Magic Mouse
    MagicMouse,
    Unknown(i32),
}

pub struct MTDevice {
    pub device_type: DeviceType,
    pub is_running: bool,
    inner: MTDeviceRef,
}

impl Debug for MTDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MTDevice")
            .field("device_type", &self.device_type)
            .field("is_running", &self.is_running)
            .finish()
    }
}

impl Default for MTDevice {
    fn default() -> Self {
        let default_dev = unsafe { MTDeviceCreateDefault() };
        let device_type = Self::inner_device_type(default_dev);

        Self {
            device_type,
            is_running: false,
            inner: default_dev,
        }
    }
}

impl MTDevice {
    /// List all MTDevices
    pub fn devices() -> Vec<MTDevice> {
        let mut d = vec![];
        unsafe {
            let devices = MTDeviceCreateList();
            let count = CFArrayGetCount(devices);

            for idx in 0..count {
                let dev: MTDeviceRef = CFArrayGetValueAtIndex(devices, idx).cast_mut();
                d.push(MTDevice::new(dev));
            }
        };
        d
    }

    fn new(dev: MTDeviceRef) -> Self {
        Self {
            is_running: false,
            device_type: Self::inner_device_type(dev),
            inner: dev,
        }
    }

    fn start(&mut self) -> Result<()> {
        unsafe { MTDeviceStart(self.inner, 0) };
        self.is_running = unsafe { MTDeviceIsRunning(self.inner) };
        Ok(())
    }

    /// Listen to the MTDevice for MTTouch events and execute the passed callback
    pub fn listen<F>(&mut self, inner_callback: F) -> Result<()>
    where
        F: Fn(MTDeviceRef, &[MTTouch], i32, f64, i32) + Send + Sync + 'static,
    {
        if !self.is_running {
            let inner_callback: Box<
                Box<dyn Fn(MTDeviceRef, &[MTTouch], i32, f64, i32) + Send + Sync + 'static>,
            > = Box::new(Box::new(inner_callback));

            unsafe {
                MTRegisterContactFrameCallbackWithRefcon(
                    self.inner,
                    callback,
                    Box::into_raw(inner_callback) as *mut _,
                )
            };
            self.start()?;
            Ok(())
        } else {
            bail!("already listening");
        }
    }

    pub fn device_id(&self) -> i32 {
        let mut dev_id = 0;
        unsafe {
            MTDeviceGetDeviceID(self.inner, &mut dev_id);
        }
        dev_id
    }

    pub fn family_id(&self) -> i32 {
        let mut family_id = 0;
        unsafe {
            MTDeviceGetFamilyID(self.inner, &mut family_id);
        }
        family_id
    }

    pub fn is_builtin(&self) -> bool {
        unsafe { MTDeviceIsBuiltIn(self.inner) }
    }

    fn inner_device_type(dev: MTDeviceRef) -> DeviceType {
        let mut family_id: i32 = 0;
        unsafe { MTDeviceGetFamilyID(dev, &mut family_id) };

        // 110 is an estimate, for M1 Pro it is 108, and M2 just came out
        if unsafe { MTDeviceIsBuiltIn(dev) } {
            DeviceType::InternalTrackpad
        } else if [112, 113].contains(&family_id) {
            DeviceType::MagicMouse
        } else if (128..=130).contains(&family_id) {
            DeviceType::ExternalTrackpad
        } else {
            DeviceType::Unknown(family_id)
        }
    }

    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    pub fn driver_type(&self) -> i32 {
        let mut driver_type = 0;
        unsafe { MTDeviceGetDriverType(self.inner, &mut driver_type) };
        driver_type
    }

    /// Returns the dimensions of the sensor as (rows, columns)
    pub fn sensor_dimensions(&self) -> (i32, i32) {
        let (mut rows, mut columns) = (0, 0);
        unsafe { MTDeviceGetSensorDimensions(self.inner, &mut rows, &mut columns) };
        (rows, columns)
    }

    /// Returns the physical size of the sensor in centimeters as (x, y)
    pub fn sensor_surface_dimensions(&self) -> (f32, f32) {
        let (mut x, mut y) = (0, 0);
        unsafe { MTDeviceGetSensorSurfaceDimensions(self.inner, &mut x, &mut y) };
        (x as f32 / 100.0, y as f32 / 100.0)
    }

    pub fn is_running(&mut self) -> bool {
        self.is_running
    }

    /// Stops the device but doesn't drop it
    pub fn stop(&mut self) {
        unsafe { MTDeviceStop(self.inner) };
        self.is_running = false;
    }

    /// Both stops and drops (releases) the MTDevice
    pub fn stop_and_drop(mut self) {
        self.stop();
        drop(self);
    }
}

impl Drop for MTDevice {
    /// Releases the MTDevice
    fn drop(&mut self) {
        unsafe { MTDeviceRelease(self.inner) }
    }
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

pub type MTDeviceRef = *mut std::ffi::c_void;
type MTContactCallbackFunction =
    extern "C" fn(MTDeviceRef, &MTTouch, i32, f64, i32, *mut c_void) -> i32;

/// # Safety
/// Unsafe as this is the actual callback function
extern "C" fn callback(
    device: MTDeviceRef,
    data: &MTTouch,
    fingers: i32,
    timestamp: f64,
    frame: i32,
    extra: *mut c_void,
) -> i32 {
    match catch_unwind(|| {
        let data = unsafe { std::slice::from_raw_parts(data, fingers as usize) };
        if !data.is_empty() {
            let inner_callback = unsafe {
                &*(extra
                    as *const Box<
                        dyn Fn(MTDeviceRef, &[MTTouch], i32, f64, i32) + Send + Sync + 'static,
                    >)
            };

            inner_callback(device, data, fingers, timestamp, frame);
        }
    }) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}
