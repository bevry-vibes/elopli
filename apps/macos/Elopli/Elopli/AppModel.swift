import AppKit
import Foundation
import ServiceManagement
import SwiftUI

enum LinkKind: String, Codable, CaseIterable {
	case card
	case power
	case brightness
	case warmth
}

struct LinkTarget: Hashable {
	let device: String
	let kind: LinkKind
}

struct LinkGroup: Identifiable, Codable, Equatable {
	var id = UUID()
	let kind: LinkKind
	var deviceNames: [String]
}

struct LightDevice: Identifiable, Equatable {
	let name: String
	let host: String
	let port: UInt16
	var state: FfiLightState?
	var info: FfiAccessoryInfo?
	var powerOn: FfiPowerOnSettings?
	var error: String?

	var id: String { name }
}

@MainActor
final class AppModel: ObservableObject {
	@Published var devices: [LightDevice] = []
	@Published var configPath = ""
	@Published var isRefreshing = false
	@Published var isScanning = false
	@Published var discovered: [FfiDiscoveredDevice] = []
	@Published var errorMessage: String?
	@Published var launchAtLogin = SMAppService.mainApp.status == .enabled
	@Published var commandDown = false
	@Published var hoveredLink: LinkTarget?
	@Published var selectedLinks: Set<LinkTarget> = []
	@Published private(set) var linkGroups: [LinkGroup] = []
	@Published private(set) var pulsingDevices: Set<String> = []
	@Published private(set) var liveBrightness: [String: UInt8] = [:]
	@Published private(set) var liveWarmth: [String: UInt8] = [:]

	private var config: FfiConfig?
	private var eventMonitor: Any?
	private let linkGroupsKey = "ElopliLinkGroupsV2"
	private static let linkAccent = Color(red: 1.0, green: 0.0, blue: 1.0)
	private static let sliderWriteInterval = Duration.milliseconds(100)
	private struct SliderKey: Hashable {
		let device: String
		let kind: LinkKind
	}
	private var pendingSliderValues: [SliderKey: UInt8] = [:]
	private var sliderWriteTasks: [SliderKey: Task<Void, Never>] = [:]
	private var liveClearTasks: [SliderKey: Task<Void, Never>] = [:]

	init() {
		do {
			let config = try FfiConfig.load(path: nil)
			self.config = config
			self.configPath = (try? config.path()) ?? ""
			reloadDevices()
			loadLinkGroups()
		} catch {
			self.errorMessage = Self.describe(error)
		}

		eventMonitor = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) {
			[weak self] event in
			guard let self else { return event }
			let down = event.modifierFlags.contains(.command)
			if self.commandDown != down {
				self.commandDown = down
				if !down {
					self.commitLinkSelection()
				}
			}
			return event
		}
	}

	deinit {
		if let eventMonitor {
			NSEvent.removeMonitor(eventMonitor)
		}
	}

	nonisolated static func describe(_ error: Error) -> String {
		if let error = error as? FfiError {
			switch error {
			case let .Message(message):
				return message
			}
		}
		return error.localizedDescription
	}

	nonisolated static func client(for device: LightDevice) throws -> FfiClient {
		try FfiClient(
			host: device.host,
			port: device.port,
			timeoutSeconds: 3.0,
			retries: 2,
			transport: "auto"
		)
	}

	func reloadDevices() {
		guard let config else { return }
		do {
			let existing = Dictionary(uniqueKeysWithValues: devices.map { ($0.name, $0) })
			devices = try config.devices().map { device in
				if let current = existing[device.name] {
					return current
				}
				return LightDevice(name: device.name, host: device.host, port: device.port)
			}
			pruneLinkGroups()
		} catch {
			errorMessage = Self.describe(error)
		}
	}

	func refresh() async {
		guard !isRefreshing else { return }
		isRefreshing = true
		defer { isRefreshing = false }
		let current = devices.filter { !pulsingDevices.contains($0.name) }

		await withTaskGroup(
			of: (String, FfiSnapshot?, FfiPowerOnSettings?, String?).self
		) { group in
			for device in current {
				group.addTask {
					do {
						let client = try Self.client(for: device)
						let result = try await Task.detached {
							let snapshot = try client.snapshot()
							let powerOn = try? client.powerOnSettings()
							return (snapshot, powerOn)
						}.value
						return (device.name, result.0, result.1, nil)
					} catch {
						return (device.name, nil, nil, Self.describe(error))
					}
				}
			}
			for await (name, snapshot, powerOn, error) in group {
				guard !pulsingDevices.contains(name) else { continue }
				guard let index = devices.firstIndex(where: { $0.name == name }) else { continue }
				if let snapshot {
					devices[index].state = snapshot.state
					devices[index].info = snapshot.info
					devices[index].error = nil
				} else {
					devices[index].error = error ?? "unreachable"
				}
				if let powerOn {
					devices[index].powerOn = powerOn
				}
			}
		}
	}

	private func update(
		names: [String],
		action: @escaping @Sendable (FfiClient) throws -> FfiLightState
	) async {
		let targets = devices.filter { names.contains($0.name) }
		await updateEach(targets) { _, client in
			try action(client)
		}
	}

	private func updateEach(
		_ targets: [LightDevice],
		action: @escaping @Sendable (LightDevice, FfiClient) throws -> FfiLightState
	) async {
		guard !targets.isEmpty else { return }
		await withTaskGroup(of: (String, FfiLightState?, String?).self) { group in
			for device in targets {
				group.addTask {
					do {
						let client = try Self.client(for: device)
						let state = try await Task.detached {
							try action(device, client)
						}.value
						return (device.name, state, nil)
					} catch {
						return (device.name, nil, Self.describe(error))
					}
				}
			}
			for await (name, state, error) in group {
				guard let index = devices.firstIndex(where: { $0.name == name }) else { continue }
				if let state {
					devices[index].state = state
					devices[index].error = nil
				} else {
					devices[index].error = error ?? "unreachable"
				}
			}
		}
	}

	func setOn(_ device: LightDevice, on: Bool) async {
		let names = linkedDeviceNames(for: device.name, kind: .power)
		let targets = devices.filter { names.contains($0.name) }
		await updateEach(targets) { target, client in
			let brightness: UInt8? = (on && (target.state?.brightness ?? 0) < 2)
				? 2
				: nil
			return try client.setOnWithBrightness(on: on, brightness: brightness)
		}
	}

	func setAll(on: Bool) async {
		await updateEach(devices) { target, client in
			let brightness: UInt8? = (on && (target.state?.brightness ?? 0) < 2)
				? 2
				: nil
			return try client.setOnWithBrightness(on: on, brightness: brightness)
		}
	}

	func toggle(_ device: LightDevice) async {
		let effectivelyOn = device.state?.on == true
			&& (device.state?.brightness ?? 0) >= 2
		await setOn(device, on: !effectivelyOn)
	}

	func setBrightness(_ device: LightDevice, value: UInt8) async {
		let value = max(2, min(100, value))
		let names = linkedDeviceNames(for: device.name, kind: .brightness)
		await update(names: names) { try $0.setBrightness(value: value) }
	}

	func setWarmth(_ device: LightDevice, value: UInt8) async {
		let value = max(0, min(100, value))
		let names = linkedDeviceNames(for: device.name, kind: .warmth)
		await update(names: names) { try $0.setWarmth(value: value) }
	}

	/// Queue a brightness write while a slider is moving. Only the newest
	/// value is kept, and at most one write is in flight per device and kind.
	func queueBrightness(_ device: LightDevice, value: UInt8) {
		enqueueSlider(device: device, kind: .brightness, value: value)
	}

	/// Queue a warmth write while a slider is moving.
	func queueWarmth(_ device: LightDevice, value: UInt8) {
		enqueueSlider(device: device, kind: .warmth, value: value)
	}

	private func enqueueSlider(device: LightDevice, kind: LinkKind, value: UInt8) {
		let key = SliderKey(device: device.name, kind: kind)
		let names = linkedDeviceNames(for: device.name, kind: kind)
		for name in names {
			setLive(kind: kind, name: name, value: value)
		}
		liveClearTasks[key]?.cancel()
		liveClearTasks[key] = nil
		pendingSliderValues[key] = value
		guard sliderWriteTasks[key] == nil else { return }
		sliderWriteTasks[key] = Task { [weak self] in
			guard let self else { return }
			await self.drainSlider(key: key, device: device)
		}
	}

	private func drainSlider(key: SliderKey, device: LightDevice) async {
		while let value = pendingSliderValues.removeValue(forKey: key) {
			let started = ContinuousClock.now
			switch key.kind {
			case .brightness:
				await setBrightness(device, value: value)
			case .warmth:
				await setWarmth(device, value: value)
			case .power, .card:
				break
			}
			let elapsed = ContinuousClock.now - started
			if elapsed < Self.sliderWriteInterval {
				try? await Task.sleep(for: Self.sliderWriteInterval - elapsed)
			}
		}
		sliderWriteTasks[key] = nil
		// A value can arrive while the last write is finishing.
		if let value = pendingSliderValues[key], sliderWriteTasks[key] == nil {
			enqueueSlider(device: device, kind: key.kind, value: value)
			return
		}
		let names = linkedDeviceNames(for: device.name, kind: key.kind)
		scheduleLiveClear(key: key, names: names)
	}

	private func setLive(kind: LinkKind, name: String, value: UInt8?) {
		switch kind {
		case .brightness:
			liveBrightness[name] = value
		case .warmth:
			liveWarmth[name] = value
		case .power, .card:
			break
		}
	}

	private func scheduleLiveClear(key: SliderKey, names: [String]) {
		liveClearTasks[key]?.cancel()
		liveClearTasks[key] = Task { [weak self] in
			try? await Task.sleep(for: .milliseconds(250))
			guard !Task.isCancelled, let self else { return }
			for name in names {
				self.setLive(kind: key.kind, name: name, value: nil)
			}
			self.liveClearTasks[key] = nil
		}
	}

	func setKelvin(_ device: LightDevice, value: UInt16) async {
		let names = linkedDeviceNames(for: device.name, kind: .warmth)
		await update(names: names) { try $0.setKelvin(value: value) }
	}

	func rename(_ device: LightDevice, to name: String) async {
		do {
			let client = try Self.client(for: device)
			_ = try await Task.detached { try client.rename(name: name) }.value
			await refresh()
		} catch {
			setError(device, error)
		}
	}

	func setPowerOn(
		_ device: LightDevice,
		behavior: UInt8,
		brightness: UInt8,
		temperatureMired: UInt16
	) async {
		do {
			let client = try Self.client(for: device)
			let powerOn = try await Task.detached {
				try client.setPowerOn(
					behavior: behavior,
					brightness: brightness,
					temperatureMired: temperatureMired
				)
			}.value
			if let index = devices.firstIndex(where: { $0.name == device.name }) {
				devices[index].powerOn = powerOn
				devices[index].error = nil
			}
		} catch {
			setError(device, error)
		}
	}

	func identify(_ device: LightDevice) async {
		do {
			let client = try Self.client(for: device)
			try await Task.detached { try client.identify() }.value
		} catch {
			setError(device, error)
		}
	}

	func pulse(
		_ device: LightDevice,
		count: Int,
		durationMs: Int,
		value: Int,
		low: Int
	) async -> FfiPulseOutcome? {
		let outcomes = await pulseGroup(
			[device],
			count: count,
			durationMs: durationMs,
			value: value,
			low: low
		)
		return outcomes.first ?? nil
	}

	func pulseGroup(
		_ devices: [LightDevice],
		count: Int,
		durationMs: Int,
		value: Int,
		low: Int
	) async -> [FfiPulseOutcome?] {
		guard !devices.isEmpty else { return [] }
		for device in devices {
			pulsingDevices.insert(device.name)
		}
		defer {
			for device in devices {
				pulsingDevices.remove(device.name)
			}
			Task { await refresh() }
		}
		return await withTaskGroup(of: (Int, FfiPulseOutcome?, String?).self) { group in
			for (index, device) in devices.enumerated() {
				group.addTask {
					do {
						let client = try Self.client(for: device)
						let outcome = try await Task.detached {
							try client.pulse(
								count: UInt32(count),
								durationMs: UInt32(durationMs),
								value: UInt8(value),
								low: UInt8(low)
							)
						}.value
						return (index, outcome, nil)
					} catch {
						return (index, nil, Self.describe(error))
					}
				}
			}
			var results = Array<FfiPulseOutcome?>(
				repeating: nil,
				count: devices.count
			)
			var errors: [Int: String] = [:]
			for await (index, outcome, error) in group {
				results[index] = outcome
				if let error {
					errors[index] = error
				}
			}
			for (index, error) in errors {
				setError(devices[index], message: error)
			}
			return results
		}
	}

	func sweep(
		_ device: LightDevice,
		count: Int,
		durationMs: Int,
		brightness: Int
	) async -> FfiPulseOutcome? {
		let outcomes = await sweepGroup(
			[device],
			count: count,
			durationMs: durationMs,
			brightness: brightness
		)
		return outcomes.first ?? nil
	}

	func sweepGroup(
		_ devices: [LightDevice],
		count: Int,
		durationMs: Int,
		brightness: Int
	) async -> [FfiPulseOutcome?] {
		guard !devices.isEmpty else { return [] }
		for device in devices {
			pulsingDevices.insert(device.name)
		}
		defer {
			for device in devices {
				pulsingDevices.remove(device.name)
			}
			Task { await refresh() }
		}
		return await withTaskGroup(of: (Int, FfiPulseOutcome?, String?).self) { group in
			for (index, device) in devices.enumerated() {
				group.addTask {
					do {
						let client = try Self.client(for: device)
						let outcome = try await Task.detached {
							try client.sweep(
								count: UInt32(count),
								durationMs: UInt32(durationMs),
								brightness: brightness > 0 ? UInt8(brightness) : nil
							)
						}.value
						return (index, outcome, nil)
					} catch {
						return (index, nil, Self.describe(error))
					}
				}
			}
			var results = Array<FfiPulseOutcome?>(
				repeating: nil,
				count: devices.count
			)
			var errors: [Int: String] = [:]
			for await (index, outcome, error) in group {
				results[index] = outcome
				if let error {
					errors[index] = error
				}
			}
			for (index, error) in errors {
				setError(devices[index], message: error)
			}
			return results
		}
	}

	func cancelPulse(_ device: LightDevice) {
		do {
			try Self.client(for: device).cancelPulse()
		} catch {
			setError(device, error)
		}
	}

	func scan() async {
		guard !isScanning else { return }
		isScanning = true
		defer { isScanning = false }
		do {
			discovered = try await Task.detached {
				try discover(timeoutSeconds: 3.0)
			}.value
		} catch {
			errorMessage = Self.describe(error)
		}
	}

	func addDiscovered(_ device: FfiDiscoveredDevice) {
		guard let config else { return }
		let host = device.address ?? device.host
		guard !devices.contains(where: {
			$0.name == device.name || $0.host == host
		}) else {
			return
		}
		do {
			_ = try config.add(name: device.name, host: host, port: device.port)
			try config.save()
			reloadDevices()
			discovered.removeAll { $0.name == device.name }
		} catch {
			errorMessage = Self.describe(error)
		}
	}

	func setLaunchAtLogin(_ enabled: Bool) {
		do {
			if enabled {
				try SMAppService.mainApp.register()
			} else {
				try SMAppService.mainApp.unregister()
			}
			launchAtLogin = enabled
		} catch {
			errorMessage = Self.describe(error)
		}
	}

	func moveDevice(_ name: String, onto target: String) {
		guard let from = devices.firstIndex(where: { $0.name == name }),
			let to = devices.firstIndex(where: { $0.name == target }),
			from != to
		else {
			return
		}
		devices.swapAt(from, to)
		persistOrder()
	}

	private func persistOrder() {
		do {
			try config?.reorder(names: devices.map(\.name))
			try config?.save()
		} catch {
			errorMessage = Self.describe(error)
		}
	}

	func setHover(_ target: LinkTarget?) {
		hoveredLink = target
	}

	func toggleLinkSelection(_ kind: LinkKind, device: LightDevice) {
		guard commandDown else { return }
		let target = LinkTarget(device: device.name, kind: kind)
		if kind != .card && isInCardGroup(device.name) {
			unlinkCardGroup(containing: device.name)
			selectedLinks.insert(target)
			return
		}
		if selectedLinks.contains(target) {
			selectedLinks.remove(target)
		} else {
			selectedLinks.insert(target)
		}
	}

	func isInCardGroup(_ deviceName: String) -> Bool {
		linkGroups.contains {
			$0.kind == .card && $0.deviceNames.contains(deviceName)
		}
	}

	func isGrouped(_ device: LightDevice) -> Bool {
		linkGroups.contains {
			$0.deviceNames.count >= 2 && $0.deviceNames.contains(device.name)
		}
	}

	func groupedDevices(for device: LightDevice) -> [LightDevice] {
		var names: Set<String> = [device.name]
		for group in linkGroups where group.deviceNames.contains(device.name) {
			names.formUnion(group.deviceNames)
		}
		return devices.filter { names.contains($0.name) }
	}

	private func unlinkCardGroup(containing deviceName: String) {
		let count = linkGroups.count
		linkGroups.removeAll {
			$0.kind == .card && $0.deviceNames.contains(deviceName)
		}
		if linkGroups.count != count {
			saveLinkGroups()
		}
	}

	func resetLinkInteraction() {
		commandDown = false
		hoveredLink = nil
		selectedLinks.removeAll()
	}

	func linkColor(for target: LinkTarget) -> Color? {
		if selectedLinks.contains(target) {
			return Color.yellow
		}
		if commandDown && hoveredLink == target {
			return Color.yellow.opacity(0.85)
		}
		if isLinked(target) {
			return Self.linkAccent.opacity(0.55)
		}
		return nil
	}

	func isLinked(_ target: LinkTarget) -> Bool {
		if target.kind == .card {
			return linkGroups.contains {
				$0.kind == .card && $0.deviceNames.contains(target.device)
			}
		}
		if isInCardGroup(target.device) {
			return false
		}
		return linkGroups.contains {
			$0.kind == target.kind && $0.deviceNames.contains(target.device)
		}
	}

	private func linkedDeviceNames(for device: String, kind: LinkKind) -> [String] {
		var names: Set<String> = [device]
		if let card = linkGroups.first(where: {
			$0.kind == .card && $0.deviceNames.contains(device)
		}) {
			names.formUnion(card.deviceNames)
		}
		if let group = linkGroups.first(where: {
			$0.kind == kind && $0.deviceNames.contains(device)
		}) {
			names.formUnion(group.deviceNames)
		}
		let existing = Set(devices.map(\.name))
		return names.filter { existing.contains($0) }
	}

	private func isFieldLinked(_ deviceName: String, kind: LinkKind) -> Bool {
		if isInCardGroup(deviceName) {
			return true
		}
		return linkGroups.contains {
			$0.kind == kind && $0.deviceNames.contains(deviceName)
		}
	}

	private func joinableNames(for deviceName: String, kind: LinkKind) -> [String]? {
		if let group = linkGroups.first(where: {
			$0.kind == kind && !$0.deviceNames.contains(deviceName)
		}) {
			return group.deviceNames
		}
		let card = linkGroups
			.filter { $0.kind == .card && !$0.deviceNames.contains(deviceName) }
			.max { $0.deviceNames.count < $1.deviceNames.count }
		return card?.deviceNames
	}

	private func commitLinkSelection() {
		guard !selectedLinks.isEmpty else { return }
		let grouped = Dictionary(grouping: selectedLinks, by: \.kind)
		selectedLinks.removeAll()
		for kind in [LinkKind.power, .brightness, .warmth, .card] {
			guard let targets = grouped[kind] else { continue }
			let names = Array(Set(targets.map(\.device)))
			if names.count >= 2 {
				if let existing = linkGroups.first(where: {
					$0.kind == kind && Set($0.deviceNames) == Set(names)
				}) {
					linkGroups.removeAll { $0.id == existing.id }
				} else {
					let sorted = names.sorted()
					link(kind: kind, names: sorted)
					syncAfterLink(kind: kind, names: sorted)
				}
			} else if let name = names.first {
				if kind != .card,
					!isFieldLinked(name, kind: kind),
					let anchor = joinableNames(for: name, kind: kind)
				{
					let joined = Array(Set(anchor + [name])).sorted()
					link(kind: kind, names: joined)
					syncAfterLink(kind: kind, names: joined)
				} else {
					unlink(name: name, kind: kind)
				}
			}
		}
		normalizeLinkGroups()
		saveLinkGroups()
	}

	/// Apply the first card's visible values to the other members when a
	/// group is created. The first member follows the UI order, not the
	/// alphabetical group order.
	private func syncAfterLink(kind: LinkKind, names: [String]) {
		guard let anchor = names.first else { return }
		if let card = linkGroups.first(where: {
			$0.kind == .card && $0.deviceNames.contains(anchor)
		}) {
			syncGroup(kind: .card, names: card.deviceNames)
		} else {
			syncGroup(kind: kind, names: names)
		}
	}

	private func syncGroup(kind: LinkKind, names: [String]) {
		let members = devices.filter { names.contains($0.name) }
		guard members.count >= 2, let source = members.first?.state else { return }
		let on = source.on
		let brightness = max(2, source.brightness)
		let warmth = source.warmth
		let temperature = source.temperatureMired
		let targets = Array(members.dropFirst())
		Task {
			await updateEach(targets) { _, client in
				switch kind {
				case .power:
					return try client.setOnWithBrightness(on: on, brightness: nil)
				case .brightness:
					return try client.setBrightness(value: brightness)
				case .warmth:
					return try client.setWarmth(value: warmth)
				case .card:
					_ = try client.setBrightness(value: brightness)
					_ = try client.setWarmth(value: warmth)
					return try client.setOnWithBrightness(
						on: on,
						brightness: on ? brightness : nil
					)
				}
			}
		}
	}

	private func link(kind: LinkKind, names: [String]) {
		for index in linkGroups.indices {
			if linkGroups[index].kind == .card {
				if kind == .card {
					linkGroups[index].deviceNames.removeAll { names.contains($0) }
				}
			} else if kind == .card || linkGroups[index].kind == kind {
				linkGroups[index].deviceNames.removeAll { names.contains($0) }
			}
		}
		linkGroups.removeAll { $0.deviceNames.count < 2 }
		linkGroups.append(LinkGroup(kind: kind, deviceNames: names.sorted()))
		if kind != .card {
			promoteToCardIfComplete(names)
		}
	}

	private func promoteToCardIfComplete(_ names: [String]) {
		let target = Set(names)
		let kinds: [LinkKind] = [.power, .brightness, .warmth]
		var groups: [LinkGroup] = []
		for kind in kinds {
			guard let group = linkGroups.first(where: {
				$0.kind == kind && Set($0.deviceNames) == target
			}) else {
				return
			}
			groups.append(group)
		}
		let ids = Set(groups.map(\.id))
		linkGroups.removeAll { ids.contains($0.id) }
		let hasCard = linkGroups.contains {
			$0.kind == .card && Set($0.deviceNames) == target
		}
		if !hasCard {
			linkGroups.append(LinkGroup(kind: .card, deviceNames: names.sorted()))
		}
	}

	private func unlink(name: String, kind: LinkKind) {
		for index in linkGroups.indices where linkGroups[index].kind == kind {
			linkGroups[index].deviceNames.removeAll { $0 == name }
		}
		linkGroups.removeAll { $0.deviceNames.count < 2 }
	}

	private func loadLinkGroups() {
		let defaults = UserDefaults.standard
		var data = defaults.data(forKey: linkGroupsKey)
		if data == nil,
			let legacy = UserDefaults(suiteName: "com.bevry.optoli"),
			let legacyData = legacy.data(forKey: "OptoliLinkGroupsV2")
		{
			defaults.set(legacyData, forKey: linkGroupsKey)
			data = legacyData
		}
		guard let data,
			let groups = try? JSONDecoder().decode([LinkGroup].self, from: data)
		else {
			return
		}
		linkGroups = groups
		pruneLinkGroups()
		normalizeLinkGroups()
	}

	private func saveLinkGroups() {
		guard let data = try? JSONEncoder().encode(linkGroups) else { return }
		UserDefaults.standard.set(data, forKey: linkGroupsKey)
	}

	private func pruneLinkGroups() {
		let names = Set(devices.map(\.name))
		linkGroups = linkGroups
			.map { group in
				var group = group
				group.deviceNames = group.deviceNames.filter { names.contains($0) }
				return group
			}
			.filter { $0.deviceNames.count >= 2 }
	}

	private func normalizeLinkGroups() {
		let cardSets = linkGroups
			.filter { $0.kind == .card }
			.map { Set($0.deviceNames) }
		linkGroups.removeAll { group in
			group.kind != .card
				&& cardSets.contains { $0.isSuperset(of: Set(group.deviceNames)) }
		}

		let candidates = Set(
			linkGroups
				.filter { $0.kind != .card }
				.map { Set($0.deviceNames) }
		)
		for candidate in candidates where candidate.count >= 2 {
			let kinds: [LinkKind] = [.power, .brightness, .warmth]
			let groups = kinds.compactMap { kind in
				linkGroups.first {
					$0.kind == kind && Set($0.deviceNames) == candidate
				}
			}
			guard groups.count == 3 else { continue }
			let ids = Set(groups.map(\.id))
			linkGroups.removeAll { ids.contains($0.id) }
			let hasCard = linkGroups.contains {
				$0.kind == .card && Set($0.deviceNames) == candidate
			}
			if !hasCard {
				linkGroups.append(
					LinkGroup(kind: .card, deviceNames: candidate.sorted())
				)
			}
		}
	}

	private func setError(_ device: LightDevice, _ error: Error) {
		if let index = devices.firstIndex(where: { $0.name == device.name }) {
			devices[index].error = Self.describe(error)
		}
	}

	private func setError(_ device: LightDevice, message: String) {
		if let index = devices.firstIndex(where: { $0.name == device.name }) {
			devices[index].error = message
		}
	}
}
