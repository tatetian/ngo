use crate::prelude::*;
#[cfg(feature = "sgx")]
use std::thread::SgxThread as Thread;
#[cfg(not(feature = "sgx"))]
use std::thread::Thread;

pub struct Parks {
    sleep_threads: Vec<Mutex<Option<Thread>>>,
}

impl Parks {
    pub fn new(parallelism: u32) -> Self {
        let sleep_threads: Vec<_> = (0..parallelism).map(|_| Mutex::new(None)).collect();
        Self { sleep_threads }
    }

    pub fn register(&self, thread_id: usize) {
        assert!(thread_id < self.sleep_threads.len());

        let mut sleep_thread = self.sleep_threads[thread_id].lock();
        sleep_thread.replace(std::thread::current());
    }

    pub fn unregister(&self, thread_id: usize) {
        assert!(thread_id < self.sleep_threads.len());

        let mut sleep_thread = self.sleep_threads[thread_id].lock();
        sleep_thread.take();
    }

    pub fn park(&self) {
        std::thread::park();
    }

    pub fn park_timeout(&self, duration: core::time::Duration) {
        std::thread::park_timeout(duration);
    }

    pub fn unpark(&self, thread_id: usize) {
        assert!(thread_id < self.sleep_threads.len());

        let sleep_thread = self.sleep_threads[thread_id].lock();
        let thread = sleep_thread.clone();
        drop(sleep_thread);
        if thread.is_some() {
            thread.unwrap().unpark();
        }
    }

    pub fn unpark_all(&self) {
        for thread_id in 0..self.sleep_threads.len() {
            self.unpark(thread_id);
        }
    }

    pub fn len(&self) -> usize {
        self.sleep_threads.len()
    }
}
