//! A ticket-lock implementation.
#![allow(static_mut_refs)]
use core::sync::atomic::{AtomicUsize, Ordering};

pub fn lock(turn: &usize, ticket: &AtomicUsize) {
    let my_turn = ticket.fetch_add(1, Ordering::Relaxed);

    #[allow(clippy::while_immutable_condition)]
    // the value of `turn` changes when the "lock owner" calls `unlock`
    while *turn != my_turn {
        std::hint::spin_loop();
    }
}

pub fn unlock(turn: &mut usize) {
    *turn += 1;
}

#[cfg(test)]
mod tests {
    use super::{lock, unlock};
    use core::sync::atomic::AtomicUsize;

    #[test]
    fn test_ticket_lock() {
        static mut COUTNER: usize = 0;
        static mut TURN: usize = 0;
        static mut TICKET: AtomicUsize = AtomicUsize::new(0);

        let mut handles = Vec::with_capacity(10);
        for _ in 1..(100 + 1) {
            handles.push(Some(std::thread::spawn(|| {
                unsafe {
                    lock(&TURN, &TICKET);
                }
                for _ in 0..500 {
                    unsafe {
                        COUTNER += 1;
                    }
                }
                unsafe {
                    unlock(&mut TURN);
                }
            })));
        }
        for mut handle in handles {
            handle.take().unwrap().join().unwrap();
        }
        unsafe {
            assert_eq!(50000, COUTNER);
        }
    }
}
