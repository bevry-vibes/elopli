import SwiftUI

struct LightCardView: View {
	@ObservedObject var model: AppModel
	let device: LightDevice

	@State private var brightness: Double
	@State private var warmth: Double
	@State private var isEditingBrightness = false
	@State private var isEditingWarmth = false
	@State private var showRename = false
	@State private var showPowerOn = false
	@State private var showPulse = false
	@State private var showSweep = false
	@State private var showPulseGroup = false
	@State private var showSweepGroup = false
	@State private var wifiHelpPresented = false
	@State private var wifiHoverTask: Task<Void, Never>?

	init(model: AppModel, device: LightDevice) {
		self.model = model
		self.device = device
		_brightness = State(
			initialValue: Double(max(2, min(100, device.state?.brightness ?? 50)))
		)
		_warmth = State(initialValue: Double(device.state?.warmth ?? 50))
	}

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			header

			if let error = device.error {
				Text(error)
					.font(.caption)
					.foregroundStyle(.red)
					.lineLimit(2)
			}

			brightnessRow
			warmthRow
			footer
		}
		.padding(14)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
		.overlay(
			RoundedRectangle(cornerRadius: 18)
				.stroke(.quaternary, lineWidth: 1)
		)
		.overlay(linkBorder(.card, cornerRadius: 18))
		.contentShape(Rectangle())
		.onDrag {
			NSItemProvider(object: device.name as NSString)
		}
		.onTapGesture {
			if model.commandDown {
				model.toggleLinkSelection(.card, device: device)
			}
		}
		.onHover { hovering in
			model.setHover(
				hovering
					? LinkTarget(device: device.name, kind: .card)
					: nil
			)
		}
		.dropDestination(for: String.self) { items, _ in
			guard let name = items.first else { return false }
			model.moveDevice(name, onto: device.name)
			return true
		}
		.onAppear {
			brightness = Double(max(2, min(100, device.state?.brightness ?? 50)))
			warmth = Double(device.state?.warmth ?? 50)
		}
		.onChange(of: device.state?.brightness) { _, value in
			if !isEditingBrightness,
				model.liveBrightness[device.name] == nil,
				let value
			{
				brightness = Double(max(2, min(100, value)))
			}
		}
		.onChange(of: device.state?.warmth) { _, value in
			if !isEditingWarmth,
				model.liveWarmth[device.name] == nil,
				let value
			{
				warmth = Double(value)
			}
		}
		.onChange(of: model.liveBrightness[device.name]) { _, value in
			if let value {
				brightness = Double(max(2, min(100, value)))
			}
		}
		.onChange(of: model.liveWarmth[device.name]) { _, value in
			if let value {
				warmth = Double(value)
			}
		}
		.sheet(isPresented: $showRename) {
			RenameSheet(model: model, device: device)
		}
		.sheet(isPresented: $showPowerOn) {
			PowerOnSheet(model: model, device: device)
		}
		.sheet(isPresented: $showPulse) {
			PulseSheet(model: model, devices: [device])
		}
		.sheet(isPresented: $showSweep) {
			SweepSheet(model: model, devices: [device])
		}
		.sheet(isPresented: $showPulseGroup) {
			PulseSheet(model: model, devices: model.groupedDevices(for: device))
		}
		.sheet(isPresented: $showSweepGroup) {
			SweepSheet(model: model, devices: model.groupedDevices(for: device))
		}
	}

	private var header: some View {
		HStack(alignment: .top, spacing: 8) {
			VStack(alignment: .leading, spacing: 2) {
				Text(device.name)
					.font(.headline)
				Text(subtitle)
					.font(.caption)
					.foregroundStyle(.secondary)
			}

			Spacer()

			HStack(alignment: .center, spacing: 8) {
				wifiIcon
					.frame(width: 18, height: 18, alignment: .center)
				Toggle(
					"",
					isOn: Binding(
						get: { isEffectivelyOn },
						set: { value in
							Task { await model.setOn(device, on: value) }
						}
					)
				)
				.toggleStyle(.switch)
				.controlSize(.small)
				.labelsHidden()
				.overlay {
					if model.commandDown {
						linkTapOverlay(.power)
					}
				}
				.overlay(linkBorder(.power, cornerRadius: 10).padding(-3))
				.onHover { hovering in
					model.setHover(
						hovering
							? LinkTarget(device: device.name, kind: .power)
							: nil
					)
				}
			}
		}
	}

	private var brightnessRow: some View {
		HStack {
			Image(systemName: "sun.max")
				.foregroundStyle(.secondary)
				.frame(width: 16)
			TaperedBrightnessSlider(
				value: $brightness,
				range: 2...100,
				color: temperatureColor(warmth),
				isLightOn: isEffectivelyOn,
				onEditingChanged: { editing in
					isEditingBrightness = editing
					if !editing {
						model.queueBrightness(
							device,
							value: UInt8(max(2, min(100, brightness.rounded())))
						)
					}
				},
				onValueChanged: { value in
					model.queueBrightness(
						device,
						value: UInt8(max(2, min(100, value.rounded())))
					)
				}
			)
			.disabled(device.state == nil)
			.accessibilityLabel("Brightness")
			Text(brightnessLabel)
				.font(.caption.monospacedDigit())
				.frame(width: 38, alignment: .trailing)
		}
		.padding(.vertical, 2)
		.padding(.horizontal, 4)
		.overlay {
			if model.commandDown {
				linkTapOverlay(.brightness)
			}
		}
		.overlay(linkBorder(.brightness, cornerRadius: 8))
		.onHover { hovering in
			model.setHover(
				hovering
					? LinkTarget(device: device.name, kind: .brightness)
					: nil
			)
		}
	}

	private var warmthRow: some View {
		HStack {
			Image(systemName: "flame")
				.foregroundStyle(.secondary)
				.frame(width: 16)
			GradientSlider(
				value: $warmth,
				range: 0...100,
				gradient: Gradient(stops: [
					.init(color: temperatureColor(100), location: 0),
					.init(color: temperatureColor(40), location: 0.6),
					.init(color: temperatureColor(0), location: 1),
				]),
				thumbColor: temperatureColor(warmth),
				isReversed: true,
				onEditingChanged: { editing in
					isEditingWarmth = editing
					if !editing {
						model.queueWarmth(
							device,
							value: UInt8(max(0, min(100, warmth.rounded())))
						)
					}
				},
				onValueChanged: { value in
					model.queueWarmth(
						device,
						value: UInt8(max(0, min(100, value.rounded())))
					)
				}
			)
			.disabled(device.state == nil)
			.accessibilityLabel("Warmth")
			Text("\(Int(warmth.rounded()))%")
				.font(.caption.monospacedDigit())
				.frame(width: 38, alignment: .trailing)
		}
		.padding(.vertical, 2)
		.padding(.horizontal, 4)
		.overlay {
			if model.commandDown {
				linkTapOverlay(.warmth)
			}
		}
		.overlay(linkBorder(.warmth, cornerRadius: 8))
		.onHover { hovering in
			model.setHover(
				hovering
					? LinkTarget(device: device.name, kind: .warmth)
					: nil
			)
		}
	}

	private var footer: some View {
		HStack {
			if let state = device.state {
				Text("\(state.temperatureMired) mired · \(state.kelvin) K")
					.font(.caption2.monospacedDigit())
					.foregroundStyle(.secondary)
			}
			Spacer()
			Menu {
				Button("Rename…") { showRename = true }
				Button("Power On Behaviour…") { showPowerOn = true }
				Button("Pulse…") { showPulse = true }
				if model.isGrouped(device) {
					Button("Pulse Group…") { showPulseGroup = true }
				}
				Button("Sweep…") { showSweep = true }
				if model.isGrouped(device) {
					Button("Sweep Group…") { showSweepGroup = true }
				}
				Button("Identify") {
					Task { await model.identify(device) }
				}
			} label: {
				Image(systemName: "ellipsis.circle")
			}
			.menuStyle(.borderlessButton)
			.help("Light settings")
		}
	}

	private func linkTapOverlay(_ kind: LinkKind) -> some View {
		Color.clear
			.contentShape(Rectangle())
			.highPriorityGesture(
				TapGesture().onEnded {
					model.toggleLinkSelection(kind, device: device)
				}
			)
			.onHover { hovering in
				model.setHover(
					hovering
						? LinkTarget(device: device.name, kind: kind)
						: nil
				)
			}
	}

	private func linkBorder(_ kind: LinkKind, cornerRadius: CGFloat) -> some View {
		RoundedRectangle(cornerRadius: cornerRadius)
			.stroke(
				model.linkColor(
					for: LinkTarget(device: device.name, kind: kind)
				) ?? .clear,
				lineWidth: 2
			)
			.allowsHitTesting(false)
	}

	private var isEffectivelyOn: Bool {
		device.state?.on == true && (device.state?.brightness ?? 0) >= 2
	}

	private var brightnessLabel: String {
		if isEditingBrightness {
			return "\(Int(brightness.rounded()))%"
		}
		if device.state?.on == true && (device.state?.brightness ?? 0) < 2 {
			return "off"
		}
		return "\(Int(brightness.rounded()))%"
	}

	private var subtitle: String {
		let display = device.info?.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
		let product = device.info?.product.trimmingCharacters(in: .whitespacesAndNewlines)
		if let display, !display.isEmpty, display.caseInsensitiveCompare(device.name) != .orderedSame {
			if let product, !product.isEmpty {
				return "\(display) · \(product)"
			}
			return display
		}
		if let product, !product.isEmpty {
			return product
		}
		return device.host
	}

	private var wifiIcon: some View {
		Image(
			systemName: device.error != nil || device.info?.wifi == nil
				? "wifi.slash"
				: "wifi"
		)
		.font(.system(size: 14, weight: .regular))
		.foregroundStyle(wifiColor)
			.onHover { hovering in
				wifiHoverTask?.cancel()
				if hovering {
					wifiHoverTask = Task {
						try? await Task.sleep(for: .milliseconds(500))
						guard !Task.isCancelled else { return }
						wifiHelpPresented = true
					}
				} else {
					wifiHelpPresented = false
				}
			}
			.popover(isPresented: $wifiHelpPresented, arrowEdge: .bottom) {
				Text(wifiHelp)
					.font(.caption)
					.fixedSize(horizontal: false, vertical: true)
					.frame(maxWidth: 280, alignment: .leading)
					.padding(8)
			}
			.onDisappear {
				wifiHoverTask?.cancel()
				wifiHelpPresented = false
			}
	}

	private var wifiColor: Color {
		if device.error != nil {
			return .red
		}
		guard let wifi = device.info?.wifi else {
			return .secondary
		}
		switch wifi.quality {
		case "excellent", "good": return .green
		case "fair": return .orange
		default: return .red
		}
	}

	private var wifiHelp: String {
		if let error = device.error {
			return "\(error) · Address \(device.host). Power cycle the light, wait for it to reconnect, then refresh."
		}
		guard let wifi = device.info?.wifi else {
			return "Loading network details · Address \(device.host)"
		}
		let frequency = wifi.frequencyMhz < 3000 ? "2.4 GHz" : "5 GHz"
		return "\(wifi.ssid) · \(frequency) · \(wifi.rssi) dBm (\(wifi.quality)) · Address \(device.host)"
	}

	private func temperatureComponents(
		_ warmth: Double
	) -> (r: Double, g: Double, b: Double) {
		let t = min(1, max(0, warmth / 100))
		let cool = (r: 0.55, g: 0.77, b: 1.0)
		let white = (r: 1.0, g: 1.0, b: 1.0)
		let warm = (r: 1.0, g: 0.62, b: 0.29)
		let (from, to, factor) = t < 0.4
			? (cool, white, t / 0.4)
			: (white, warm, (t - 0.4) / 0.6)
		return (
			r: from.r + (to.r - from.r) * factor,
			g: from.g + (to.g - from.g) * factor,
			b: from.b + (to.b - from.b) * factor
		)
	}

	private func temperatureColor(_ warmth: Double) -> Color {
		let color = temperatureComponents(warmth)
		return Color(red: color.r, green: color.g, blue: color.b)
	}
}
