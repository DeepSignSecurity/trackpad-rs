# trackpad-rs

A Rust library to get information about and from Multitouch devices (Trackpad / Magic Mouse) using the `MultitouchSupport` framework. 
*Note: `MultitouchSupport` is a private framework, and might not be ideal if you want to ship your app to the App Store.*

## Requirements
- MacOS, unsure about the minimum version, but it works with 12, and 13.

## Usage
Example: 
```rust
    let mut devices = MTDevice::devices();
    devices.iter_mut().for_each(|d| {
        d.listen(|dev, touches, fingers, timestamp, frame| {
            println!("{touches:?}");
        })
        .unwrap();
    });
```

## Documentation
`cargo doc --open`
