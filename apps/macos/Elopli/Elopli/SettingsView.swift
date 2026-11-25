import SwiftUI

struct RenameSheet: View {
	@ObservedObject var model: AppModel
	let device: LightDevice
	@Environment(\.dismiss) private var dismiss
	@State private var name: String

	init(model: AppModel, device: LightDevice) {
		self.model = model
		self.device = device
		let current = device.info?.displayName ?? ""
		_name = State(initialValue: current.isEmpty ? device.name : current)
	}

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text("Rename Light")
				.font(.title3.bold())
			Text("This changes the name stored on the light.")
				.font(.caption)
				.foregroundStyle(.secondary)
			TextField("Name", text: $name)
				.textFieldStyle(.roundedBorder)
			Text("The name can have 1 to 63 bytes.")
				.font(.caption2)
				.foregroundStyle(.secondary)
			HStack {
				Spacer()
				Button("Cancel") { dismiss() }
					.keyboardShortcut(.cancelAction)
				Button("Save") {
					Task {
						await model.rename(device, to: name)
						dismiss()
					}
				}
				.keyboardShortcut(.defaultAction)
				.disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
			}
		}
		.padding(20)
		.frame(width: 380)
	}
}

struct PowerOnSheet: View {
	@ObservedObject var model: AppModel
	let device: LightDevice
	@Environment(\.dismiss) private var dismiss
	@State private var behavior: Int
	@State private var brightness: Double
	@State private var warmth: Double

	init(model: AppModel, device: LightDevice) {
		self.model = model
		self.device = device
		_behavior = State(initialValue: Int(device.powerOn?.behavior ?? 1))
		_brightness = State(initialValue: Double(max(2, device.powerOn?.brightness ?? 20)))
		_warmth = State(initialValue: Double(device.powerOn?.warmth ?? 35))
	}

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text("Power On Behaviour")
				.font(.title3.bold())
			Text("Choose what the light does when it receives power.")
				.font(.caption)
				.foregroundStyle(.secondary)
			Picker("When the light powers on", selection: $behavior) {
				Text("Restore Last Used Setting").tag(1)
				Text("Restore Defaults").tag(2)
			}
			.pickerStyle(.radioGroup)

			if behavior == 2 {
				Divider()
				HStack {
					Image(systemName: "sun.max")
						.foregroundStyle(.secondary)
						.frame(width: 16)
					Slider(value: $brightness, in: 2...100)
					Text("\(Int(brightness.rounded()))%")
						.font(.caption.monospacedDigit())
						.frame(width: 38, alignment: .trailing)
				}
				HStack {
					Image(systemName: "thermometer.medium")
						.foregroundStyle(.secondary)
						.frame(width: 16)
					Slider(value: $warmth, in: 0...100)
					Text("\(Int(warmth.rounded()))%")
						.font(.caption.monospacedDigit())
						.frame(width: 38, alignment: .trailing)
				}
			}

			HStack {
				Spacer()
				Button("Cancel") { dismiss() }
					.keyboardShortcut(.cancelAction)
				Button("Save") {
					let defaults = behavior == 2
					let mired = UInt16((143.0 + warmth / 100.0 * 201.0).rounded())
					Task {
						await model.setPowerOn(
							device,
							behavior: UInt8(behavior),
							brightness: defaults
								? UInt8(min(100, max(2, brightness.rounded())))
								: nil,
							temperatureMired: defaults ? mired : nil
						)
						dismiss()
					}
				}
				.disabled(device.powerOn == nil)
				.keyboardShortcut(.defaultAction)
			}
		}
		.padding(20)
		.frame(width: 420)
	}
}
