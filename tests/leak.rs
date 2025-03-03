//! Ideally this would just be a unit test, but it needs to be run on the main thread
//! (specifically on macOS), and the Rust test harness runs tests on worker threads, so it has to
//! be an integration test with `harness = false` instead.

fn main() {
    println!();
    portlight::tests::leak();
}
