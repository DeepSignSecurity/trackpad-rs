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

/// The type of Multitouch device
#[derive(Debug, Clone, Copy)]
pub enum DeviceType {
    /// Builtin Internal Trackpad
    InternalTrackpad,
    /// External Trackpad
    ExternalTrackpad,
    /// External Magic Mouse
    MagicMouse,
    /// Unknown
    Unknown(i32),
}

/// A type that represents the actual Multitouch device
pub struct MTDevice {
    device_type: DeviceType,
    is_running: bool,
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

    /// Get the ID of the current trackpad
    pub fn device_id(&self) -> i32 {
        let mut dev_id = 0;
        unsafe {
            MTDeviceGetDeviceID(self.inner, &mut dev_id);
        }
        dev_id
    }

    /// Get the Family ID of the current trackpad
    pub fn family_id(&self) -> i32 {
        let mut family_id = 0;
        unsafe {
            MTDeviceGetFamilyID(self.inner, &mut family_id);
        }
        family_id
    }

    /// Is this multitouch device (trackpad) built in? 
    /// As opposed to being an external device 
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

    /// Gives information about what type of Device this current Multitouch device is
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    /// Gives information about the type of driver in use. Honestly, no idea what this means for
    /// us.
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
        (x as f32 / 1000.0, y as f32 / 1000.0)
    }

    /// Are we listening to this device?
    pub fn is_running(&mut self) -> bool {
        self.is_running
    }

    /// Get the actual inner [`MTDeviceRef`]
    /// Its a good idea to avoid using it directly if you can
    pub fn inner(&self) -> MTDeviceRef {
        self.inner
    }

    /// Stops the device but doesn't drop it.
    /// Allows you to start it again using [`Self::listen`]
    pub fn stop(&mut self) {
        unsafe { MTDeviceStop(self.inner) };
        self.is_running = false;
    }

    /// Both stops and drops (releases) the MTDevice.
    /// Releases all resources
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

/// Just a point (x, y)
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct MTPoint {
    pub x: f32,
    pub y: f32,
}

/// A struct that contains the current touch position, and velocity
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct MTVector {
    /// current touch position
    pub pos: MTPoint,
    /// velocity
    pub vel: MTPoint,
}

/// The state of an individual touch on a Multitouch device / trackpad.
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

/// The data that the Multitouch Framework gives back, some of the fields are unknown
///
/// ### References:
/// <https://gist.github.com/rmhsilva/61cc45587ed34707da34818a76476e11>
/// <https://web.archive.org/web/20151012175118/http://steike.com/code/multitouch/>
/// <https://hci.rwth-aachen.de/guide-trackpad>
/// <http://www.iphonesmartapps.org/aladino/?a=multitouch>
/// <https://chuck.cs.princeton.edu/release/files/examples/chuck-embed/core/util_hid.cpp> maybe useful
/// <https://github.com/JitouchApp/Jitouch/blob/3b5018e4bc839426a6ce0917cea6df753d19da10/Application/Gesture.m#L2930>
#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct MTTouch {
    /// The current frame number
    pub frame: i32,
    /// Event timestamp
    pub timestamp: f64,
    /// identifier unique for life of a touch
    pub identifier: i32,
    /// the current state
    pub state: MTTouchState,
    pub finger_id: i32,
    pub hand_id: i32,
    /// the normalized position and vector of the touch (0,0 to 1,1)
    pub normalized: MTVector,
    /// ZTotal?
    pub z_total: f32,
    /// Always 0?
    pub unknown3: i32,
    /// angle of the touch in radian
    pub angle: f32,
    /// ellipsoid (you can track the angle of each finger)
    pub major_axis: f32,
    pub minor_axis: f32,
    /// Absolute position and velocity?
    pub absolute: MTVector,
    /// Always 0?
    pub unknown4: i32,
    /// Always 0?
    pub unknown5: i32,
    pub z_density: f32,
}

/// The actual inner pointer to Multitouch Device, best not to use this directly.
/// Only public because of a specific use case for us.
pub type MTDeviceRef = *mut std::ffi::c_void;

type MTContactCallbackFunction =
    extern "C" fn(MTDeviceRef, &MTTouch, i32, f64, i32, *mut c_void) -> i32;

/// # Safety
///
/// This function is the one passed to the Multitouch Framework as the callback to execute.
/// casts [`*mut c_void`] into [`*const Box<dyn Fn()>`] which is the actual callback the user gives
/// to this library and executes it.
/// Probably best not to touch this.
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
