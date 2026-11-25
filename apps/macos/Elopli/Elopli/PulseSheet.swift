import SwiftUI

private enum PulseEaseOption: String, CaseIterable, Identifiable {
	case linear
	case easeIn
	case easeOut
	case easeInOut

	var id: String { rawValue }

	var label: String {
		switch self {
		case .linear: return "Linear"
		case .easeIn: return "Ease In"
		case .easeOut: return "Ease Out"
		case .easeInOut: return "Ease In Out"
		}
	}

	var value: FfiPulseEase {
		switch self {
		case .linear: return .linear
		case .easeIn: return .easeIn
		case .easeOut: return .easeOut
		case .easeInOut: return .easeInOut
		}
	}
}

private enum PulseTargetOption: String, CaseIterable, Identifiable {
	case brightness
	case temperature
	case both

	var id: String { rawValue }

	var label: String {
		switch self {
		case .brightness: return "Brightness"
		case .temperature: return "Temperature"
		case .both: return "Both"
		}
	}

	var value: FfiPulseTarget {
		switch self {
		case .brightness: return .brightness
		case .temperature: return .temperature
		case .both: return .both
		}
	}
}

struct PulseSheet: View {
	@ObservedObject var model: AppModel
	let device: LightDevice
	@Environment(\.dismiss) private var dismiss

	@State private var count = 3
	@State private var durationMs = 600
	@State private var ease = PulseEaseOption.easeInOut
	@State private var target = PulseTargetOption.both
	@State private var value = 100.0
	@State private var isPulsing = false
	@State private var stopRequested = false
	@State private var result: String?

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text("Pulse \(device.name)")
				.font(.title3.bold())
			Text("Each pulse is one peak-to-0 fade. Count 3 runs peak → 0 three times, then restores the original values.")
				.font(.caption)
				.foregroundStyle(.secondary)

			HStack {
				Text("Count")
				Spacer()
				Stepper(
					"",
					value: $count,
					in: 1...20
				)
				.labelsHidden()
				.frame(width: 110, alignment: .trailing)
			}
			.disabled(isPulsing)

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
			.disabled(isPulsing)

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
			.disabled(isPulsing)

			HStack {
				Text("Target")
				Spacer()
				Picker("", selection: $target) {
					ForEach(PulseTargetOption.allCases) { option in
						Text(option.label).tag(option)
					}
				}
				.labelsHidden()
				.frame(width: 220)
			}
			.disabled(isPulsing)

			HStack {
				Text("Peak")
				Slider(value: $value, in: 2...100)
				Text("\(Int(value.rounded()))%")
					.font(.caption.monospacedDigit())
					.frame(width: 38, alignment: .trailing)
			}
			.disabled(isPulsing)

			Text("The light uses more requests while it pulses. Refresh polling for this light pauses until the pulse ends.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			if let result {
				Text(result)
					.font(.caption)
					.foregroundStyle(.secondary)
			}

			HStack {
				if isPulsing {
					ProgressView()
						.controlSize(.small)
				}
				Spacer()
				Button("Close") {
					if isPulsing {
						model.cancelPulse(device)
					}
					dismiss()
				}
				.keyboardShortcut(.cancelAction)

				if isPulsing {
					Button("Stop") {
						stopRequested = true
						result = "Stopping…"
						model.cancelPulse(device)
					}
				} else {
					Button("Pulse") {
						startPulse()
					}
					.keyboardShortcut(.defaultAction)
				}
			}
		}
		.padding(20)
		.frame(width: 420)
		.onDisappear {
			if isPulsing {
				model.cancelPulse(device)
			}
		}
	}

	private func startPulse() {
		guard !isPulsing else { return }
		isPulsing = true
		stopRequested = false
		result = nil
		let count = min(20, max(1, count))
		let durationMs = min(10_000, max(50, durationMs))
		let value = Int(value.rounded())
		Task {
			let outcome = await model.pulse(
				device,
				count: count,
				durationMs: durationMs,
				ease: ease.value,
				target: target.value,
				value: value
			)
			isPulsing = false
			guard let outcome else {
				result = "Pulse failed."
				return
			}
			if outcome.completed {
				result = "Pulse completed."
			} else if stopRequested {
				result = "Pulse stopped."
			} else if outcome.cancellation == "external-change" {
				result = "Pulse stopped after an external change."
			} else {
				result = "Pulse stopped by another action."
			}
		}
	}
}
