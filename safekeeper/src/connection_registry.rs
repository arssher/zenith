//! A shared map storing for each ttid list of Arcs to entries, providing
//! registration of new entries and a guard implementing unregister in Drop.
//! Used for storing state of walreceivers and walsenders.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};
use tracing::*;
use utils::id::TenantTimelineId;

type EntryId = u32; // id of entry

static walsenders: Lazy<ConnectionRegistry<u32>> = Lazy::new(|| ConnectionRegistry::new());

pub struct ConnectionRegistry<T> {
    registry: Arc<RwLock<SharedState<T>>>,
}

// impl is manual to not demand Clone for T.
impl<T> Clone for ConnectionRegistry<T> {
    fn clone(&self) -> Self {
        ConnectionRegistry {
            registry: self.registry.clone(),
        }
    }
}

impl<T> ConnectionRegistry<T> {
    /// Create new registry. Usage example:
    ///
    /// ```
    /// use once_cell::sync::Lazy;
    ///
    /// struct Apple {roundness: u32}
    /// static apples: Lazy<ConnectionRegistry<Apple>> = Lazy::new(|| { ConnectionRegistry::new() });
    /// ```
    pub fn new() -> Self {
        ConnectionRegistry {
            registry: Arc::new(RwLock::new(SharedState::new())),
        }
    }

    /// Register new entry with provided data. Returned handle on drop
    /// unregisters it.
    pub fn register(&self, ttid: TenantTimelineId, data: T) -> EntryGuard<T> {
        let entry = self.registry.write().register(ttid, data);
        trace!("registered {}/{}", ttid, entry.id);
        EntryGuard {
            ttid,
            id: entry.id,
            registry: self.clone(),
            data: entry.data,
        }
    }

    fn unregister(&self, ttid: TenantTimelineId, id: EntryId) {
        self.registry.write().unregister(ttid, id);
        trace!("unregistered {ttid}/{id}");
    }
}

/// Handle returned to the user. On drop, unregisters from the map.
pub struct EntryGuard<T> {
    ttid: TenantTimelineId,
    id: EntryId,
    // Backlink to registry from where entry is unregistered in Drop. It would
    // be natural to statically determine it since registry is static in our
    // usage, but dunno how to do that.
    registry: ConnectionRegistry<T>,
    data: Arc<T>,
}

impl<T> Drop for EntryGuard<T> {
    fn drop(&mut self) {
        self.registry.unregister(self.ttid, self.id);
    }
}

#[derive(Default)]
struct SharedState<T> {
    // We assume only a few entries for each ttid, so Vec is more suitable than
    // map.
    entries: HashMap<TenantTimelineId, Vec<Entry<T>>>,
    last_issued_id: EntryId,
}

struct Entry<T> {
    id: EntryId,
    data: Arc<T>,
}

// impl is manual to not demand Clone for T.
impl<T> Clone for Entry<T> {
    fn clone(&self) -> Self {
        Entry {
            id: self.id,
            data: self.data.clone(),
        }
    }
}

impl<T> SharedState<T> {
    fn new() -> Self {
        SharedState {
            entries: HashMap::new(),
            last_issued_id: 0,
        }
    }

    // find free id and register new entry
    fn register(&mut self, ttid: TenantTimelineId, data: T) -> Entry<T> {
        let id = self.issue_id(ttid);
        let entry = Entry {
            id,
            data: Arc::new(data),
        };
        let v = self.entries.entry(ttid).or_insert(Vec::new());
        v.push(entry.clone());
        entry
    }

    // Find an unused id. Most likely it will succeed on the first iteration.
    fn issue_id(&mut self, ttid: TenantTimelineId) -> EntryId {
        if self.entries.get(&ttid).map(|v| v.len()).unwrap_or(0) as u32 == EntryId::MAX {
            // too many entries
            assert!(false);
        }
        let mut candidate = self.last_issued_id;
        // per check above we must find free id
        loop {
            candidate = candidate.wrapping_add(1);
            let is_busy = match self.entries.get(&ttid) {
                Some(v) => v.iter().any(|e| e.id == candidate),
                None => false,
            };
            if !is_busy {
                self.last_issued_id = candidate;
                return candidate;
            }
        }
    }

    // Remove the entry. Panicks if it is not found.
    fn unregister(&mut self, ttid: TenantTimelineId, id: EntryId) {
        match self.entries.get_mut(&ttid) {
            Some(v) => {
                let prev_len = v.len();
                v.retain(|e| e.id != id);
                assert!(
                    v.len() - prev_len == 1,
                    "failed to find element ttid {}, id {} in the vec",
                    ttid,
                    id
                );
                if v.is_empty() {
                    self.entries.remove(&ttid);
                }
            }
            None => assert!(
                false,
                "failed to find element ttid {}, id {} in the map",
                ttid, id
            ),
        }
    }
}
