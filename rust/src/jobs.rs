//! Job registry: id allocation, states, spool dirs, per-job / per-client
//! override caches, and the busy flag with grace window.
//!
//! Port of the state-holding half of the Python `IppServer` class.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub paper_id: String,
    pub auth_value: String,
}

impl Overrides {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.paper_id.is_empty() && self.auth_value.is_empty()
    }
}

#[derive(Default)]
struct Inner {
    next_job_id: i32,
    jobs: HashMap<i32, PathBuf>,
    job_states: HashMap<i32, i32>,
    job_overrides: HashMap<i32, Overrides>,
    client_overrides: HashMap<String, Overrides>,
    busy_depth: u32,
    busy_grace_until: Option<Instant>,
}

pub struct JobRegistry {
    inner: Mutex<Inner>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner { next_job_id: 1, ..Default::default() }),
        }
    }

    pub fn job_activity_begin(&self) {
        self.inner.lock().unwrap().busy_depth += 1;
    }

    /// Keep reporting "processing" for a short grace window after the job so
    /// ~1s status pollers reliably observe the processing -> idle transition.
    pub fn job_activity_end(&self, grace: Duration) {
        let mut g = self.inner.lock().unwrap();
        g.busy_depth = g.busy_depth.saturating_sub(1);
        let until = Instant::now() + grace;
        g.busy_grace_until = Some(match g.busy_grace_until {
            Some(existing) if existing > until => existing,
            _ => until,
        });
    }

    pub fn printer_is_busy(&self) -> bool {
        let g = self.inner.lock().unwrap();
        g.busy_depth > 0 || g.busy_grace_until.is_some_and(|t| Instant::now() < t)
    }

    pub fn allocate_job_id(&self) -> i32 {
        let mut g = self.inner.lock().unwrap();
        let id = g.next_job_id;
        g.next_job_id += 1;
        id
    }

    pub fn register_job(&self, job_id: i32, spool_dir: PathBuf) {
        let mut g = self.inner.lock().unwrap();
        g.jobs.insert(job_id, spool_dir);
        g.job_states.insert(job_id, 3);
    }

    pub fn set_job_state(&self, job_id: i32, state: i32) {
        if job_id <= 0 {
            return;
        }
        self.inner.lock().unwrap().job_states.insert(job_id, state);
    }

    pub fn get_job_state(&self, job_id: i32, default: i32) -> i32 {
        if job_id <= 0 {
            return default;
        }
        *self.inner.lock().unwrap().job_states.get(&job_id).unwrap_or(&default)
    }

    pub fn list_jobs(&self) -> Vec<(i32, i32)> {
        let g = self.inner.lock().unwrap();
        let mut ids: Vec<_> = g.jobs.keys().copied().collect();
        ids.sort();
        ids.into_iter()
            .map(|id| (id, *g.job_states.get(&id).unwrap_or(&9)))
            .collect()
    }

    pub fn register_job_overrides(&self, job_id: i32, overrides: &Overrides) {
        self.inner.lock().unwrap().job_overrides.insert(
            job_id,
            Overrides {
                paper_id: overrides.paper_id.trim().to_string(),
                auth_value: overrides.auth_value.trim().to_string(),
            },
        );
    }

    pub fn get_job_overrides(&self, job_id: i32) -> Overrides {
        self.inner
            .lock()
            .unwrap()
            .job_overrides
            .get(&job_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_job_spool_dir(&self, job_id: i32) -> Option<PathBuf> {
        self.inner.lock().unwrap().jobs.get(&job_id).cloned()
    }

    /// Cache per-client (ip|user-agent) overrides; merge non-empty fields.
    pub fn register_client_overrides(&self, key: &str, overrides: &Overrides) {
        if key.is_empty() {
            return;
        }
        let paper_id = overrides.paper_id.trim();
        let auth_value = overrides.auth_value.trim();
        if paper_id.is_empty() && auth_value.is_empty() {
            return;
        }
        let mut g = self.inner.lock().unwrap();
        let entry = g.client_overrides.entry(key.to_string()).or_default();
        if !paper_id.is_empty() {
            entry.paper_id = paper_id.to_string();
        }
        if !auth_value.is_empty() {
            entry.auth_value = auth_value.to_string();
        }
    }

    pub fn get_client_overrides(&self, key: &str) -> Overrides {
        if key.is_empty() {
            return Overrides::default();
        }
        self.inner
            .lock()
            .unwrap()
            .client_overrides
            .get(key)
            .cloned()
            .unwrap_or_default()
    }
}
