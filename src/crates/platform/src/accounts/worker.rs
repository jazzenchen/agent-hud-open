//! The worker: one reading at a time, of an account or a status page, each
//! under the collection gate, saving the restart copy as readings land.
use super::{
    restart_copy, schedule::due_after, settle::settle, status, AccountContext, CollectionGate,
    PageReading, Providers, Shared,
};
use crate::clients::claude::cloud::{Collector as CloudCollector, Reading as CloudReading};
use agent_hud_agents as agents;
use agents::account::Provider;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::Ordering, Arc, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// UsageRefresh.accountStepBudget: after this long of back-to-back readings the
/// worker waits as long again, so a local pass is not held behind the queue.
const STEP_BUDGET: Duration = Duration::from_secs(1);

/// What the worker does next: read a provider's account, or a status page.
enum Job {
    Account(
        &'static str,
        &'static Provider,
        HashMap<String, Vec<PathBuf>>,
    ),
    Page(&'static agents::account::StatusPage),
    DesktopCloud(u64),
}

pub(super) fn read_desktop_cloud_job<F>(
    cancelled: &std::sync::atomic::AtomicBool,
    read: F,
) -> Option<(CloudReading, i64)>
where
    F: FnOnce() -> CloudReading,
{
    if cancelled.load(Ordering::Acquire) {
        return None;
    }
    let reading = read();
    if cancelled.load(Ordering::Acquire) {
        return None;
    }
    Some((reading, chrono::Utc::now().timestamp_millis()))
}

pub(super) fn run(
    context: AccountContext,
    shared: Arc<Shared>,
    gate: CollectionGate,
    readers: Providers,
    cloud_supported: bool,
) {
    let mut burst: Option<Instant> = None;
    let mut resume_at: Option<Instant> = None;
    let mut saved: Option<Instant> = None;
    let mut cloud = CloudCollector::default();
    loop {
        let selected = {
            let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if state.cloud_reset {
                    cloud = CloudCollector::default();
                    state.cloud_reset = false;
                }
                if state.stopped {
                    return;
                }
                // Force remains pending until each enabled account satisfies the
                // same 60s request spacing as the original collector.
                let mut pause = Duration::from_secs(300);
                let mut due = None;
                for &facts in readers {
                    let provider = facts.id;
                    if !state.enabled.contains(provider) {
                        continue;
                    }
                    let wait = state
                        .entries
                        .get(provider)
                        .map_or(Duration::ZERO, |entry| due_after(facts, entry, &state));
                    if wait.is_zero() {
                        due = Some((provider, facts));
                        break;
                    }
                    pause = pause.min(wait);
                }
                let now = Instant::now();
                let claude_enabled = state.enabled.contains(agents::claude::AGENT.id);
                let cloud_due = cloud_supported && claude_enabled && cloud.due();
                if cloud_supported && claude_enabled {
                    pause = pause.min(cloud.next_due().saturating_duration_since(now));
                }
                // A status page waits behind every account reading that is due.
                let mut page = None;
                if due.is_none() && !cloud_due {
                    for &facts in readers {
                        let Some(status) =
                            facts.status.filter(|_| state.enabled.contains(facts.id))
                        else {
                            continue;
                        };
                        let wait = state.pages.get(status.url).map_or(Duration::ZERO, |read| {
                            status::INTERVAL.saturating_sub(read.attempted.elapsed())
                        });
                        if wait.is_zero() {
                            page = Some(status);
                            break;
                        }
                        pause = pause.min(wait);
                    }
                }
                if let Some(at) = resume_at.filter(|at| *at > now) {
                    pause = at - now;
                } else if due.is_some() || cloud_due || page.is_some() {
                    if burst.is_none_or(|started| started.elapsed() < STEP_BUDGET) {
                        resume_at = None;
                        let Some(selected) = due else {
                            if cloud_due {
                                break Job::DesktopCloud(state.cloud_sequence);
                            }
                            break Job::Page(page.expect("a page is due"));
                        };
                        // Consume the request this job serves. A callback arriving
                        // during I/O remains pending for the next permitted pass.
                        state.force.remove(selected.0);
                        break Job::Account(selected.0, selected.1, state.account_homes.clone());
                    }
                    burst = None;
                    resume_at = Some(now + STEP_BUDGET);
                    pause = STEP_BUDGET;
                } else {
                    burst = None;
                }
                state = shared
                    .wake
                    .wait_timeout(state, pause)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        };
        burst.get_or_insert_with(Instant::now);
        let selected = match selected {
            Job::Account(provider, facts, homes) => (provider, facts, homes),
            Job::DesktopCloud(sequence) => {
                let Some((reading, observed_ms)) =
                    read_desktop_cloud_job(&context.cancelled, || {
                        cloud.collect(&context.home, &gate)
                    })
                else {
                    return;
                };
                let waker = {
                    let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
                    if state.stopped {
                        return;
                    }
                    if !state.enabled.contains(agents::claude::AGENT.id)
                        || sequence != state.cloud_sequence
                    {
                        continue;
                    }
                    state.cloud_sequence = state.cloud_sequence.wrapping_add(1);
                    state.cloud_completion = Some(super::CloudCompletion {
                        sequence: state.cloud_sequence,
                        reading,
                        observed_ms,
                    });
                    state.waker.clone()
                };
                if let Some(waker) = waker {
                    waker();
                }
                continue;
            }
            Job::Page(page) => {
                let attempted = Instant::now();
                let summary = {
                    let _turn = gate.lock().unwrap_or_else(PoisonError::into_inner);
                    if context.cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    status::read(&context.http, page.url)
                };
                let waker = {
                    let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
                    // A page that could not be read keeps what it said last.
                    let old = state.pages.remove(page.url);
                    let changed = summary.is_some()
                        && old.as_ref().and_then(|old| old.summary.as_ref()) != summary.as_ref();
                    let (checked_at, summary) = match summary {
                        Some(summary) => (Some(chrono::Utc::now().to_rfc3339()), Some(summary)),
                        None => old
                            .map(|old| (old.checked_at, old.summary))
                            .unwrap_or_default(),
                    };
                    state.pages.insert(
                        page.url,
                        PageReading {
                            attempted,
                            checked_at,
                            summary,
                        },
                    );
                    changed.then(|| state.waker.clone()).flatten()
                };
                if let Some(waker) = waker {
                    waker();
                }
                continue;
            }
        };
        let attempted = Instant::now();
        let attempted_ms = chrono::Utc::now().timestamp_millis();
        // The gate, not the shared state or the Runtime, is held across the I/O.
        let result = {
            let _turn = gate.lock().unwrap_or_else(PoisonError::into_inner);
            if context.cancelled.load(Ordering::Acquire) {
                return;
            }
            let mut reading_context = context.clone();
            reading_context.account_homes = selected.2.clone();
            (selected.1.read)(&reading_context)
        };
        let (waker, copy) = {
            let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.stopped {
                return;
            }
            // A removed home must not reappear when its in-flight read completes.
            if state.account_homes.get(selected.0) != selected.2.get(selected.0) {
                continue;
            }
            let spread = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as u64
                % 61;
            settle(
                &mut state.entries,
                selected.1,
                result,
                (attempted, attempted_ms),
                spread,
            );
            let copy = saved
                .is_none_or(|at| at.elapsed() >= restart_copy::SAVE_INTERVAL)
                .then(|| restart_copy::copy(&state.entries, &state.account_homes));
            (state.waker.clone(), copy)
        };
        if let Some(copy) = copy {
            saved = Some(Instant::now());
            restart_copy::save(&context.data_dir, &copy);
        }
        if let Some(waker) = waker {
            waker();
        }
    }
}
