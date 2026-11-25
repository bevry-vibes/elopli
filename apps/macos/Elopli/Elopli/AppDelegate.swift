import AppKit
import Foundation

final class AppDelegate: NSObject, NSApplicationDelegate {
	weak var model: AppModel?

	func applicationShouldTerminate(
		_ sender: NSApplication
	) -> NSApplication.TerminateReply {
		guard let model, model.hasRunningAnimations else {
			return .terminateNow
		}
		Task { @MainActor in
			model.cancelAllAnimations()
			let deadline = Date().addingTimeInterval(5)
			while model.hasRunningAnimations && Date() < deadline {
				try? await Task.sleep(for: .milliseconds(100))
			}
			NSApplication.shared.reply(toApplicationShouldTerminate: true)
		}
		return .terminateLater
	}

	func applicationDidFinishLaunching(_ notification: Notification) {
		guard CommandLine.arguments.contains("--smoke-test") else { return }
		Task {
			do {
				let config = try FfiConfig.load(path: nil)
				let devices = try config.devices()
				print("config: \(try config.path())")
				for device in devices {
					let client = try FfiClient(
						host: device.host,
						port: device.port,
						timeoutSeconds: 3.0,
						retries: 2,
						transport: "auto"
					)
					let snapshot = try client.snapshot()
					let wifi = snapshot.info.wifi?.ssid ?? "-"
					print(
						"\(device.name): \(snapshot.state.on ? "on" : "off") "
							+ "\(snapshot.state.brightness)% "
							+ "\(snapshot.state.warmth)% wifi=\(wifi)"
					)
				}
				exit(0)
			} catch {
				print("smoke test failed: \(AppModel.describe(error))")
				exit(1)
			}
		}
	}
}
