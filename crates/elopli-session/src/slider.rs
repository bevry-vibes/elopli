use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::link::LinkKind;

/// At most one slider write per linked group per interval.
pub const SLIDER_WRITE_INTERVAL: Duration = Duration::from_millis(100);

/// The identity of one linked slider group.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SliderKey {
	pub names: Vec<String>,
	pub kind: LinkKind,
}

impl SliderKey {
	pub fn new(mut names: Vec<String>, kind: LinkKind) -> Self {
		names.sort();
		names.dedup();
		Self { names, kind }
	}
}

/// Coalesces slider drags. A UI queues the newest value for a linked group;
/// the worker writes it at most every [`SLIDER_WRITE_INTERVAL`].
#[derive(Debug, Default)]
pub struct SliderQueue {
	pending: HashMap<SliderKey, u8>,
	last_write: HashMap<SliderKey, Instant>,
}

impl SliderQueue {
	pub fn queue(&mut self, key: SliderKey, value: u8) {
		self.pending.insert(key, value);
	}

	pub fn is_pending(&self, key: &SliderKey) -> bool {
		self.pending.contains_key(key)
	}

	pub fn has_pending_for(&self, name: &str) -> bool {
		self.pending
			.keys()
			.any(|key| key.names.iter().any(|n| n == name))
	}

	pub fn pending(&self) -> usize {
		self.pending.len()
	}

	/// The next value that may be written now. A group that has never been
	/// written is due immediately; otherwise it waits for the interval.
	pub fn due(&self, now: Instant) -> Option<(SliderKey, u8)> {
		self.pending
			.iter()
			.filter(|(key, _)| {
				self.last_write.get(*key).is_none_or(|last| {
					now.saturating_duration_since(*last) >= SLIDER_WRITE_INTERVAL
				})
			})
			.min_by_key(|(key, _)| self.last_write.get(*key).copied())
			.map(|(key, value)| (key.clone(), *value))
	}

	/// Remove a pending value. Returns true when it was still pending.
	pub fn take(&mut self, key: &SliderKey) -> Option<u8> {
		self.pending.remove(key)
	}

	/// Record a completed write. The next value waits for the interval.
	pub fn complete(&mut self, key: &SliderKey, now: Instant) {
		self.last_write.insert(key.clone(), now);
	}

	pub fn retain_names(&mut self, valid: &HashSet<String>) {
		self.pending
			.retain(|key, _| key.names.iter().all(|name| valid.contains(name)));
		self.last_write
			.retain(|key, _| key.names.iter().all(|name| valid.contains(name)));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn key(kind: LinkKind, values: &[&str]) -> SliderKey {
		SliderKey::new(values.iter().map(|value| value.to_string()).collect(), kind)
	}

	#[test]
	fn keeps_only_the_newest_value() {
		let mut queue = SliderQueue::default();
		let key = key(LinkKind::Brightness, &["Left", "Right"]);
		queue.queue(key.clone(), 20);
		queue.queue(key.clone(), 80);
		assert_eq!(queue.due(Instant::now()), Some((key.clone(), 80)));
		assert_eq!(queue.take(&key), Some(80));
		assert!(queue.due(Instant::now()).is_none() || queue.take(&key).is_none());
	}

	#[test]
	fn waits_for_the_interval_after_a_write() {
		let mut queue = SliderQueue::default();
		let key = key(LinkKind::Warmth, &["Left"]);
		let now = Instant::now();
		queue.queue(key.clone(), 40);
		assert_eq!(queue.due(now), Some((key.clone(), 40)));
		queue.take(&key);
		queue.complete(&key, now);
		queue.queue(key.clone(), 60);
		assert_eq!(
			queue.due(now + SLIDER_WRITE_INTERVAL / 2),
			None,
			"a write inside the interval is not due"
		);
		assert_eq!(queue.due(now + SLIDER_WRITE_INTERVAL), Some((key, 60)));
	}

	#[test]
	fn sorts_and_deduplicates_names() {
		let key = key(LinkKind::Power, &["Right", "Left", "Right"]);
		assert_eq!(key.names, vec!["Left".to_string(), "Right".to_string()]);
	}
}
