use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

use crate::scheduler::local_scheduler::StatusNotifier;
use crate::scheduler::SchedState;
use crate::util::AtomicBits;

/// vCPU selector.
///
/// A vCPU selector decides which vCPU a schedulable entity should run on.
/// vCPU selectors are designed to make _fast_ and _sensible_ vCPU
/// selection decisions. As such, decisions are made by following
/// a set of simple rules.
///
/// First and foremost, vCPU assignment must respect the
/// affinity mask of an entity. Beyond that, vCPU selectors adopt some heuristics
/// to make sensible decisions. The basic idea is to prioritize vCPUs that
/// satisfy the following conditions.
///
/// 1. Idle vCPUs (which are busy looping for available entities to schedule);
/// 2. Active vCPUs (which are not sleeping);
/// 3. The last vCPU that the entity runs on;
/// 4. The current vCPU that is making the selection.
///
/// If no such vCPUs are in the affinity, then a vCPU selector
/// picks a vCPU in a round-robin fashion so that
/// workloads are more liekly to spread across multiple vCPUs evenly.
pub struct VcpuSelector {
    idle_vcpu_mask: AtomicBits,
    sleep_vcpu_mask: AtomicBits,
    num_vcpus: u32,
}

impl StatusNotifier for Arc<VcpuSelector> {
    fn notify_idle_status(&self, vcpu: u32, is_idle: bool) {
        self.idle_vcpu_mask.set(vcpu as usize, is_idle);
    }

    fn notify_sleep_status(&self, vcpu: u32, is_sleep: bool) {
        self.sleep_vcpu_mask.set(vcpu as usize, is_sleep);
    }
}

impl VcpuSelector {
    /// Create an instance.
    pub fn new(num_vcpus: u32) -> Self {
        Self {
            idle_vcpu_mask: AtomicBits::new_zeroes(num_vcpus as usize),
            sleep_vcpu_mask: AtomicBits::new_zeroes(num_vcpus as usize),
            num_vcpus,
        }
    }

    /// Select the vCPU for an entity, given its state.
    ///
    /// If the current thread is used as a vCPU, then the vCPU number should
    /// be provided.
    pub fn select_vcpu(&self, sched_state: &SchedState, has_this_vcpu: Option<u32>) -> u32 {
        // Need to respect the CPU affinity mask
        let affinity = sched_state.affinity();
        debug_assert!(affinity.iter_ones().count() > 0);
        // Check whether this vCPU is in the affinity mask
        let has_this_vcpu = {
            if let Some(this_vcpu) = has_this_vcpu {
                if affinity.get(this_vcpu as usize) {
                    Some(this_vcpu)
                } else {
                    None
                }
            } else {
                None
            }
        };
        // Check whether the last vCPU is in the affinity mask
        let has_last_vcpu = {
            if let Some(last_vcpu) = sched_state.vcpu() {
                if affinity.get(last_vcpu as usize) {
                    Some(last_vcpu)
                } else {
                    None
                }
            } else {
                None
            }
        };

        // 1. This vCPU, if it is idle
        if let Some(this_vcpu) = has_this_vcpu {
            if self.idle_vcpu_mask.get(this_vcpu as usize) {
                return this_vcpu;
            }
        }
        // 2. The last vCPU that the entity runs on, if it is idle
        if let Some(last_vcpu) = has_last_vcpu {
            if self.idle_vcpu_mask.get(last_vcpu as usize) {
                return last_vcpu;
            }
        }
        // 3. Any idle vCPU
        let has_idle_vcpu = self
            .idle_vcpu_mask
            .iter_ones()
            .find(|idle_vcpu| affinity.get(*idle_vcpu));
        if let Some(idle_vcpu) = has_idle_vcpu {
            return idle_vcpu as u32;
        }

        // 4. The last vCPU that the entity runs on, if it is active (not sleeping)
        if let Some(last_vcpu) = has_last_vcpu {
            if !self.sleep_vcpu_mask.get(last_vcpu as usize) {
                return last_vcpu;
            }
        }
        // 5. Any active (non-sleeping) vCPU
        let has_active_vcpu = self
            .sleep_vcpu_mask
            .iter_zeroes()
            .find(|active_vcpu| affinity.get(*active_vcpu));
        if let Some(active_vcpu) = has_active_vcpu {
            return active_vcpu as u32;
        }

        // 6. The last vCPU that the entity runs on, regardless of whether it is
        // active or not (as long as it is in the affinity mask)
        if let Some(last_vcpu) = has_last_vcpu {
            return last_vcpu;
        }

        // 7. Any vCPU that is in the affinity mask
        //
        // Arriving at this step means the affinity mask must have changed or
        // this is the first time that an entity is scheduled (no last vCPU).
        // Either way, this is a rare situation. So it is ok to spend a little
        // bit more CPU cycless to select a vCPU. The heuristic is to select
        // the vCPU in a round-robin fashion so that it is more likely that
        // workloads are spreaded evenly across the vCPUs.
        loop {
            static NEXT_VCPU: AtomicU32 = AtomicU32::new(0);
            let vcpu = NEXT_VCPU.fetch_add(1, Relaxed) % self.num_vcpus;
            if affinity.get(vcpu as usize) {
                return vcpu;
            }
        }
    }
}
