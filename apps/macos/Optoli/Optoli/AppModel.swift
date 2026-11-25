import Foundation
import ServiceManagement
import SwiftUI

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

	private var config: FfiConfig?

	init() {
		do {
			let config = try FfiConfig.load(path: nil)
			self.config = config
			self.configPath = (try? config.path()) ?? ""
			reloadDevices()
		} catch {
			self.errorMessage = Self.describe(error)
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
		} catch {
			errorMessage = Self.describe(error)
		}
	}

	func refresh() async {
		guard !isRefreshing else { return }
		isRefreshing = true
		defer { isRefreshing = false }
		let current = devices

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
		_ device: LightDevice,
		action: @escaping @Sendable (FfiClient) throws -> FfiLightState
	) async {
		do {
			let client = try Self.client(for: device)
			let state = try await Task.detached { try action(client) }.value
			if let index = devices.firstIndex(where: { $0.name == device.name }) {
				devices[index].state = state
				devices[index].error = nil
			}
		} catch {
			setError(device, error)
		}
	}

	func setOn(_ device: LightDevice, on: Bool) async {
		await update(device) { try $0.setOn(on: on) }
	}

	func toggle(_ device: LightDevice) async {
		await update(device) { try $0.toggle() }
	}

	func setBrightness(_ device: LightDevice, value: UInt8) async {
		await update(device) { try $0.setBrightness(value: value) }
	}

	func setWarmth(_ device: LightDevice, value: UInt8) async {
		await update(device) { try $0.setWarmth(value: value) }
	}

	func setKelvin(_ device: LightDevice, value: UInt16) async {
		await update(device) { try $0.setKelvin(value: value) }
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

	private func setError(_ device: LightDevice, _ error: Error) {
		if let index = devices.firstIndex(where: { $0.name == device.name }) {
			devices[index].error = Self.describe(error)
		}
	}
}
