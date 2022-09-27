use std::sync::{Mutex, MutexGuard};

pub trait MutexExt<T> {
    fn always_lock(&self) -> MutexGuard<T>;
}

impl <T> MutexExt<T> for Mutex<T> {
    fn always_lock(&self) -> MutexGuard<T> {
        match self.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner()
        }
    }
}
