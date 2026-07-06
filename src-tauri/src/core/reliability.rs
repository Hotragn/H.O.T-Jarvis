//! Free-tier hygiene (§2): response caching and per-provider backoff.
//! Everything here is pure bookkeeping with an injected clock, so it
//! unit-tests without HTTP or sleeps.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// Stable key for "the same request": hash of the serialized message list.
pub fn cache_key(serialized_request: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    serialized_request.hash(&mut hasher);
    hasher.finish()
}

/// Exponential backoff: 30s, 60s, 120s, … capped at 10 minutes.
pub fn backoff_delay(consecutive_failures: u32) -> Duration {
    let exp = consecutive_failures.saturating_sub(1).min(5);
    let secs = 30u64.saturating_mul(1u64 << exp);
    Duration::from_secs(secs.min(600))
}

#[derive(Default)]
struct CooldownState {
    failures: u32,
    until: Option<Instant>,
}

/// Tracks which providers are rate-limited and until when. A provider on
/// cooldown gets skipped in the routing plan instead of hammered.
#[derive(Default)]
pub struct CooldownTracker {
    states: HashMap<&'static str, CooldownState>,
}

impl CooldownTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn available(&self, provider: &'static str, now: Instant) -> bool {
        match self.states.get(provider).and_then(|s| s.until) {
            Some(until) => now >= until,
            None => true,
        }
    }

    /// Records a rate-limit hit. Honors the provider's Retry-After when
    /// given; otherwise backs off exponentially per consecutive failure.
    pub fn penalize(
        &mut self,
        provider: &'static str,
        retry_after: Option<Duration>,
        now: Instant,
    ) {
        let state = self.states.entry(provider).or_default();
        state.failures += 1;
        let delay = retry_after.unwrap_or_else(|| backoff_delay(state.failures));
        state.until = Some(now + delay);
    }

    /// A success clears the slate.
    pub fn reset(&mut self, provider: &'static str) {
        self.states.remove(provider);
    }
}

struct CacheEntry<V> {
    value: V,
    stored_at: Instant,
    seq: u64,
}

/// Small TTL + capacity cache for chat replies. Dedupes identical requests
/// (double-clicked send, retries) so free tiers aren't billed twice for the
/// same answer.
pub struct ResponseCache<V> {
    capacity: usize,
    ttl: Duration,
    next_seq: u64,
    entries: HashMap<u64, CacheEntry<V>>,
}

impl<V: Clone> ResponseCache<V> {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            capacity: capacity.max(1),
            ttl,
            next_seq: 0,
            entries: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: u64, now: Instant) -> Option<V> {
        match self.entries.get(&key) {
            Some(entry) if now.duration_since(entry.stored_at) < self.ttl => {
                Some(entry.value.clone())
            }
            Some(_) => {
                self.entries.remove(&key);
                None
            }
            None => None,
        }
    }

    pub fn put(&mut self, key: u64, value: V, now: Instant) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            // Evict the oldest insertion.
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.seq)
                .map(|(k, _)| *k)
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            CacheEntry {
                value,
                stored_at: now,
                seq: self.next_seq,
            },
        );
        self.next_seq += 1;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_stable_and_input_sensitive() {
        assert_eq!(cache_key("abc"), cache_key("abc"));
        assert_ne!(cache_key("abc"), cache_key("abd"));
    }

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_delay(1), Duration::from_secs(30));
        assert_eq!(backoff_delay(2), Duration::from_secs(60));
        assert_eq!(backoff_delay(3), Duration::from_secs(120));
        assert_eq!(backoff_delay(10), Duration::from_secs(600), "capped");
        assert_eq!(
            backoff_delay(0),
            Duration::from_secs(30),
            "0 treated as first"
        );
    }

    #[test]
    fn cooldown_blocks_then_releases() {
        let now = Instant::now();
        let mut tracker = CooldownTracker::new();
        assert!(tracker.available("groq", now));

        tracker.penalize("groq", None, now);
        assert!(!tracker.available("groq", now));
        assert!(!tracker.available("groq", now + Duration::from_secs(29)));
        assert!(tracker.available("groq", now + Duration::from_secs(30)));

        // Second consecutive failure backs off longer.
        tracker.penalize("groq", None, now);
        assert!(!tracker.available("groq", now + Duration::from_secs(59)));
        assert!(tracker.available("groq", now + Duration::from_secs(60)));
    }

    #[test]
    fn cooldown_honors_retry_after_and_reset() {
        let now = Instant::now();
        let mut tracker = CooldownTracker::new();
        tracker.penalize("groq", Some(Duration::from_secs(7)), now);
        assert!(!tracker.available("groq", now + Duration::from_secs(6)));
        assert!(tracker.available("groq", now + Duration::from_secs(7)));

        tracker.penalize("groq", None, now);
        tracker.reset("groq");
        assert!(tracker.available("groq", now));
    }

    #[test]
    fn cache_roundtrip_and_ttl_expiry() {
        let now = Instant::now();
        let mut cache: ResponseCache<String> = ResponseCache::new(8, Duration::from_secs(600));
        let key = cache_key("request");
        assert!(cache.get(key, now).is_none());

        cache.put(key, "answer".into(), now);
        assert_eq!(cache.get(key, now).as_deref(), Some("answer"));
        assert_eq!(
            cache.get(key, now + Duration::from_secs(599)).as_deref(),
            Some("answer")
        );
        assert!(cache.get(key, now + Duration::from_secs(600)).is_none());
        assert!(cache.is_empty(), "expired entry is dropped");
    }

    #[test]
    fn cache_evicts_oldest_when_full() {
        let now = Instant::now();
        let mut cache: ResponseCache<u32> = ResponseCache::new(2, Duration::from_secs(600));
        cache.put(1, 10, now);
        cache.put(2, 20, now);
        cache.put(3, 30, now); // evicts key 1
        assert_eq!(cache.len(), 2);
        assert!(cache.get(1, now).is_none());
        assert_eq!(cache.get(2, now), Some(20));
        assert_eq!(cache.get(3, now), Some(30));
    }
}
