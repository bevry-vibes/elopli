import AppKit
import Combine
import SwiftUI

struct ContentView: View {
	@ObservedObject var model: AppModel
	@State private var showDiscovery = false
	@State private var contentHeight: CGFloat = 500

	private let maxContentHeight: CGFloat = 540

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
		.frame(width: 460)
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
			.frame(maxWidth: .infinity)
			.frame(height: 300)
		} else {
			ScrollView {
				VStack(alignment: .leading, spacing: 12) {
					ForEach(model.devices) { device in
						LightCardView(model: model, device: device)
					}
					footer
						.padding(.top, 2)
				}
				.background {
					GeometryReader { geometry in
						Color.clear.preference(
							key: PanelContentHeightKey.self,
							value: geometry.size.height
						)
					}
				}
			}
			.frame(height: min(contentHeight, maxContentHeight))
			.scrollBounceBehavior(.basedOnSize)
			.onPreferenceChange(PanelContentHeightKey.self) { height in
				contentHeight = height
			}
		}
	}

	private var footer: some View {
		Text("Hold ⌘ and click a card, toggle, or slider to link lights. Linked items change together.")
			.font(.caption2)
			.foregroundStyle(.secondary)
	}
}

private struct PanelContentHeightKey: PreferenceKey {
	static let defaultValue: CGFloat = 0

	static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
		value = max(value, nextValue())
	}
}

private struct DiscoveryRow: Identifiable {
	enum State {
		case added
		case missing
		case new
	}

	let id: String
	let name: String
	let detail: String
	let state: State
	let discovered: FfiDiscoveredDevice?
}

struct DiscoverySheet: View {
	@ObservedObject var model: AppModel
	@Environment(\.dismiss) private var dismiss

	var body: some View {
		let rows = discoveryRows
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
			} else if rows.isEmpty {
				ContentUnavailableView(
					"No lights found",
					systemImage: "dot.radiowaves.left.and.right",
					description: Text("Make sure the lights are on the same network.")
				)
				.frame(maxWidth: .infinity, minHeight: 160)
			} else {
				List(rows) { row in
					HStack {
						VStack(alignment: .leading) {
							Text(row.name)
							Text(row.detail)
								.font(.caption)
								.foregroundStyle(.secondary)
						}
						Spacer()
						switch row.state {
						case .added:
							Button("Added") {}
								.buttonStyle(.bordered)
								.disabled(true)
						case .missing:
							Text("Missing")
								.font(.callout)
								.padding(.horizontal, 10)
								.padding(.vertical, 4)
								.foregroundStyle(.red)
								.background(
									RoundedRectangle(cornerRadius: 6)
										.stroke(.red.opacity(0.45), lineWidth: 1)
								)
								.opacity(0.75)
								.help("Not found on the network. Power cycle the light, wait for it to reconnect, then scan again.")
						case .new:
							if let device = row.discovered {
								Button("Add") {
									model.addDiscovered(device)
								}
							}
						}
					}
				}
				.frame(minHeight: 220)
			}
			Spacer(minLength: 0)
			HStack {
				Spacer()
				Button("Done") { dismiss() }
					.keyboardShortcut(.defaultAction)
			}
		}
		.padding(20)
		.frame(width: 560, height: 380)
	}

	private var discoveryRows: [DiscoveryRow] {
		var rows = model.devices.map { known -> DiscoveryRow in
			let discovered = model.discovered.first { matches(known, $0) }
			let detail: String
			if let discovered {
				detail = "\(discovered.product) · \(discovered.address ?? discovered.host)"
			} else {
				detail = "\(known.info?.product ?? known.host) · \(known.host) · not found"
			}
			return DiscoveryRow(
				id: "known-\(known.name)",
				name: known.name,
				detail: detail,
				state: discovered == nil ? .missing : .added,
				discovered: discovered
			)
		}
		for device in model.discovered
		where !model.devices.contains(where: { matches($0, device) }) {
			rows.append(
				DiscoveryRow(
					id: "new-\(device.name)",
					name: device.name,
					detail: "\(device.product) · \(device.address ?? device.host)",
					state: .new,
					discovered: device
				)
			)
		}
		return rows
	}

	private func matches(_ known: LightDevice, _ discovered: FfiDiscoveredDevice) -> Bool {
		let host = discovered.address ?? discovered.host
		return known.name == discovered.name || known.host == host
	}
}
