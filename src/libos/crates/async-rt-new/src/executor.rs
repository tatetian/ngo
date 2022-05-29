
use crate::scheduler::{Scheduler};

pub struct Executor {
    num_vcpus: u32,
    scheduler: Scheduler,
    running_vcpus: AtomicU32,
}

impl Executor {
    pub fn shutdown(&self) {
        self.scheduler.shutdown();
    }

    pub fn run_tasks(&self) {
        let this_vcpu = self.running_vcpus.fetch_add(1);
        debug_assert!(this_vcpu < self.num_vcpus);

        vcpu::set_current(this_vcpu);

        loop {
            let task = match self.scheduler.dequeue() {
                Ok(task) => task,
                Err(_) => {
                    // Must have been shut down
                    break;
                }
            };

            let future_slot = match task.future().try_lock() {
                Some(future) => future,
                None => {
                    // The task happens to be executed by other vCPUs at the moment.
                    // Try to execute it later.
                    self.scheduler.enqueue(task);
                    continue;
                }
            };

            let future = match future_slot.as_mut() {
                Some(future) => future,
                None => {
                    // The task happens to be completed
                    continue;
                }
            };

            let waker = waker_ref(&task);
            let context = &mut Context::from_waker(&*waker);
            let ret = future.as_mut().poll(context);
            if let Poll::Poll(()) = ret {
                // As the task is completed, we can destory the future
                drop(future_slot.take());
            }
        }

        vcpu::clear_current();
    }

    pub fn schedule_task(&self, task: &Arc<Task>) {
       self.scheduler.enqueue(task);
    }
}