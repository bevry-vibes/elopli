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
		try? Self.allEntities().first
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

enum IntentSupport {
	static func config() throws -> FfiConfig {
		try FfiConfig.load(path: nil)
	}

	static func devices(
		from entities: [ElopliLightEntity]
	) throws -> [FfiDevice] {
		let config = try config()
		let all = try config.devices()
		if entities.isEmpty || entities.contains(where: { $0.id == ElopliLightEntity.allID }) {
			return all
		}
		let names = Set(entities.map(\.name))
		return all.filter { names.contains($0.name) }
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
		for device in devices {
			let client = try IntentSupport.client(for: device)
			_ = try await IntentSupport.run { try client.setOn(on: true) }
		}
		return .result(dialog: "Turned on \(IntentSupport.names(devices))")
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
		for device in devices {
			let client = try IntentSupport.client(for: device)
			_ = try await IntentSupport.run { try client.setOn(on: false) }
		}
		return .result(dialog: "Turned off \(IntentSupport.names(devices))")
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
		for device in devices {
			let client = try IntentSupport.client(for: device)
			_ = try await IntentSupport.run { try client.toggle() }
		}
		return .result(dialog: "Toggled \(IntentSupport.names(devices))")
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
		for device in devices {
			let client = try IntentSupport.client(for: device)
			_ = try await IntentSupport.run { try client.setBrightness(value: value) }
		}
		return .result(dialog: "Set \(IntentSupport.names(devices)) to \(value)%")
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
			image: DisplayRepresentation.Image(systemName: "thermometer.medium")
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
		for device in devices {
			let client = try IntentSupport.client(for: device)
			_ = try await IntentSupport.run { try client.setWarmth(value: value) }
		}
		return .result(
			dialog: "Set warmth on \(IntentSupport.names(devices)) to \(value)%"
		)
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
		var lines: [String] = []
		for device in devices {
			let client = try IntentSupport.client(for: device)
			let state = try await IntentSupport.run { try client.state() }
			lines.append(
				"\(device.name): \(state.on ? "on" : "off"), "
					+ "\(state.brightness)% brightness, \(state.warmth)% warmth"
			)
		}
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
		let devices = try IntentSupport.devices(from: [light])
		guard devices.count == 1 else {
			throw IntentError.message("Choose one light to rename.")
		}
		let device = devices[0]
		let client = try IntentSupport.client(for: device)
		let result = try await IntentSupport.run { try client.rename(name: newName) }
		return .result(dialog: "Renamed \(device.name) to \(result)")
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
		description: "Target duration of the pulse animation in milliseconds. The minimum hold can extend it.",
		default: 3000
	)
	var durationMs: Int


	@Parameter(
		title: "Peak Value",
		description: "Peak brightness percentage.",
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
		let durationMs = UInt32(max(50, min(10_000, durationMs)))
		let value = UInt8(max(0, min(100, value)))
		let low = UInt8(min(Int(value), max(0, min(100, low))))
		var stopped = 0
		for device in devices {
			let client = try IntentSupport.client(for: device)
			let outcome = try await IntentSupport.run {
				try client.pulse(
					count: count,
					durationMs: durationMs,
					value: value,
					low: low
				)
			}
			if !outcome.completed {
				stopped += 1
			}
		}
		if stopped > 0 {
			return .result(
				dialog: "Stopped pulsing \(IntentSupport.names(devices)) because the light changed or another action took over."
			)
		}
		return .result(
			dialog: "Pulsed \(IntentSupport.names(devices))"
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
		description: "Duration of the entire sweep animation in milliseconds.",
		default: 3000
	)
	var durationMs: Int


	@Parameter(
		title: "Brightness",
		description: "Brightness to hold during the sweep. 0 keeps the current brightness.",
		default: 0
	)
	var brightness: Int

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		let count = UInt32(max(1, min(20, count)))
		let durationMs = UInt32(max(100, min(60_000, durationMs)))
		let brightness = max(0, min(100, brightness))
		var stopped = 0
		for device in devices {
			let client = try IntentSupport.client(for: device)
			let outcome = try await IntentSupport.run {
				try client.sweep(
					count: count,
					durationMs: durationMs,
					brightness: brightness > 0 ? UInt8(brightness) : nil
				)
			}
			if !outcome.completed {
				stopped += 1
			}
		}
		if stopped > 0 {
			return .result(
				dialog: "Stopped sweeping \(IntentSupport.names(devices)) because the light changed or another action took over."
			)
		}
		return .result(
			dialog: "Swept \(IntentSupport.names(devices))"
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

	func perform() async throws -> some IntentResult & ProvidesDialog {
		let devices = try IntentSupport.devices(from: lights)
		for device in devices {
			let client = try IntentSupport.client(for: device)
			_ = try await IntentSupport.run {
				try client.setPowerOn(
					behavior: behavior.value,
					brightness: nil,
					temperatureMired: nil
				)
			}
		}
		return .result(
			dialog: "Set power-on behaviour on \(IntentSupport.names(devices))"
		)
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
			systemImageName: "thermometer.medium"
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
	}
}
