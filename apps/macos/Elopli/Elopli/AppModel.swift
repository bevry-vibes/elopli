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

	private var config: FfiConfig?
	private var eventMonitor: Any?
	private let linkGroupsKey = "ElopliLinkGroupsV2"
	private static let linkAccent = Color(red: 1.0, green: 0.0, blue: 1.0)

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
		ease: FfiPulseEase,
		target: FfiPulseTarget,
		value: Int
	) async -> FfiPulseOutcome? {
		pulsingDevices.insert(device.name)
		defer {
			pulsingDevices.remove(device.name)
			Task { await refresh() }
		}
		do {
			let client = try Self.client(for: device)
			return try await Task.detached {
				try client.pulse(
					count: UInt32(count),
					durationMs: UInt32(durationMs),
					ease: ease,
					target: target,
					value: UInt8(value)
				)
			}.value
		} catch {
			setError(device, error)
			return nil
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
		do {
			let host = device.address ?? device.host
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
			selectedLinks = [target]
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
			return Self.linkAccent
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
		let names: [String]
		if let card = linkGroups.first(where: {
			$0.kind == .card && $0.deviceNames.contains(device)
		}) {
			names = card.deviceNames
		} else if let group = linkGroups.first(where: {
			$0.kind == kind && $0.deviceNames.contains(device)
		}) {
			names = group.deviceNames
		} else {
			names = [device]
		}
		let existing = Set(devices.map(\.name))
		return names.filter { existing.contains($0) }
	}

	private func commitLinkSelection() {
		guard !selectedLinks.isEmpty else { return }
		let grouped = Dictionary(grouping: selectedLinks, by: \.kind)
		selectedLinks.removeAll()
		for kind in [LinkKind.power, .brightness, .warmth, .card] {
			guard let targets = grouped[kind] else { continue }
			let names = Array(Set(targets.map(\.device)))
			if names.count >= 2 {
				link(kind: kind, names: names.sorted())
			} else if let name = names.first {
				unlink(name: name, kind: kind)
			}
		}
		saveLinkGroups()
	}

	private func link(kind: LinkKind, names: [String]) {
		for index in linkGroups.indices {
			if linkGroups[index].kind == .card {
				linkGroups[index].deviceNames.removeAll { names.contains($0) }
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
		linkGroups.append(LinkGroup(kind: .card, deviceNames: names.sorted()))
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
		let cardDevices = Set(
			linkGroups
				.filter { $0.kind == .card }
				.flatMap(\.deviceNames)
		)
		for index in linkGroups.indices where linkGroups[index].kind != .card {
			linkGroups[index].deviceNames.removeAll { cardDevices.contains($0) }
		}
		linkGroups.removeAll { $0.deviceNames.count < 2 }

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
			linkGroups.append(
				LinkGroup(kind: .card, deviceNames: candidate.sorted())
			)
		}
	}

	private func setError(_ device: LightDevice, _ error: Error) {
		if let index = devices.firstIndex(where: { $0.name == device.name }) {
			devices[index].error = Self.describe(error)
		}
	}
}
