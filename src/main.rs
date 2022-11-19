use trackpad_rs::MTDevice;

fn main() {
    let mut devices = MTDevice::devices();
    devices.iter_mut().for_each(|d| {
        d.listen(|dev, touches, fingers, timestamp, frame| {
            println!("{touches:?}");
        })
        .unwrap();
    });

    loop {}
}
