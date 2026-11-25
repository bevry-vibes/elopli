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

	init(model: AppModel, device: LightDevice) {
		self.model = model
		self.device = device
		_brightness = State(initialValue: Double(device.state?.brightness ?? 0))
		_warmth = State(initialValue: Double(device.state?.warmth ?? 0))
	}

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			HStack(alignment: .top) {
				Circle()
					.fill(statusColor)
					.frame(width: 8, height: 8)
					.padding(.top, 6)
				VStack(alignment: .leading, spacing: 2) {
					Text(device.name)
						.font(.headline)
					Text(device.info?.product ?? device.host)
						.font(.caption)
						.foregroundStyle(.secondary)
				}
				Spacer()
				wifiIcon
				Toggle(
					"",
					isOn: Binding(
						get: { device.state?.on ?? false },
						set: { value in
							Task { await model.setOn(device, on: value) }
						}
					)
				)
				.toggleStyle(.switch)
				.labelsHidden()
			}

			if let error = device.error {
				Text(error)
					.font(.caption)
					.foregroundStyle(.red)
					.lineLimit(2)
			}

			VStack(alignment: .leading, spacing: 6) {
				HStack {
					Image(systemName: "sun.max")
						.foregroundStyle(.secondary)
						.frame(width: 16)
					GradientSlider(
						value: $brightness,
						range: 0...100,
						gradient: Gradient(colors: [
							Color(white: 0.08),
							temperatureColor(warmth),
						]),
						thumbColor: brightnessColor(brightness),
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
					Text("\(Int(brightness.rounded()))%")
						.font(.caption.monospacedDigit())
						.frame(width: 38, alignment: .trailing)
				}
				HStack {
					Image(systemName: "thermometer.medium")
						.foregroundStyle(.secondary)
						.frame(width: 16)
					GradientSlider(
						value: $warmth,
						range: 0...100,
						gradient: Gradient(colors: [
							temperatureColor(0),
							temperatureColor(50),
							temperatureColor(100),
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
			}

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
		.padding(14)
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
		.overlay(
			RoundedRectangle(cornerRadius: 18)
				.stroke(.quaternary, lineWidth: 1)
		)
		.onAppear {
			brightness = Double(device.state?.brightness ?? 0)
			warmth = Double(device.state?.warmth ?? 0)
		}
		.onChange(of: device.state?.brightness) { _, value in
			if !isEditingBrightness, let value {
				brightness = Double(value)
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
	}

	private var statusColor: Color {
		if device.error != nil {
			return .red
		}
		return device.state?.on == true ? .green : .secondary
	}

	private var wifiIcon: some View {
		Image(systemName: device.info?.wifi == nil ? "wifi.slash" : "wifi")
			.foregroundStyle(wifiColor)
			.help(wifiHelp)
			.padding(.top, 2)
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
		let neutral = (r: 1.0, g: 0.93, b: 0.82)
		let warm = (r: 1.0, g: 0.62, b: 0.29)
		let (from, to, factor) = t < 0.5
			? (cool, neutral, t * 2)
			: (neutral, warm, (t - 0.5) * 2)
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

	private func brightnessColor(_ brightness: Double) -> Color {
		let t = min(1, max(0, brightness / 100))
		let target = temperatureComponents(warmth)
		let base = 0.06
		return Color(
			red: base + (target.r - base) * t,
			green: base + (target.g - base) * t,
			blue: base + (target.b - base) * t
		)
	}
}
