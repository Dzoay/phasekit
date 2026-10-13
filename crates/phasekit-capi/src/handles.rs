//! Generational handles (map 11 F1, F3; ROT-150): a handle is a `u64` whose low 32 bits are a slot's index + 1 and high 32
//! its generation, so 0 is never a handle and a freed one never names the value that later takes its slot. A call
//! clones the slot's `Arc` under the table's lock and works on its own clone, so freeing a handle another thread is
//! using leaves that call intact and makes every later one `PK_STALE_HANDLE`. One of the two modules allowed shared
//! state (D8, E13).

#![allow(clippy::disallowed_types, reason = "the C ABI's handle tables, one lock each (D11, E13)")]

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use phasekit_core::{Fluid, Registry};

/// The registries `pk_registry_*` made.
pub(crate) fn registries() -> &'static Mutex<Table<Registry>> {
    static REGISTRIES: Mutex<Table<Registry>> = Mutex::new(Table::new());
    &REGISTRIES
}

/// The fluids `pk_fluid_new` made.
pub(crate) fn fluids() -> &'static Mutex<Table<Fluid>> {
    static FLUIDS: Mutex<Table<Fluid>> = Mutex::new(Table::new());
    &FLUIDS
}

/// A table's lock; a panic while it was held (none is reachable) leaves the table consistent, so a poisoned lock is
/// taken as it is.
pub(crate) fn lock<T>(table: &Mutex<T>) -> MutexGuard<'_, T> {
    table.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Values behind generational handles.
pub(crate) struct Table<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

struct Slot<T> {
    generation: u32,
    value: Option<Arc<T>>,
}

impl<T> Table<T> {
    /// An empty table.
    pub(crate) const fn new() -> Self {
        Table { slots: Vec::new(), free: Vec::new() }
    }

    /// A new handle for `value`; `None` once every index is taken.
    pub(crate) fn insert(&mut self, value: T) -> Option<u64> {
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                let index = u32::try_from(self.slots.len()).ok().filter(|i| *i < u32::MAX)?;
                self.slots.push(Slot { generation: 1, value: None });
                index
            }
        };
        let slot = self.slots.get_mut(index as usize)?;
        slot.value = Some(Arc::new(value));
        Some(handle(index, slot.generation))
    }

    /// The value `h` names, if it is live.
    pub(crate) fn get(&self, h: u64) -> Option<Arc<T>> {
        let (index, generation) = split(h)?;
        self.slots.get(index as usize).filter(|s| s.generation == generation).and_then(|s| s.value.clone())
    }

    /// Frees `h`, returning its value if it was live. The slot's generation moves on, and a slot whose generation would
    /// wrap is retired instead of reused, so no two values ever share a handle.
    pub(crate) fn remove(&mut self, h: u64) -> Option<Arc<T>> {
        let (index, generation) = split(h)?;
        let slot = self.slots.get_mut(index as usize).filter(|s| s.generation == generation)?;
        let value = slot.value.take()?;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(index);
        }
        Some(value)
    }

    /// Test support: the next handle of a fresh slot starts at `generation`.
    #[cfg(test)]
    pub(crate) fn starting_at(generation: u32) -> Self {
        Table { slots: vec![Slot { generation, value: None }], free: vec![0] }
    }
}

fn handle(index: u32, generation: u32) -> u64 {
    (u64::from(generation) << 32) | (u64::from(index) + 1)
}

fn split(h: u64) -> Option<(u32, u32)> {
    let index = u32::try_from(h & 0xffff_ffff).ok()?.checked_sub(1)?;
    Some((index, u32::try_from(h >> 32).ok()?))
}
