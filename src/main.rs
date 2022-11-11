use trackpad_rs::init_listener;

fn main() {
    let recv = init_listener().unwrap();
    while let Ok(v) = recv.recv() {
        println!("{v:?}");
    }
}
