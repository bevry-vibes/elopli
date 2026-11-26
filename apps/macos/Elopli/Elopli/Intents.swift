import AppIntents
import Foundation

enum IntentError: Error, LocalizedError {
	case message(String)

	var errorDescription: String? {
		switch self {
		case let .message(message): return message
		}
	}
}

struct ElopliLightEntity: AppEntity, Hashable {
	static let allID = "all"

	static var typeDisplayRepresentation = TypeDisplayRepresentation(
		name: "Elopli Light"
	)
	static var defaultQuery = ElopliLightQuery()

	let id: String
	let name: String
	let detail: String

	var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(title: "\(name)", subtitle: "\(detail)")
	}
}

struct ElopliLightQuery: EntityQuery {
	func entities(for identifiers: [String]) async throws -> [ElopliLightEntity] {
		let all = try Self.allEntities()
		return identifiers.compactMap { identifier in
			all.first { $0.id == identifier }
		}
	}

	func suggestedEntities() async throws -> [ElopliLightEntity] {
		try Self.allEntities()
	}

	func defaultResult() async -> ElopliLightEntity? {
		try? Self.allEntities().dropFirst().first
	}

	private static func allEntities() throws -> [ElopliLightEntity] {
		let config = try FfiConfig.load(path: nil)
		let devices = try config.devices()
		let all = ElopliLightEntity(
			id: ElopliLightEntity.allID,
			name: "All Lights",
			detail: "Every configured light"
		)
		return [all] + devices.map { device in
			ElopliLightEntity(
				id: "device:\(device.name)",
				name: device.name,
				detail: device.host
			)
		}
	}
}

struct DeviceResult {
	let name: String
	let message: String?
	let error: String?
}

struct AnimationDeviceResult {
	let name: String
	let outcome: FfiPulseOutcome?
	let error: String?
}

enum IntentSupport {
	static func config() throws -> FfiConfig {
		try FfiConfig.load(path: nil)
	}

	static func describe(_ error: Error) -> String {
		if let ffi = error as? FfiError {
			switch ffi {
			case let .Message(message):
				return message
			case let .Detailed(_, message):
				return message
			}
		}
		return error.localizedDescription
	}

	static func devices(
		from entities: [ElopliLightEntity]
	) throws -> [FfiDevice] {
		let config = try config()
		let all = try config.devices()
		if entities.contains(where: { $0.id == ElopliLightEntity.allID }) {
			return all
		}
		guard !entities.isEmpty else {
			throw IntentError.message("Choose at least one light, or choose All Lights.")
		}
		let names = Set(entities.map(\.name))
		let matched = all.filter { names.contains($0.name) }
		guard !matched.isEmpty else {
			throw IntentError.message("None of the selected lights are in the registry.")
		}
		return matched
	}

	static func client(for device: FfiDevice) throws -> FfiClient {
		try FfiClient(
			host: device.host,
			port: device.port,
			timeoutSeconds: 3.0,
			retries: 2,
			transport: "auto"
		)
	}

	static func run<T>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
		try await Task.detached { try body() }.value
	}

	static func names(_ devices: [FfiDevice]) -> String {
		devices.map(\.name).joined(separator: ", ")
	}

	static func duration(
		_ text: String,
		minimum: UInt32,
		maximum: UInt32
	) throws -> UInt32 {
		let value: UInt32
		do {
			value = try parseDuration(input: text)
		} catch {
			throw IntentError.message(describe(error))
		}
		guard value >= minimum, value <= maximum else {
			throw IntentError.message(
				"Duration must be between \(minimum) and \(maximum) ms."
			)
		}
		return value
	}

	static func runEach(
		_ devices: [FfiDevice],
		_ body: @escaping @Sendable (FfiDevice, FfiClient) throws -> String
	) async -> [DeviceResult] {
		await withTaskGroup(of: DeviceResult.self) { group in
			for device in devices {
				group.addTask {
					do {
						let client = try client(for: device)
						let message = try await run { try body(device, client) }
						return DeviceResult(name: device.name, message: message, error: nil)
					} catch {
						return DeviceResult(
							name: device.name,
							message: nil,
							error: describe(error)
						)
					}
				}
			}
			var results: [DeviceResult] = []
			for await result in group {
				results.append(result)
			}
			return devices.compactMap { device in
				results.first { $0.name == device.name }
			}
		}
	}

	static func runPulses(
		_ devices: [FfiDevice],
		count: UInt32,
		durationMs: UInt32,
		value: UInt8,
		low: UInt8
	) async -> [AnimationDeviceResult] {
		await withTaskGroup(of: AnimationDeviceResult.self) { group in
			for device in devices {
				group.addTask {
					do {
						let client = try client(for: device)
						let outcome = try await run {
							try client.pulse(
								count: count,
								durationMs: durationMs,
								value: value,
								low: low
							)
						}
						return AnimationDeviceResult(
							name: device.name,
							outcome: outcome,
							error: nil
						)
					} catch {
						return AnimationDeviceResult(
							name: device.name,
							outcome: nil,
							error: describe(error)
						)
					}
				}
			}
			var results: [AnimationDeviceResult] = []
			for await result in group {
				results.append(result)
			}
			return devices.compactMap { device in
				results.first { $0.name == device.name }
			}
		}
	}

	static func runSweeps(
		_ devices: [FfiDevice],
		count: UInt32,
		durationMs: UInt32,
		brightness: UInt8?
	) async -> [AnimationDeviceResult] {
		await withTaskGroup(of: AnimationDeviceResult.self) { group in
			for device in devices {
				group.addTask {
					do {
						let client = try client(for: device)
						let outcome = try await run {
							try client.sweep(
								count: count,
								durationMs: durationMs,
								brightness: brightness
							)
						}
						return AnimationDeviceResult(
							name: device.name,
							outcome: outcome,
							error: nil
						)
					} catch {
						return AnimationDeviceResult(
							name: device.name,
							outcome: nil,
							error: describe(error)
						)
					}
				}
			}
			var results: [AnimationDeviceResult] = []
			for await result in group {
				results.append(result)
			}
			return devices.compactMap { device in
				results.first { $0.name == device.name }
			}
		}
	}

	static func summary(_ action: String, _ results: [DeviceResult]) -> String {
		let succeeded = results.filter { $0.error == nil }.map(\.name)
		let failed = results.filter { $0.error != nil }
		if failed.isEmpty {
			return succeeded.isEmpty ? "\(action) no lights." : "\(action) \(succeeded.joined(separator: ", "))"
		}
		let failures = failed.map { "\($0.name): \($0.error ?? "unknown error")" }
		if succeeded.isEmpty {
			return "Failed to \(action.lowercased()): \(failures.joined(separator: "; "))"
		}
		return "\(action) \(succeeded.joined(separator: ", ")). Failed: \(failures.joined(separator: "; "))"
	}

	static func animationSummary(
		_ action: String,
		_ results: [AnimationDeviceResult]
	) -> String {
		let completed = results.filter { $0.outcome?.completed == true }.map(\.name)
		let stopped = results.filter { $0.outcome?.completed == false }.map(\.name)
		let failed = results.filter { $0.error != nil }
		var parts: [String] = []
		if !completed.isEmpty {
			parts.append("\(action) \(completed.joined(separator: ", "))")
		}
		if !stopped.isEmpty {
			parts.append("stopped on \(stopped.joined(separator: ", "))")
		}
		if !failed.isEmpty {
			parts.append(
				"failed on \(failed.map { "\($0.name): \($0.error ?? "unknown error")" }.joined(separator: "; "))"
			)
		}
		return parts.isEmpty ? "No lights were changed." : parts.joined(separator: ". ")
	}
}

struct TurnOnIntent: AppIntent {
	static var title: LocalizedStringResource = "Turn Light On"
	static var description = IntentDescription(
		"Turn one or more Elopli lights, or all lights, on."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Turn Light On",
			image: DisplayRepresentation.Image(systemName: "lightbulb")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let results = await IntentSupport.runEach(devices) { _, client in
			_ = try client.setOn(on: true)
			return "on"
		}
		return .result(dialog: IntentDialog(stringLiteral: IntentSupport.summary("Turned on", results)))
	}
}

struct TurnOffIntent: AppIntent {
	static var title: LocalizedStringResource = "Turn Light Off"
	static var description = IntentDescription(
		"Turn one or more Elopli lights, or all lights, off."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Turn Light Off",
			image: DisplayRepresentation.Image(systemName: "lightbulb.slash")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let results = await IntentSupport.runEach(devices) { _, client in
			_ = try client.setOn(on: false)
			return "off"
		}
		return .result(dialog: IntentDialog(stringLiteral: IntentSupport.summary("Turned off", results)))
	}
}

struct ToggleIntent: AppIntent {
	static var title: LocalizedStringResource = "Toggle Light"
	static var description = IntentDescription(
		"Toggle one or more Elopli lights, or all lights."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Toggle Light",
			image: DisplayRepresentation.Image(systemName: "lightbulb")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let results = await IntentSupport.runEach(devices) { _, client in
			_ = try client.toggle()
			return "toggled"
		}
		return .result(dialog: IntentDialog(stringLiteral: IntentSupport.summary("Toggled", results)))
	}
}

struct SetBrightnessIntent: AppIntent {
	static var title: LocalizedStringResource = "Set Light Brightness"
	static var description = IntentDescription(
		"Set the brightness of one or more Elopli lights, or all lights."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Set Light Brightness",
			image: DisplayRepresentation.Image(systemName: "sun.max")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	@Parameter(
		title: "Brightness",
		description: "Brightness from 2 to 100.",
		default: 50
	)
	var brightness: Int

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let value = UInt8(max(2, min(100, brightness)))
		let devices = try IntentSupport.devices(from: lights)
		let results = await IntentSupport.runEach(devices) { _, client in
			_ = try client.setBrightness(value: value)
			return "brightness"
		}
		return .result(dialog: IntentDialog(stringLiteral: "\(IntentSupport.summary("Set brightness", results)) to \(value)%"))
	}
}

struct SetWarmthIntent: AppIntent {
	static var title: LocalizedStringResource = "Set Light Warmth"
	static var description = IntentDescription(
		"Set the warmth of one or more Elopli lights, or all lights."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Set Light Warmth",
			image: DisplayRepresentation.Image(systemName: "flame")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	@Parameter(
		title: "Warmth",
		description: "Warmth from 0 (cool) to 100 (warm).",
		default: 50
	)
	var warmth: Int

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let value = UInt8(max(0, min(100, warmth)))
		let devices = try IntentSupport.devices(from: lights)
		let results = await IntentSupport.runEach(devices) { _, client in
			_ = try client.setWarmth(value: value)
			return "warmth"
		}
		return .result(dialog: IntentDialog(stringLiteral: "\(IntentSupport.summary("Set warmth", results)) to \(value)%"))
	}
}

struct GetStateIntent: AppIntent {
	static var title: LocalizedStringResource = "Get Light State"
	static var description = IntentDescription(
		"Read the state of one or more Elopli lights, or all lights."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Get Light State",
			image: DisplayRepresentation.Image(systemName: "info.circle")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let results = await IntentSupport.runEach(devices) { device, client in
			let state = try client.state()
			return "\(device.name): \(state.on ? "on" : "off"), "
				+ "\(state.brightness)% brightness, \(state.warmth)% warmth"
		}
		let lines = results.map { $0.message ?? $0.error ?? "unknown error" }
		return .result(dialog: IntentDialog(stringLiteral: lines.joined(separator: "\n")))
	}
}

struct RenameIntent: AppIntent {
	static var title: LocalizedStringResource = "Rename Light"
	static var description = IntentDescription(
		"Change the display name of one Elopli light."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Rename Light",
			image: DisplayRepresentation.Image(systemName: "pencil")
		)
	}

	@Parameter(title: "Light", description: "The light to rename.")
	var light: ElopliLightEntity

	@Parameter(title: "New Name")
	var newName: String

	func perform() async throws -> some IntentResult & ProvidesDialog {
		guard light.id != ElopliLightEntity.allID else {
			throw IntentError.message("Choose one light to rename.")
		}
		let devices = try IntentSupport.devices(from: [light])
		guard devices.count == 1 else {
			throw IntentError.message("Choose one light to rename.")
		}
		let device = devices[0]
		do {
			let client = try IntentSupport.client(for: device)
			let result = try await IntentSupport.run { try client.rename(name: newName) }
			return .result(dialog: "Renamed \(device.name) to \(result)")
		} catch {
			throw IntentError.message(IntentSupport.describe(error))
		}
	}
}

enum PowerOnBehaviorChoice: String, AppEnum {
	case restoreLast
	case restoreDefaults

	static var typeDisplayRepresentation = TypeDisplayRepresentation(
		name: "Power-On Behaviour"
	)
	static var caseDisplayRepresentations: [Self: DisplayRepresentation] = [
		.restoreLast: "Restore Last Used Setting",
		.restoreDefaults: "Restore Defaults",
	]

	var value: UInt8 {
		switch self {
		case .restoreLast: return 1
		case .restoreDefaults: return 2
		}
	}
}

struct PulseIntent: AppIntent {
	static var title: LocalizedStringResource = "Pulse Light"
	static var description = IntentDescription(
		"Pulse brightness from a peak to the minimum, then restore the original values."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Pulse Light",
			image: DisplayRepresentation.Image(systemName: "waveform.path.ecg")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	@Parameter(
		title: "Count",
		description: "Number of peak-to-minimum pulses.",
		default: 3
	)
	var count: Int

	@Parameter(
		title: "Duration",
		description: "Duration, for example 1.0s, 500ms, or 10m. Bare numbers are seconds.",
		default: "1.0s"
	)
	var duration: String

	@Parameter(
		title: "Peak Value",
		description: "Peak brightness from 2 to 100.",
		default: 100
	)
	var value: Int

	@Parameter(
		title: "Low Value",
		description: "Low brightness percentage. 0 or 1 turns the light off during the pulse.",
		default: 0
	)
	var low: Int

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let count = UInt32(max(1, min(20, count)))
		let minimum = (count * 2 + 1) * 100
		let durationMs = try IntentSupport.duration(
			duration,
			minimum: minimum,
			maximum: 3_600_000
		)
		let value = UInt8(max(2, min(100, value)))
		let low = UInt8(min(Int(value), max(0, min(100, low))))
		let results = await IntentSupport.runPulses(
			devices,
			count: count,
			durationMs: durationMs,
			value: value,
			low: low
		)
		return .result(
			dialog: IntentDialog(
				stringLiteral: IntentSupport.animationSummary("Pulsed", results)
			)
		)
	}
}

struct SweepLightIntent: AppIntent {
	static var title: LocalizedStringResource = "Sweep Light"
	static var description = IntentDescription(
		"Sweep temperature across the full range, then restore the original values."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Sweep Light",
			image: DisplayRepresentation.Image(systemName: "arrow.left.and.right")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	@Parameter(
		title: "Count",
		description: "Number of warm-to-cool sweeps.",
		default: 1
	)
	var count: Int

	@Parameter(
		title: "Duration",
		description: "Duration, for example 1.0s, 500ms, or 10m. Bare numbers are seconds.",
		default: "1.0s"
	)
	var duration: String

	@Parameter(
		title: "Brightness",
		description: "Brightness to hold during the sweep. 0 or 1 keeps the current brightness.",
		default: 0
	)
	var brightness: Int

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let count = UInt32(max(1, min(20, count)))
		let minimum = (count * 2 + 1) * 100
		let durationMs = try IntentSupport.duration(
			duration,
			minimum: minimum,
			maximum: 3_600_000
		)
		let brightness = max(0, min(100, brightness))
		let results = await IntentSupport.runSweeps(
			devices,
			count: count,
			durationMs: durationMs,
			brightness: brightness >= 2 ? UInt8(brightness) : nil
		)
		return .result(
			dialog: IntentDialog(
				stringLiteral: IntentSupport.animationSummary("Swept", results)
			)
		)
	}
}

struct SetPowerOnIntent: AppIntent {
	static var title: LocalizedStringResource = "Set Power-On Behaviour"
	static var description = IntentDescription(
		"Set what one or more Elopli lights do when they receive power."
	)
	static var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(
			title: "Set Power-On Behaviour",
			image: DisplayRepresentation.Image(systemName: "power")
		)
	}

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [ElopliLightEntity]

	@Parameter(title: "Behaviour", default: .restoreLast)
	var behavior: PowerOnBehaviorChoice

	@Parameter(
		title: "Default Brightness",
		description: "Used with Restore Defaults, from 2 to 100.",
		default: 20
	)
	var brightness: Int

	@Parameter(
		title: "Default Warmth",
		description: "Used with Restore Defaults, from 0 to 100.",
		default: 35
	)
	var warmth: Int

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let defaults = behavior == .restoreDefaults
		let brightnessValue = defaults ? UInt8(max(2, min(100, brightness))) : nil
		let temperature = defaults
			? UInt16(max(0, min(100, warmth))) * 201 / 100 + 143
			: nil
		let results = await IntentSupport.runEach(devices) { _, client in
			_ = try client.setPowerOn(
				behavior: behavior.value,
				brightness: brightnessValue,
				temperatureMired: temperature
			)
			return "power-on"
		}
		return .result(dialog: IntentDialog(stringLiteral: IntentSupport.summary("Set power-on behaviour", results)))
	}
}

struct ElopliShortcuts: AppShortcutsProvider {
	static var appShortcuts: [AppShortcut] {
		AppShortcut(
			intent: TurnOnIntent(),
			phrases: [
				"Turn on \(.applicationName)",
				"Turn on my lights with \(.applicationName)",
			],
			shortTitle: "Turn On",
			systemImageName: "lightbulb"
		)
		AppShortcut(
			intent: TurnOffIntent(),
			phrases: [
				"Turn off \(.applicationName)",
				"Turn off my lights with \(.applicationName)",
			],
			shortTitle: "Turn Off",
			systemImageName: "lightbulb.slash"
		)
		AppShortcut(
			intent: ToggleIntent(),
			phrases: [
				"Toggle \(.applicationName)"
			],
			shortTitle: "Toggle",
			systemImageName: "lightbulb"
		)
		AppShortcut(
			intent: SetBrightnessIntent(),
			phrases: [
				"Set brightness with \(.applicationName)"
			],
			shortTitle: "Set Brightness",
			systemImageName: "sun.max"
		)
		AppShortcut(
			intent: SetWarmthIntent(),
			phrases: [
				"Set warmth with \(.applicationName)"
			],
			shortTitle: "Set Warmth",
			systemImageName: "flame"
		)
		AppShortcut(
			intent: GetStateIntent(),
			phrases: [
				"Get light state with \(.applicationName)"
			],
			shortTitle: "Get State",
			systemImageName: "info.circle"
		)
		AppShortcut(
			intent: PulseIntent(),
			phrases: [
				"Pulse \(.applicationName)"
			],
			shortTitle: "Pulse",
			systemImageName: "waveform.path.ecg"
		)
		AppShortcut(
			intent: SweepLightIntent(),
			phrases: [
				"Sweep \(.applicationName)"
			],
			shortTitle: "Sweep",
			systemImageName: "arrow.left.and.right"
		)
	}
}
