use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkOrigin {
    Visible,
    Decorative,
    Prefetch,
}
#[derive(Clone, Copy, Debug)]
pub enum WorkStage {
    Network,
    Conversion,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum WorkError {
    Cancelled,
    Failed(String),
}
impl std::fmt::Display for WorkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Artwork request cancelled"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}
#[derive(Clone, Default)]
pub struct ArtworkScheduler {
    inner: Arc<Inner>,
}
#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    changed: Notify,
}
#[derive(Default)]
struct State {
    selected: Option<u32>,
    next: u64,
    requests: HashMap<String, Weak<Lease>>,
    network: StageState,
    conversion: StageState,
}
#[derive(Default)]
struct StageState {
    queue: Vec<Ticket>,
    active: HashMap<u64, Active>,
}
struct Ticket {
    id: u64,
    lease: Arc<Lease>,
}
struct Active {
    app_id: u32,
    reserved: bool,
    preempted: bool,
}
struct Lease {
    scheduler: Weak<Inner>,
    request_id: String,
    app_id: u32,
    origin: WorkOrigin,
    cancelled: AtomicBool,
}
#[derive(Clone)]
pub struct WorkHandle {
    lease: Arc<Lease>,
}
impl WorkHandle {
    pub fn is_cancelled(&self) -> bool {
        self.lease.cancelled.load(Ordering::Acquire)
    }
}
pub struct ConversionPermit {
    inner: Arc<Inner>,
    stage: WorkStage,
    id: u64,
    admitted: bool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(inner) = self.scheduler.upgrade() {
            if let Ok(mut state) = inner.state.lock() {
                state.requests.remove(&self.request_id);
            }
            inner.changed.notify_waiters();
        }
    }
}
impl Drop for ConversionPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.lock() {
            let stage = stage_mut(&mut state, self.stage);
            if self.admitted {
                stage.active.remove(&self.id);
            } else {
                stage.queue.retain(|ticket| ticket.id != self.id);
            }
        }
        self.inner.changed.notify_waiters();
    }
}
fn stage_mut(state: &mut State, stage: WorkStage) -> &mut StageState {
    match stage {
        WorkStage::Network => &mut state.network,
        WorkStage::Conversion => &mut state.conversion,
    }
}
fn priority(lease: &Lease, selected: Option<u32>) -> u8 {
    let origin = match lease.origin {
        WorkOrigin::Visible => 0,
        WorkOrigin::Decorative => 1,
        WorkOrigin::Prefetch => 2,
    };
    if selected == Some(lease.app_id) {
        origin
    } else {
        origin + 3
    }
}
impl ArtworkScheduler {
    pub fn begin(
        &self,
        request_id: String,
        app_id: u32,
        origin: WorkOrigin,
    ) -> Result<WorkHandle, WorkError> {
        if request_id.is_empty() || request_id.len() > 128 || app_id == 0 {
            return Err(WorkError::Failed("Invalid artwork request".into()));
        }
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| WorkError::Failed("Artwork scheduler unavailable".into()))?;
        if let Some(lease) = state.requests.get(&request_id).and_then(Weak::upgrade) {
            drop(state);
            if lease.app_id != app_id || lease.origin != origin {
                return Err(WorkError::Failed(
                    "Artwork request identity mismatch".into(),
                ));
            }
            if lease.cancelled.load(Ordering::Acquire) {
                return Err(WorkError::Cancelled);
            }
            return Ok(WorkHandle { lease });
        }
        let lease = Arc::new(Lease {
            scheduler: Arc::downgrade(&self.inner),
            request_id: request_id.clone(),
            app_id,
            origin,
            cancelled: AtomicBool::new(false),
        });
        state.requests.insert(request_id, Arc::downgrade(&lease));
        Ok(WorkHandle { lease })
    }
    pub fn select_game(&self, app_id: Option<u32>) -> Result<(), WorkError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| WorkError::Failed("Artwork scheduler unavailable".into()))?;
        state.selected = app_id;
        for active in state.network.active.values_mut() {
            if active.reserved && Some(active.app_id) != app_id {
                active.preempted = true;
            }
        }
        drop(state);
        self.inner.changed.notify_waiters();
        Ok(())
    }
    pub fn cancel(&self, request_id: &str) -> Result<(), WorkError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| WorkError::Failed("Artwork scheduler unavailable".into()))?;
        let lease = state.requests.get(request_id).and_then(Weak::upgrade);
        drop(state);
        if let Some(lease) = lease {
            lease.cancelled.store(true, Ordering::Release);
        }
        self.inner.changed.notify_waiters();
        Ok(())
    }
    async fn acquire(
        &self,
        work: &WorkHandle,
        stage: WorkStage,
    ) -> Result<ConversionPermit, WorkError> {
        let id = {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| WorkError::Failed("Artwork scheduler unavailable".into()))?;
            state.next += 1;
            let id = state.next;
            stage_mut(&mut state, stage).queue.push(Ticket {
                id,
                lease: work.lease.clone(),
            });
            id
        };
        let mut permit = ConversionPermit {
            inner: self.inner.clone(),
            stage,
            id,
            admitted: false,
        };
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if work.lease.cancelled.load(Ordering::Acquire) {
                return Err(WorkError::Cancelled);
            }
            let admitted = {
                let mut state = self
                    .inner
                    .state
                    .lock()
                    .map_err(|_| WorkError::Failed("Artwork scheduler unavailable".into()))?;
                let selected = state.selected;
                let target = stage_mut(&mut state, stage);
                let total = match stage {
                    WorkStage::Network => 5,
                    WorkStage::Conversion => 2,
                };
                let ordinary = target
                    .active
                    .values()
                    .filter(|active| !active.reserved)
                    .count();
                let chosen = target
                    .queue
                    .iter()
                    .filter(|t| !t.lease.cancelled.load(Ordering::Acquire))
                    .filter(|t| {
                        target.active.len() < total
                            && (selected == Some(t.lease.app_id) || ordinary < total - 1)
                    })
                    .min_by_key(|t| (priority(&t.lease, selected), t.id))
                    .map(|t| t.id);
                if chosen == Some(id) {
                    target.queue.retain(|t| t.id != id);
                    target.active.insert(
                        id,
                        Active {
                            app_id: work.lease.app_id,
                            reserved: selected == Some(work.lease.app_id),
                            preempted: false,
                        },
                    );
                    true
                } else {
                    false
                }
            };
            if admitted {
                permit.admitted = true;
                self.inner.changed.notify_waiters();
                return Ok(permit);
            }
            notified.await;
        }
    }
    pub async fn acquire_conversion(
        &self,
        work: &WorkHandle,
    ) -> Result<ConversionPermit, WorkError> {
        self.acquire(work, WorkStage::Conversion).await
    }
    pub async fn run_network<T, F: Future<Output = Result<T, String>>>(
        &self,
        work: &WorkHandle,
        mut operation: impl FnMut() -> F,
    ) -> Result<T, WorkError> {
        loop {
            let permit = self.acquire(work, WorkStage::Network).await?;
            let future = operation();
            tokio::pin!(future);
            loop {
                let notified = self.inner.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if work.lease.cancelled.load(Ordering::Acquire) {
                    return Err(WorkError::Cancelled);
                }
                let preempted = self
                    .inner
                    .state
                    .lock()
                    .map_err(|_| WorkError::Failed("Artwork scheduler unavailable".into()))?
                    .network
                    .active
                    .get(&permit.id)
                    .is_some_and(|active| active.preempted);
                if preempted {
                    break;
                }
                tokio::select! { result = &mut future => return result.map_err(WorkError::Failed), _ = &mut notified => {} }
            }
            drop(permit);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    async fn flush() {
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
    }
    fn work(s: &ArtworkScheduler, id: &str, app: u32) -> WorkHandle {
        s.begin(id.into(), app, WorkOrigin::Visible).unwrap()
    }
    #[tokio::test]
    async fn ordinary_network_leaves_urgent_capacity() {
        let scheduler = ArtworkScheduler::default();
        let started = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(Notify::new());
        let mut tasks = Vec::new();
        for id in 0..5 {
            let s = scheduler.clone();
            let count = started.clone();
            let stop = stop.clone();
            let w = work(&s, &format!("normal-{id}"), id + 1);
            tasks.push(tokio::spawn(async move {
                s.run_network(&w, || async {
                    count.fetch_add(1, Ordering::SeqCst);
                    stop.notified().await;
                    Ok(())
                })
                .await
            }));
        }
        flush().await;
        assert_eq!(started.load(Ordering::SeqCst), 4);
        scheduler.select_game(Some(99)).unwrap();
        let w = work(&scheduler, "urgent", 99);
        scheduler
            .run_network(&w, || async {
                assert_eq!(started.load(Ordering::SeqCst), 4);
                Ok(())
            })
            .await
            .unwrap();
        for id in 0..5 {
            scheduler.cancel(&format!("normal-{id}")).unwrap();
        }
        for task in tasks {
            assert!(matches!(task.await.unwrap(), Err(WorkError::Cancelled)));
        }
        assert!(
            scheduler
                .inner
                .state
                .lock()
                .unwrap()
                .network
                .active
                .is_empty()
        );
    }
    #[tokio::test]
    async fn selection_promotes_pending_work() {
        let s = ArtworkScheduler::default();
        let mut permits = Vec::new();
        let mut handles = Vec::new();
        for id in 1..=4 {
            let w = work(&s, &format!("held-{id}"), id);
            permits.push(s.acquire(&w, WorkStage::Network).await.unwrap());
            handles.push(w);
        }
        let a = work(&s, "a", 10);
        let b = work(&s, "b", 11);
        let sa = s.clone();
        let sb = s.clone();
        let ta = tokio::spawn(async move { sa.acquire(&a, WorkStage::Network).await });
        let tb = tokio::spawn(async move { sb.acquire(&b, WorkStage::Network).await });
        flush().await;
        assert!(!ta.is_finished() && !tb.is_finished());
        s.select_game(Some(11)).unwrap();
        flush().await;
        assert!(tb.is_finished());
        assert!(!ta.is_finished());
        drop(tb.await.unwrap().unwrap());
        s.cancel("a").unwrap();
        assert!(matches!(ta.await.unwrap(), Err(WorkError::Cancelled)));
    }
    #[tokio::test]
    async fn selection_preempts_obsolete_urgent_transfer() {
        let s = ArtworkScheduler::default();
        let mut held = Vec::new();
        let mut handles = Vec::new();
        for id in 1..=4 {
            let w = work(&s, &format!("held-{id}"), id);
            held.push(s.acquire(&w, WorkStage::Network).await.unwrap());
            handles.push(w);
        }
        s.select_game(Some(10)).unwrap();
        let a = work(&s, "a", 10);
        let sa = s.clone();
        let starts = Arc::new(AtomicUsize::new(0));
        let count = starts.clone();
        let ta = tokio::spawn(async move {
            sa.run_network(&a, || async {
                count.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<Result<(), String>>().await
            })
            .await
        });
        flush().await;
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        s.select_game(Some(11)).unwrap();
        let b = work(&s, "b", 11);
        s.run_network(&b, || async { Ok(()) }).await.unwrap();
        s.select_game(Some(10)).unwrap();
        flush().await;
        assert_eq!(starts.load(Ordering::SeqCst), 2);
        s.cancel("a").unwrap();
        assert!(matches!(ta.await.unwrap(), Err(WorkError::Cancelled)));
    }
    #[tokio::test]
    async fn conversion_reserves_capacity_without_preempting_codec() {
        let s = ArtworkScheduler::default();
        let a = work(&s, "a", 1);
        let b = work(&s, "b", 2);
        let first = s.acquire_conversion(&a).await.unwrap();
        let sb = s.clone();
        let normal = tokio::spawn(async move { sb.acquire_conversion(&b).await });
        flush().await;
        assert!(!normal.is_finished());
        s.select_game(Some(3)).unwrap();
        let c = work(&s, "c", 3);
        let urgent = s.acquire_conversion(&c).await.unwrap();
        s.select_game(Some(4)).unwrap();
        let d = work(&s, "d", 4);
        let sd = s.clone();
        let next = tokio::spawn(async move { sd.acquire_conversion(&d).await });
        flush().await;
        assert!(!next.is_finished());
        drop(urgent);
        flush().await;
        assert!(next.is_finished());
        assert!(!normal.is_finished());
        drop(next.await.unwrap().unwrap());
        drop(first);
        assert!(normal.await.unwrap().is_ok());
    }
    #[tokio::test]
    async fn cancelled_waiter_releases_queue_and_capacity() {
        let s = ArtworkScheduler::default();
        let a = work(&s, "a", 1);
        let b = work(&s, "b", 2);
        let p = s.acquire_conversion(&a).await.unwrap();
        let sb = s.clone();
        let t = tokio::spawn(async move { sb.acquire_conversion(&b).await });
        flush().await;
        s.cancel("b").unwrap();
        assert!(matches!(t.await.unwrap(), Err(WorkError::Cancelled)));
        drop(p);
        assert!(s.inner.state.lock().unwrap().conversion.queue.is_empty());
        let shared = s.begin("shared".into(), 3, WorkOrigin::Visible).unwrap();
        let shared2 = s.begin("shared".into(), 3, WorkOrigin::Visible).unwrap();
        assert!(Arc::ptr_eq(&shared.lease, &shared2.lease));
        assert!(s.begin("shared".into(), 4, WorkOrigin::Visible).is_err());
    }
}
