//! NOTE: Don't run in --release mode because rust will optimize the loops away
//! And set the counter directly.
use ostep_rs::threads::atomic_exchange::Mutex;
use std::sync::Arc;

fn main() {
    let counter = Arc::new(Mutex::new(0));

    const COUNT: usize = 500;
    let mut handles: [Option<std::thread::JoinHandle<()>>; COUNT] = unsafe { std::mem::zeroed() };

    // spawn `COUNT` threads all incrementing the same counter.
    (0..COUNT).for_each(|i| {
        let counter = Arc::clone(&counter);
        handles[i] = Some(std::thread::spawn(move || {
            let mut counter = counter.lock().unwrap();
            for _ in 0..5000 {
                *counter += 1;
            }
        }));
    });

    // join the threads.
    (0..COUNT).for_each(|i| {
        handles[i].take().unwrap().join().unwrap();
    });
    println!("counter: {}", *counter.lock().unwrap());
}
