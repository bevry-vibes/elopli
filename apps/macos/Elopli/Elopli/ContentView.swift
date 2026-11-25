import AppKit
import Combine
import SwiftUI

struct ContentView: View {
	@ObservedObject var model: AppModel
	@State private var showDiscovery = false

	private let timer = Timer.publish(
		every: 5,
		on: .main,
		in: .common
	).autoconnect()

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			header
			if let error = model.errorMessage {
				Text(error)
					.font(.caption)
					.foregroundStyle(.red)
					.lineLimit(2)
			}
			content
		}
		.padding(18)
		.frame(width: 460, height: 640)
		.onAppear {
			Task { await model.refresh() }
		}
		.onReceive(timer) { _ in
			Task { await model.refresh() }
		}
		.onReceive(
			NotificationCenter.default.publisher(
				for: NSApplication.didBecomeActiveNotification
			)
		) { _ in
			Task { await model.refresh() }
		}
		.onDisappear {
			model.resetLinkInteraction()
		}
		.sheet(isPresented: $showDiscovery) {
			DiscoverySheet(model: model)
		}
	}

	private var header: some View {
		HStack(alignment: .center, spacing: 10) {
			Text("Elopli")
				.font(.title2.bold())
			Spacer()
			if model.isRefreshing {
				ProgressView()
					.controlSize(.small)
			}
			Button {
				Task { await model.refresh() }
			} label: {
				Image(systemName: "arrow.clockwise")
			}
			.buttonStyle(.borderless)
			.help("Refresh")
			Button {
				showDiscovery = true
				Task { await model.scan() }
			} label: {
				Image(systemName: "dot.radiowaves.left.and.right")
			}
			.buttonStyle(.borderless)
			.help("Scan for lights")
			Button {
				Task { await model.setAll(on: true) }
			} label: {
				Image(systemName: "lightbulb.fill")
			}
			.buttonStyle(.borderless)
			.help("All On")
			Button {
				Task { await model.setAll(on: false) }
			} label: {
				Image(systemName: "lightbulb.slash")
			}
			.buttonStyle(.borderless)
			.help("All Off")
			Menu {
				Toggle(
					"Launch at Login",
					isOn: Binding(
						get: { model.launchAtLogin },
						set: { model.setLaunchAtLogin($0) }
					)
				)
				Divider()
				Button("Reveal Config") {
					let url = URL(fileURLWithPath: model.configPath)
					if FileManager.default.fileExists(atPath: url.path) {
						NSWorkspace.shared.activateFileViewerSelecting([url])
					} else {
						NSWorkspace.shared.open(url.deletingLastPathComponent())
					}
				}
				Divider()
				Button("Quit Elopli") {
					NSApplication.shared.terminate(nil)
				}
			} label: {
				Image(systemName: "gearshape")
			}
			.menuStyle(.borderlessButton)
			.help("Settings")
		}
	}

	@ViewBuilder
	private var content: some View {
		if model.devices.isEmpty {
			ContentUnavailableView(
				"No lights configured",
				systemImage: "lightbulb.slash",
				description: Text(
					"Add devices to \(model.configPath), or scan the local network."
				)
			)
			.frame(maxWidth: .infinity, maxHeight: .infinity)
		} else {
			ScrollView {
				LazyVStack(spacing: 12) {
					ForEach(model.devices) { device in
						LightCardView(model: model, device: device)
					}
					footer
						.padding(.top, 2)
				}
				.padding(.vertical, 4)
				.padding(.horizontal, 4)
			}
		}
	}

	private var footer: some View {
		Text("Hold ⌘ and click a card, toggle, or slider to link lights. Linked items change together.")
			.font(.caption2)
			.foregroundStyle(.secondary)
	}
}

struct DiscoverySheet: View {
	@ObservedObject var model: AppModel
	@Environment(\.dismiss) private var dismiss

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			HStack {
				Text("Discover Lights")
					.font(.title3.bold())
				Spacer()
				Button {
					Task { await model.scan() }
				} label: {
					Label("Scan Again", systemImage: "arrow.clockwise")
				}
				.disabled(model.isScanning)
			}
			if model.isScanning {
				ProgressView("Scanning the local network…")
					.frame(maxWidth: .infinity, minHeight: 120)
			} else if model.discovered.isEmpty {
				ContentUnavailableView(
					"No lights found",
					systemImage: "dot.radiowaves.left.and.right",
					description: Text("Make sure the lights are on the same network.")
				)
				.frame(maxWidth: .infinity, minHeight: 160)
			} else {
				List(model.discovered, id: \.name) { device in
					HStack {
						VStack(alignment: .leading) {
							Text(device.name)
							Text("\(device.product) · \(device.address ?? device.host)")
								.font(.caption)
								.foregroundStyle(.secondary)
						}
						Spacer()
						Button("Add") {
							model.addDiscovered(device)
						}
					}
				}
				.frame(minHeight: 220)
			}
			HStack {
				Spacer()
				Button("Done") { dismiss() }
					.keyboardShortcut(.defaultAction)
			}
		}
		.padding(20)
		.frame(width: 560, height: 380)
	}
}
