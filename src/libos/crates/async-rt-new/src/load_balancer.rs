use crate::sync::Mutex;
use crate::scheduler::{Scheduler};
use crate::util::{SlidingWindowAverager as Averager};
use crate::wait::{WaiterQueue};

/// A load balancer.
///
/// A load balancer spawns per-VCPU tasks that periodically migrate
/// tasks from vCPUs of higher CPU load to those of lower CPU load.
/// The CPU load is defined as the number of runnable tasks on a vCPU.
pub struct LoadBalancer {
    // The state shared between all migration tasks
    shared: Arc<Shared>,
    // The handle of the migration tasks
    join_handles: Mutex<Vec<JoinHandle>>,
}

struct Shared {
    scheduler: Arc<Scheduler>,
    // Whether the migration tasks should stop
    should_stop: AtomicBool,
    // Notify the migration tasks to stop
    stop_wq: WaiterQueue,
}

impl LoadBalancer {
    /// Create a load balancer given the scheduler that is currently in 
    /// charge of scheduling tasks of async_rt.
    pub fn new(scheduler: Arc<Scheduler>) -> Self {
        let shared = {
            let num_vcpus = scheduler.num_vcpus();
            Shared {
                scheduler,
                should_stop: AtomicBool::new(false),
                stop_wq: WaiterQueue::new(),
            }
        };
        Self {
            shared,
            join_handles: Mutex::new(None),
        }
    }

    /// Start the migration tasks for load balancing.
    pub async fn start(&self) {
        let mut join_handles = self.join_handles.lock().await;
        if join_handles.len() > 0 {
            return;
        }

        // Spawn the per-vCPU tasks
        let num_vcpus = self.shared.scheduler.num_vcpus();
        for this_vcpu in 0..num_vcpus {
            // TODO: add affinity mask
            let task = MigrationTask::new(this_vcpu, self.shared.clone());
            let join_handle = crate::task::spawn(async move || {
                task.run().await;
            });
            join_handles.push(join_handle);
        }
    }

    /// Stop the migration tasks.
    pub async fn stop(&self) {
        let mut join_handles = self.join_handles.lock().await;
        if join_handles.len() == 0 {
            return;
        }

        let should_stop = &self.shared.should_stop;
        should_stop.store(true, Relaxed);
        self.shared.stop_wq.wake_all();

        for join_handle in join_handles {
            join_handle.await;
        }

        should_stop.store(false, Relaxed);
    }
}

struct MigrationTask {
    this_vcpu: u32,
    shared: Arc<Shared>,
}

impl MigrationTask {
    const WINDOW_SIZE: usize = 4;
    const INTERVAL_MS: u32 = 50;

    pub fn new(this_vcpu: u32, shared: Arc<Shared>) -> Self {
        Self {
            this_vcpu,
            shared,
        }
    }

    pub async fn run(self) {
        let mut waiter = Waiter::new();
        let stop_wq = &self.shared.stop_wq;
        stop_wq.enqueue(&mut waiter);
        while !shared.should_stop.load(Relaxed) {
            self.do_migration();

            let mut timeout = Duration::millisec(Self::INTERVAL_MS);
            let _ = waiter.wait_timeout(Some(&mut timeout)).await;
            waiter.reset();
        }
        stop_wq.dequeue(&mut waiter);
    }

    fn do_migratation(&self) {
        let mut num_migrated_tasks = 0;

        // Find vCPUs that are less busy than this vCPU. We only migrate 
        // tasks from this vCPU to less busy vCPUs.
        let this_vcpu = self.this_vcpu;
        let this_load = local_schedulers[this_vcpu].len();
        let local_schedulers = self.scheduler.local_schedulers();
        let mut less_busy_vcpus: Vec<(usize, u32)> = (0..num_vcpus)
            .map(|vcpu| {
                let load = local_schedulers[vcpu as usize].len()
                (vcpu, load)
            })
            .filter(|(vcpu, load)| {
                vcpu != this_vcpu && load < this_load
            })
            .collect();
        less_busy_vcpus.sort_unstable_by(|a, b| {
            let load_a = a.1;
            let load_b = b.1;
            load_a < load_b
        });

        // Migrate tasks by iterating the less busy vCPUs
        let this_scheduler = &local_schedulers[this_vcpu];
        for (dst_vcpu, dst_load) in less_busy_vcpus {
            // Get the latest load of this vCPU
            let this_load = this_scheduler.len();

            // The load of the src vCPU must be higher than the dst vCPU by 
            // at least 2.
            if this_load <= dst_load + 2{
                break;
            }

            let max_tasks_to_migrate = (this_load - dst_load) / 2;
            if max_tasks_to_migrate == 0 {
                break;
            }
            let migrated_tasks = this_scheduler.drain(
                max_tasks_to_migrate,
                |task| {
                    // Need to respect affinity when doing migration
                    let affinity = task.sched_state().affinity();
                    affinity.get(dst_vcpu)
                });
            num_migrated_tasks += migrated_tasks.len(); 

            dst_scheduler = &load_schedulers[dst_vcpu as usize];
            for task in tasks {
                dst_scheduler.enqueue(task);
            }
        }

        num_migrated_tasks
    }
}
