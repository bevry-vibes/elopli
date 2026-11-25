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

struct OptoliLightEntity: AppEntity, Hashable {
	static let allID = "all"

	static var typeDisplayRepresentation = TypeDisplayRepresentation(
		name: "Optoli Light"
	)
	static var defaultQuery = OptoliLightQuery()

	let id: String
	let name: String
	let detail: String

	var displayRepresentation: DisplayRepresentation {
		DisplayRepresentation(title: "\(name)", subtitle: "\(detail)")
	}
}

struct OptoliLightQuery: EntityQuery {
	func entities(for identifiers: [String]) async throws -> [OptoliLightEntity] {
		let all = try Self.allEntities()
		return identifiers.compactMap { identifier in
			all.first { $0.id == identifier }
		}
	}

	func suggestedEntities() async throws -> [OptoliLightEntity] {
		try Self.allEntities()
	}

	func defaultResult() async -> OptoliLightEntity? {
		try? Self.allEntities().first
	}

	private static func allEntities() throws -> [OptoliLightEntity] {
		let config = try FfiConfig.load(path: nil)
		let devices = try config.devices()
		let all = OptoliLightEntity(
			id: OptoliLightEntity.allID,
			name: "All Lights",
			detail: "Every configured light"
		)
		return [all] + devices.map { device in
			OptoliLightEntity(
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
		from entities: [OptoliLightEntity]
	) throws -> [FfiDevice] {
		let config = try config()
		let all = try config.devices()
		if entities.isEmpty || entities.contains(where: { $0.id == OptoliLightEntity.allID }) {
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
		"Turn one or more Optoli lights, or all lights, on."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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
		"Turn one or more Optoli lights, or all lights, off."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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
		"Toggle one or more Optoli lights, or all lights."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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
		"Set the brightness of one or more Optoli lights, or all lights."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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
		"Set the warmth of one or more Optoli lights, or all lights."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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
		"Read the state of one or more Optoli lights, or all lights."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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
		"Change the display name of one Optoli light."
	)

	@Parameter(title: "Light", description: "The light to rename.")
	var light: OptoliLightEntity

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

struct SetPowerOnIntent: AppIntent {
	static var title: LocalizedStringResource = "Set Power-On Behaviour"
	static var description = IntentDescription(
		"Set what one or more Optoli lights do when they receive power."
	)

	@Parameter(
		title: "Lights",
		description: "Select one or more lights, or All Lights."
	)
	var lights: [OptoliLightEntity]

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

struct OptoliShortcuts: AppShortcutsProvider {
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
	}
}
