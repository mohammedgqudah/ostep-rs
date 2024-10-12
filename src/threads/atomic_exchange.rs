//! A mutex implemented using atomic exchange.
//!
//! https://pages.cs.wisc.edu/~remzi/OSTEP/threads-locks.pdf
//!
//! This Wiki is good to understand atomic memory ordering: <https://gcc.gnu.org/wiki/Atomic/GCCMM/AtomicSync>

use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU8, Ordering};

const MUTEX_AVAILABLE: u8 = 0;
const MUTEX_LOCKED: u8 = 1;
const MUTEX_POISONED: u8 = 2;

type LockResult<'a, T> = Result<MutexGuard<'a, T>, &'static str>;

/// A spin-lock Mutex implementation using CAS.
pub struct Mutex<T> {
    inner: UnsafeCell<T>,
    // 0: available
    // 1: locked
    // 2: poisoned
    flag: AtomicU8,
}

/// RAII
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Mutex<T> {
    pub fn new(inner: T) -> Self {
        Mutex {
            inner: inner.into(),
            flag: AtomicU8::new(0),
        }
    }

    /// "Test and set" lock.
    pub fn lock(&self) -> LockResult<T> {
        self._lock(false)
    }

    /// Test And Test And set lock.
    pub fn lock_ttas(&self) -> LockResult<T> {
        self._lock(true)
    }

    /// Acquire a lock using (test & test_and_set) or test_and_set.
    ///
    /// # Benchmarks
    ///
    /// ::test_and_set_performance          ... bench:   7,122,268.55 ns/iter (+/- 2,196,692.45)
    //  ::test_and_test_and_set_performance ... bench:   5,527,019.90 ns/iter (+/- 2,446,606.89)
    //
    // # Errors
    // Will return an error if the lock is poisoned.
    //
    // # Notes
    // When yielding instead of `continue`ing, TAS is faster than TTAS.
    // Check later the performance of looping first for some time then yielding (hopefully avoid a
    // syscall).
    fn _lock(&self, test_and_test: bool) -> LockResult<T> {
        // Perform an additional test step before the atomic operation.
        // <https://en.wikipedia.org/wiki/Test_and_test-and-set>
        if test_and_test {
            unsafe { while *self.flag.as_ptr() == 1 {} };
        }

        loop {
            // Note: On success, use `Acquire` because I want all memory operations from the
            // previous thread to be visible in this thread. (MutexGuard::drop uses `Ordering::Release`).
            // On failure use `Relaxed`, because it doesn't matter since we're not accessing shared memory.
            //
            // Note: compare_exchange_weak is allowed to spuriously fail (results in efficient code
            // on some platforms).
            match self.flag.compare_exchange_weak(
                MUTEX_AVAILABLE,
                MUTEX_LOCKED,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                // Instead of spending the entire time-slice looping, give it up.
                // There are two problems with yielding:
                // 1. A big number of syscalls
                // 2. Does not address starvation
                //MUTEX_LOCKED => std::thread::yield_now(),
                Err(MUTEX_LOCKED) => continue,
                Ok(MUTEX_AVAILABLE) => break,
                Err(MUTEX_POISONED) => {
                    return Err("The lock is poinsoned");
                }
                _ => unreachable!(),
            }
        }
        Ok(MutexGuard { mutex: self })
    }

    /// Attempt to acquire a lock.
    ///
    /// # Errors
    /// Will return an error if the lock is being held by another thread.
    /// Will return an error if the lock is poisoned.
    pub fn try_lock(&self) -> LockResult<T> {
        match self.flag.swap(MUTEX_LOCKED, Ordering::Relaxed) {
            MUTEX_LOCKED => Err("Lock is not available"),
            MUTEX_AVAILABLE => Ok(MutexGuard { mutex: self }),
            MUTEX_POISONED => Err("The lock is poinsoned"),
            _ => unreachable!(),
        }
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.mutex.inner.get().as_ref().expect("Inner is not null") }
    }
}
impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.inner.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        let flag = if std::thread::panicking() {
            MUTEX_POISONED
        } else {
            MUTEX_AVAILABLE
        };
        self.mutex.flag.store(flag, Ordering::Release);
    }
}

unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Sync> Sync for Mutex<T> {}

#[cfg(test)]
mod tests {
    use super::{Mutex, MUTEX_AVAILABLE};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    extern crate test;

    #[test]
    fn it_acquires_a_lock() {
        let mutex = Mutex::new(5);
        assert_eq!(MUTEX_AVAILABLE, mutex.flag.load(Ordering::Relaxed));

        let mut num = mutex.lock().unwrap();

        assert!(mutex.try_lock().is_err(), "The lock is already acquired");

        *num = 15;
        drop(num); // drop the guard (unlock)
        assert_eq!(15, *mutex.lock().unwrap());
        assert!(mutex.try_lock().is_ok());
    }

    #[test]
    fn it_returns_an_error_if_the_lock_is_poinsoned() {
        let mutex = Arc::new(Mutex::new(5));
        {
            let mutex = Arc::clone(&mutex);
            let _ = std::thread::spawn(move || {
                let _num = mutex.lock().unwrap();
                panic!("Intentionally poison the lock");
            })
            .join();
        }
        assert!(mutex.lock().is_err());
    }

    #[bench]
    fn test_and_set_performance(b: &mut test::Bencher) {
        b.iter(|| {
            let counter = Arc::new(Mutex::new(0));

            const COUNT: usize = 10;
            let mut handles: [Option<std::thread::JoinHandle<()>>; COUNT] =
                unsafe { std::mem::zeroed() };

            // spawn `COUNT` threads all incrementing the same counter.
            (0..COUNT).for_each(|i| {
                let counter = Arc::clone(&counter);
                handles[i] = Some(std::thread::spawn(move || {
                    let mut counter = counter.lock().unwrap();
                    for _ in 0..test::black_box(5000) {
                        *counter += test::black_box(1);
                    }
                }));
            });

            // join the threads.
            (0..COUNT).for_each(|i| {
                handles[i].take().unwrap().join().unwrap();
            });

            assert_eq!(50000, *counter.lock().unwrap());
        });
    }

    #[bench]
    fn test_and_test_and_set_performance(b: &mut test::Bencher) {
        b.iter(|| {
            let counter = Arc::new(Mutex::new(0));

            const COUNT: usize = 10;
            let mut handles: [Option<std::thread::JoinHandle<()>>; COUNT] =
                unsafe { std::mem::zeroed() };

            // spawn `COUNT` threads all incrementing the same counter.
            (0..COUNT).for_each(|i| {
                let counter = Arc::clone(&counter);
                handles[i] = Some(std::thread::spawn(move || {
                    let mut counter = counter.lock_ttas().unwrap();
                    for _ in 0..test::black_box(5000) {
                        *counter += test::black_box(1);
                    }
                }));
            });

            // join the threads.
            (0..COUNT).for_each(|i| {
                handles[i].take().unwrap().join().unwrap();
            });

            assert_eq!(50000, *counter.lock().unwrap());
        });
    }
}
