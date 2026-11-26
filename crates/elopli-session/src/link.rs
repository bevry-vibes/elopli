use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use elopli_core::{ElopliError, Result};
use serde::{Deserialize, Serialize};

/// The part of a card that a link applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
	Card,
	Power,
	Brightness,
	Warmth,
}

impl LinkKind {
	pub fn label(self) -> &'static str {
		match self {
			Self::Card => "card",
			Self::Power => "power",
			Self::Brightness => "brightness",
			Self::Warmth => "warmth",
		}
	}
}

const COMMIT_ORDER: [LinkKind; 4] = [
	LinkKind::Power,
	LinkKind::Brightness,
	LinkKind::Warmth,
	LinkKind::Card,
];

/// One card field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LinkTarget {
	pub device: String,
	pub kind: LinkKind,
}

impl LinkTarget {
	pub fn new(device: impl Into<String>, kind: LinkKind) -> Self {
		Self {
			device: device.into(),
			kind,
		}
	}
}

/// A committed group. Every group has at least two members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkGroup {
	pub kind: LinkKind,
	pub device_names: Vec<String>,
}

impl LinkGroup {
	pub fn contains(&self, name: &str) -> bool {
		self.device_names.iter().any(|device| device == name)
	}
}

/// The persistent link graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkGraph {
	#[serde(default)]
	pub groups: Vec<LinkGroup>,
	#[serde(skip)]
	pub pending: BTreeSet<LinkTarget>,
}

impl LinkGraph {
	pub fn load(path: &Path) -> Result<Self> {
		if !path.exists() {
			return Ok(Self::default());
		}
		let raw = fs::read_to_string(path).map_err(|error| {
			ElopliError::Config(format!("cannot read {}: {error}", path.display()))
		})?;
		let contents = raw.trim_start_matches('\u{feff}');
		let mut graph: Self = serde_json::from_str(contents).map_err(|error| {
			ElopliError::Config(format!("cannot parse {}: {error}", path.display()))
		})?;
		for group in &mut graph.groups {
			group.device_names = normalized(&group.device_names);
		}
		graph.normalize();
		Ok(graph)
	}

	pub fn save(&self, path: &Path) -> Result<()> {
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).map_err(|error| {
				ElopliError::Config(format!("cannot create {}: {error}", parent.display()))
			})?;
		}
		let contents = serde_json::to_string_pretty(self)
			.map_err(|error| ElopliError::Config(error.to_string()))?;
		let contents = format!("{contents}\n");
		let temporary = temporary_path(path);
		let result = (|| -> Result<()> {
			fs::write(&temporary, contents.as_bytes())?;
			replace_file(&temporary, path)?;
			Ok(())
		})();
		if result.is_err() {
			let _ = fs::remove_file(&temporary);
		}
		result
	}

	pub fn save_if_changed(&self, path: &Path, changed: bool) -> Result<()> {
		if changed {
			self.save(path)?;
		}
		Ok(())
	}

	pub fn is_in_card_group(&self, device: &str) -> bool {
		self.groups
			.iter()
			.any(|group| group.kind == LinkKind::Card && group.contains(device))
	}

	pub fn is_grouped(&self, device: &str) -> bool {
		self.groups.iter().any(|group| {
			group.kind == LinkKind::Card && group.device_names.len() >= 2 && group.contains(device)
		})
	}

	pub fn card_group(&self, device: &str) -> Option<&LinkGroup> {
		self.groups
			.iter()
			.find(|group| group.kind == LinkKind::Card && group.contains(device))
	}

	pub fn grouped_device_names(&self, device: &str) -> Vec<String> {
		self.card_group(device)
			.map(|group| group.device_names.clone())
			.unwrap_or_else(|| vec![device.to_string()])
	}

	pub fn is_linked(&self, target: &LinkTarget) -> bool {
		if target.kind == LinkKind::Card {
			return self
				.groups
				.iter()
				.any(|group| group.kind == LinkKind::Card && group.contains(&target.device));
		}
		if self.is_in_card_group(&target.device) {
			return false;
		}
		self.groups
			.iter()
			.any(|group| group.kind == target.kind && group.contains(&target.device))
	}

	pub fn linked_device_names(&self, device: &str, kind: LinkKind) -> Vec<String> {
		let mut names = BTreeSet::from([device.to_string()]);
		if let Some(card) = self.card_group(device) {
			names.extend(card.device_names.iter().cloned());
		}
		if kind != LinkKind::Card
			&& let Some(group) = self
				.groups
				.iter()
				.find(|group| group.kind == kind && group.contains(device))
		{
			names.extend(group.device_names.iter().cloned());
		}
		names.into_iter().collect()
	}

	pub fn is_field_linked(&self, device: &str, kind: LinkKind) -> bool {
		if self.is_in_card_group(device) {
			return true;
		}
		self.groups
			.iter()
			.any(|group| group.kind == kind && group.contains(device))
	}

	fn joinable_names(&self, device: &str, kind: LinkKind) -> Option<Vec<String>> {
		if let Some(group) = self
			.groups
			.iter()
			.find(|group| group.kind == kind && !group.contains(device))
		{
			return Some(group.device_names.clone());
		}
		self.groups
			.iter()
			.filter(|group| group.kind == LinkKind::Card && !group.contains(device))
			.max_by_key(|group| group.device_names.len())
			.map(|group| group.device_names.clone())
	}

	/// Toggle a pending link selection. A field click on a card group breaks
	/// the card group and starts a field selection.
	pub fn toggle_selection(&mut self, target: LinkTarget) -> bool {
		if target.kind != LinkKind::Card && self.is_in_card_group(&target.device) {
			self.unlink_card_group(&target.device);
			self.pending.insert(target);
			return true;
		}
		if !self.pending.remove(&target) {
			self.pending.insert(target);
		}
		false
	}

	/// Commit the pending selection. Returns the groups that should be synced
	/// from their first member, in UI order.
	pub fn commit(&mut self) -> Vec<LinkGroup> {
		let pending = std::mem::take(&mut self.pending);
		let mut sync = Vec::new();
		for kind in COMMIT_ORDER {
			let names = pending
				.iter()
				.filter(|target| target.kind == kind)
				.map(|target| target.device.clone())
				.collect::<BTreeSet<_>>()
				.into_iter()
				.collect::<Vec<_>>();
			if names.is_empty() {
				continue;
			}
			if names.len() >= 2 {
				if let Some(index) = self
					.groups
					.iter()
					.position(|group| group.kind == kind && same_set(&group.device_names, &names))
				{
					self.groups.remove(index);
				} else {
					self.link(kind, names.clone());
					sync.push(LinkGroup {
						kind,
						device_names: names,
					});
				}
			} else if let Some(name) = names.first() {
				if kind != LinkKind::Card
					&& !self.is_field_linked(name, kind)
					&& let Some(anchor) = self.joinable_names(name, kind)
				{
					let joined = normalized(&[anchor, vec![name.clone()]].concat());
					self.link(kind, joined.clone());
					sync.push(LinkGroup {
						kind,
						device_names: joined,
					});
					continue;
				}
				self.unlink(name, kind);
			}
		}
		self.normalize();
		sync
	}

	/// Rename a device everywhere in the graph: groups and the pending
	/// selection. Returns true when anything changed.
	pub fn rename(&mut self, from: &str, to: &str) -> bool {
		if from == to {
			return false;
		}
		let before = self.clone();
		for group in &mut self.groups {
			for name in &mut group.device_names {
				if name == from {
					*name = to.to_string();
				}
			}
		}
		self.pending = self
			.pending
			.iter()
			.cloned()
			.map(|mut target| {
				if target.device == from {
					target.device = to.to_string();
				}
				target
			})
			.collect();
		self.normalize();
		*self != before
	}

	pub fn clear_pending(&mut self) {
		self.pending.clear();
	}

	pub fn unlink_card_group(&mut self, device: &str) {
		self.groups
			.retain(|group| group.kind != LinkKind::Card || !group.contains(device));
	}

	fn link(&mut self, kind: LinkKind, names: Vec<String>) {
		let names = normalized(&names);
		if names.len() < 2 {
			return;
		}
		for group in &mut self.groups {
			if group.kind == LinkKind::Card {
				if kind == LinkKind::Card {
					group.device_names.retain(|name| !names.contains(name));
				}
			} else if kind == LinkKind::Card || group.kind == kind {
				group.device_names.retain(|name| !names.contains(name));
			}
		}
		self.groups.retain(|group| group.device_names.len() >= 2);
		self.groups.push(LinkGroup {
			kind,
			device_names: names.clone(),
		});
		if kind != LinkKind::Card {
			self.promote_to_card_if_complete(&names);
		}
		self.normalize();
	}

	fn promote_to_card_if_complete(&mut self, names: &[String]) {
		let kinds = [LinkKind::Power, LinkKind::Brightness, LinkKind::Warmth];
		let Some(groups) = kinds
			.iter()
			.map(|kind| {
				self.groups
					.iter()
					.position(|group| group.kind == *kind && same_set(&group.device_names, names))
			})
			.collect::<Option<Vec<_>>>()
		else {
			return;
		};
		for index in groups.into_iter().rev() {
			self.groups.remove(index);
		}
		self.groups.retain(|group| {
			group.kind != LinkKind::Card || !names.iter().any(|name| group.contains(name))
		});
		self.groups.push(LinkGroup {
			kind: LinkKind::Card,
			device_names: names.to_vec(),
		});
	}

	fn unlink(&mut self, name: &str, kind: LinkKind) {
		for group in &mut self.groups {
			if group.kind == kind {
				group.device_names.retain(|device| device != name);
			}
		}
		self.groups.retain(|group| group.device_names.len() >= 2);
	}

	/// Drop devices that are no longer registered. Returns true when the graph
	/// changed.
	pub fn prune(&mut self, valid_names: &HashSet<String>) -> bool {
		let before = self.clone();
		for group in &mut self.groups {
			group.device_names.retain(|name| valid_names.contains(name));
		}
		self.groups.retain(|group| group.device_names.len() >= 2);
		self.pending
			.retain(|target| valid_names.contains(&target.device));
		self.normalize();
		*self != before
	}

	/// Remove redundant groups and promote complete field triples to cards.
	pub fn normalize(&mut self) {
		self.groups.retain(|group| group.device_names.len() >= 2);
		for group in &mut self.groups {
			group.device_names = normalized(&group.device_names);
		}

		let mut seen = HashSet::new();
		self.groups
			.retain(|group| seen.insert((group.kind, group.device_names.clone())));

		let cards = self
			.groups
			.iter()
			.filter(|group| group.kind == LinkKind::Card)
			.cloned()
			.collect::<Vec<_>>();
		self.groups.retain(|group| {
			if group.kind != LinkKind::Card {
				return true;
			}
			!cards.iter().any(|other| {
				other.device_names != group.device_names
					&& is_superset(&other.device_names, &group.device_names)
			})
		});

		let card_sets = self
			.groups
			.iter()
			.filter(|group| group.kind == LinkKind::Card)
			.map(|group| group.device_names.clone())
			.collect::<Vec<_>>();
		self.groups.retain(|group| {
			group.kind == LinkKind::Card
				|| !card_sets
					.iter()
					.any(|card| is_superset(card, &group.device_names))
		});

		let candidates = self
			.groups
			.iter()
			.filter(|group| group.kind != LinkKind::Card)
			.map(|group| group.device_names.clone())
			.collect::<BTreeSet<_>>();
		for candidate in candidates {
			if candidate.len() < 2 {
				continue;
			}
			let kinds = [LinkKind::Power, LinkKind::Brightness, LinkKind::Warmth];
			if kinds.iter().all(|kind| {
				self.groups
					.iter()
					.any(|group| group.kind == *kind && same_set(&group.device_names, &candidate))
			}) {
				self.groups
					.retain(|group| !same_set(&group.device_names, &candidate));
				self.groups.push(LinkGroup {
					kind: LinkKind::Card,
					device_names: candidate,
				});
			}
		}

		self.groups.sort_by(|left, right| {
			left.kind
				.cmp(&right.kind)
				.then_with(|| left.device_names.cmp(&right.device_names))
		});
	}
}

fn normalized(names: &[String]) -> Vec<String> {
	names
		.iter()
		.cloned()
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

fn same_set(left: &[String], right: &[String]) -> bool {
	left.len() == right.len() && left.iter().all(|name| right.contains(name))
}

fn is_superset(left: &[String], right: &[String]) -> bool {
	right.iter().all(|name| left.contains(name))
}

fn temporary_path(path: &Path) -> PathBuf {
	let parent = path.parent().unwrap_or_else(|| Path::new("."));
	let name = path
		.file_name()
		.and_then(|value| value.to_str())
		.unwrap_or("links.json");
	parent.join(format!(
		".{name}.tmp-{}-{}.json",
		std::process::id(),
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|duration| duration.as_nanos())
			.unwrap_or_default()
	))
}

fn replace_file(from: &Path, to: &Path) -> Result<()> {
	let mut delay = Duration::from_millis(10);
	let mut last_error = None;
	for attempt in 0..5 {
		match fs::rename(from, to) {
			Ok(()) => return Ok(()),
			Err(error) => {
				last_error = Some(error);
				if attempt < 4 {
					std::thread::sleep(delay);
					delay *= 2;
				}
			}
		}
	}
	Err(last_error
		.map(ElopliError::Io)
		.unwrap_or_else(|| ElopliError::Config(format!("cannot replace {}", to.display()))))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn names(values: &[&str]) -> Vec<String> {
		values.iter().map(|value| value.to_string()).collect()
	}

	fn graph(groups: &[(LinkKind, &[&str])]) -> LinkGraph {
		LinkGraph {
			groups: groups
				.iter()
				.map(|(kind, values)| LinkGroup {
					kind: *kind,
					device_names: names(values),
				})
				.collect(),
			pending: BTreeSet::new(),
		}
	}

	#[test]
	fn field_links_are_scoped_to_the_field() {
		let graph = graph(&[(LinkKind::Brightness, &["Left", "Right"])]);
		assert_eq!(
			graph.linked_device_names("Left", LinkKind::Brightness),
			names(&["Left", "Right"])
		);
		assert_eq!(
			graph.linked_device_names("Left", LinkKind::Warmth),
			names(&["Left"])
		);
		assert!(graph.is_linked(&LinkTarget::new("Left", LinkKind::Brightness)));
		assert!(!graph.is_linked(&LinkTarget::new("Left", LinkKind::Warmth)));
	}

	#[test]
	fn three_matching_fields_promote_to_a_card() {
		let mut graph = graph(&[
			(LinkKind::Power, &["Left", "Right"]),
			(LinkKind::Brightness, &["Left", "Right"]),
			(LinkKind::Warmth, &["Left", "Right"]),
		]);
		graph.normalize();
		assert_eq!(graph.groups.len(), 1);
		assert_eq!(graph.groups[0].kind, LinkKind::Card);
		assert!(graph.is_grouped("Left"));
	}

	#[test]
	fn a_card_group_covers_its_field_links() {
		let mut graph = graph(&[
			(LinkKind::Card, &["Left", "Right"]),
			(LinkKind::Brightness, &["Left", "Right"]),
		]);
		graph.normalize();
		assert_eq!(graph.groups.len(), 1);
		assert!(!graph.is_linked(&LinkTarget::new("Left", LinkKind::Brightness)));
		assert_eq!(
			graph.linked_device_names("Left", LinkKind::Power),
			names(&["Left", "Right"])
		);
	}

	#[test]
	fn pending_selection_commits_and_ungroups() {
		let mut graph = LinkGraph::default();
		graph.toggle_selection(LinkTarget::new("Left", LinkKind::Brightness));
		graph.toggle_selection(LinkTarget::new("Right", LinkKind::Brightness));
		let sync = graph.commit();
		assert_eq!(sync.len(), 1);
		assert_eq!(graph.groups.len(), 1);

		graph.toggle_selection(LinkTarget::new("Left", LinkKind::Brightness));
		graph.toggle_selection(LinkTarget::new("Right", LinkKind::Brightness));
		assert!(graph.commit().is_empty());
		assert!(graph.groups.is_empty());
	}

	#[test]
	fn a_field_click_breaks_a_card_group() {
		let mut graph = graph(&[(LinkKind::Card, &["Left", "Right"])]);
		assert!(graph.toggle_selection(LinkTarget::new("Left", LinkKind::Warmth)));
		assert!(!graph.is_in_card_group("Left"));
		assert!(
			graph
				.pending
				.contains(&LinkTarget::new("Left", LinkKind::Warmth))
		);
	}

	#[test]
	fn a_single_field_joins_the_matching_group() {
		let mut graph = graph(&[(LinkKind::Brightness, &["Left", "Right"])]);
		graph.toggle_selection(LinkTarget::new("Back", LinkKind::Brightness));
		let sync = graph.commit();
		assert_eq!(sync.len(), 1);
		assert_eq!(
			graph.linked_device_names("Back", LinkKind::Brightness),
			names(&["Back", "Left", "Right"])
		);
	}

	#[test]
	fn rename_migrates_groups_and_pending_selections() {
		let mut graph = graph(&[(LinkKind::Card, &["Left", "Right"])]);
		graph
			.pending
			.insert(LinkTarget::new("Left", LinkKind::Warmth));
		assert!(graph.rename("Left", "Desk"));
		assert_eq!(graph.groups[0].device_names, names(&["Desk", "Right"]));
		assert!(
			graph
				.pending
				.contains(&LinkTarget::new("Desk", LinkKind::Warmth))
		);
		assert!(!graph.rename("Desk", "Desk"));
	}

	#[test]
	fn prune_drops_missing_devices() {
		let mut graph = graph(&[(LinkKind::Card, &["Left", "Right", "Back"])]);
		let valid = HashSet::from(["Left".to_string(), "Right".to_string()]);
		assert!(graph.prune(&valid));
		assert_eq!(graph.groups.len(), 1);
		assert_eq!(graph.groups[0].device_names, names(&["Left", "Right"]));
	}

	#[test]
	fn persistence_round_trips() {
		let directory = std::env::temp_dir().join(format!(
			"elopli-links-{}-{}",
			std::process::id(),
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map(|duration| duration.as_nanos())
				.unwrap_or_default()
		));
		fs::create_dir_all(&directory).unwrap();
		let path = directory.join("links.json");
		let mut graph = graph(&[(LinkKind::Card, &["Left", "Right"])]);
		graph.toggle_selection(LinkTarget::new("Back", LinkKind::Power));
		graph.save(&path).unwrap();
		let loaded = LinkGraph::load(&path).unwrap();
		assert_eq!(loaded.groups, graph.groups);
		assert!(loaded.pending.is_empty());
	}
}
