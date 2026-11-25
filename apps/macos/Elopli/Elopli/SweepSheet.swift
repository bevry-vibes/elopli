import SwiftUI

struct SweepSheet: View {
	@ObservedObject var model: AppModel
	let device: LightDevice
	@Environment(\.dismiss) private var dismiss

	@State private var count = 1
	@State private var durationMs = 3000
	@State private var ease = PulseEaseOption.linear
	@State private var brightness = 0.0
	@State private var isSweeping = false
	@State private var stopRequested = false
	@State private var result: String?

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text("Sweep \(device.name)")
				.font(.title3.bold())
			Text("Each sweep is one warm-to-cool fade. Count 3 runs warm → cool three times, then restores the original values.")
				.font(.caption)
				.foregroundStyle(.secondary)

			HStack {
				Text("Count")
				Spacer()
				Stepper(value: $count, in: 1...20) {
					Text("\(count)")
						.monospacedDigit()
				}
			}
			.disabled(isSweeping)

			HStack {
				Text("Duration")
				Spacer()
				TextField(
					"",
					value: $durationMs,
					format: .number
				)
				.textFieldStyle(.roundedBorder)
				.frame(width: 76)
				.multilineTextAlignment(.trailing)
				Text("ms")
					.foregroundStyle(.secondary)
			}
			.disabled(isSweeping)

			Text("Duration covers the entire sweep animation.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			HStack {
				Text("Ease")
				Spacer()
				Picker("", selection: $ease) {
					ForEach(PulseEaseOption.allCases) { option in
						Text(option.label).tag(option)
					}
				}
				.labelsHidden()
				.frame(width: 150)
			}
			.disabled(isSweeping)

			HStack {
				Text("Brightness")
				Slider(value: $brightness, in: 0...100)
				Text("\(Int(brightness.rounded()))%")
					.font(.caption.monospacedDigit())
					.frame(width: 38, alignment: .trailing)
			}
			.disabled(isSweeping)

			Text("0% keeps the current brightness. Any other value holds that brightness for the whole sweep.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			Text("Refresh polling for this light pauses until the sweep ends.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			if let result {
				Text(result)
					.font(.caption)
					.foregroundStyle(.secondary)
			}

			HStack {
				if isSweeping {
					ProgressView()
						.controlSize(.small)
				}
				Spacer()
				Button("Close") {
					if isSweeping {
						model.cancelPulse(device)
					}
					dismiss()
				}
				.keyboardShortcut(.cancelAction)

				if isSweeping {
					Button("Stop") {
						stopRequested = true
						result = "Stopping…"
						model.cancelPulse(device)
					}
				} else {
					Button("Sweep") {
						startSweep()
					}
					.keyboardShortcut(.defaultAction)
				}
			}
		}
		.padding(20)
		.frame(width: 420)
		.onDisappear {
			if isSweeping {
				model.cancelPulse(device)
			}
		}
	}

	private func startSweep() {
		guard !isSweeping else { return }
		isSweeping = true
		stopRequested = false
		result = nil
		let count = min(20, max(1, count))
		let durationMs = min(60_000, max(100, durationMs))
		let brightness = min(100, max(0, Int(brightness.rounded())))
		Task {
			let outcome = await model.sweep(
				device,
				count: count,
				durationMs: durationMs,
				ease: ease.value,
				brightness: brightness
			)
			isSweeping = false
			guard let outcome else {
				result = "Sweep failed."
				return
			}
			if outcome.completed {
				result = "Sweep completed."
			} else if stopRequested {
				result = "Sweep stopped."
			} else if outcome.cancellation == "external-change" {
				result = "Sweep stopped after an external change."
			} else {
				result = "Sweep stopped by another action."
			}
		}
	}
}
