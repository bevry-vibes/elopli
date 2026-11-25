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
			if !isEditingBrightness, let value {
				brightness = Double(max(2, min(100, value)))
			}
		}
		.onChange(of: device.state?.warmth) { _, value in
			if !isEditingWarmth, let value {
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
			PulseSheet(model: model, device: device)
		}
		.sheet(isPresented: $showSweep) {
			SweepSheet(model: model, device: device)
		}
	}

	private var header: some View {
		HStack(alignment: .top, spacing: 8) {
			VStack(alignment: .leading, spacing: 2) {
				Text(device.name)
					.font(.headline)
				Text(device.info?.product ?? device.host)
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
						Task {
							await model.setBrightness(
								device,
								value: UInt8(brightness.rounded())
							)
						}
					}
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
			Image(systemName: "thermometer.medium")
				.foregroundStyle(.secondary)
				.frame(width: 16)
			GradientSlider(
				value: $warmth,
				range: 0...100,
				gradient: Gradient(stops: [
					.init(color: temperatureColor(0), location: 0),
					.init(color: temperatureColor(40), location: 0.4),
					.init(color: temperatureColor(100), location: 1),
				]),
				thumbColor: temperatureColor(warmth),
				onEditingChanged: { editing in
					isEditingWarmth = editing
					if !editing {
						Task {
							await model.setWarmth(
								device,
								value: UInt8(warmth.rounded())
							)
						}
					}
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
				Button("Sweep…") { showSweep = true }
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

	private var wifiIcon: some View {
		Image(systemName: device.info?.wifi == nil ? "wifi.slash" : "wifi")
			.font(.system(size: 14, weight: .regular))
			.foregroundStyle(wifiColor)
			.help(wifiHelp)
	}

	private var wifiColor: Color {
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
		guard let wifi = device.info?.wifi else {
			return device.error ?? "Offline"
		}
		let frequency = wifi.frequencyMhz == 2400 ? "2.4 GHz" : "5 GHz"
		return "\(wifi.ssid) · \(frequency) · \(wifi.rssi) dBm (\(wifi.quality))"
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
