//! ENFORCE LOCK ORDERING BY LOCK ADDRESS
//! https://pages.cs.wisc.edu/~remzi/OSTEP/threads-bugs.pdf
use std::sync::{Arc, Mutex};
use std::thread;

fn lock<T>(lock1: &Mutex<T>, lock2: &Mutex<T>) {
    // Uncomment this expression to make a deadlock.
    let (lock1, lock2) = if std::ptr::from_ref(lock2) > std::ptr::from_ref(lock1) {
        (lock2, lock1)
    } else {
        (lock1, lock2)
    };

    let _l1 = lock1.lock();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let _l2 = lock2.lock();
}

fn main() {
    let l1 = Arc::new(Mutex::new(10));
    let l2 = Arc::new(Mutex::new(5));
    let t1 = {
        let l1 = Arc::clone(&l1);
        let l2 = Arc::clone(&l2);
        thread::spawn(move || {
            lock(&l1, &l2);
        })
    };
    let t2 = {
        let l1 = Arc::clone(&l1);
        let l2 = Arc::clone(&l2);
        thread::spawn(move || {
            lock(&l2, &l1);
        })
    };
    t1.join().unwrap();
    t2.join().unwrap();
}

