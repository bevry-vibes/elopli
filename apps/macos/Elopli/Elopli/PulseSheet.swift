import SwiftUI

struct PulseSheet: View {
	@ObservedObject var model: AppModel
	let devices: [LightDevice]
	@Environment(\.dismiss) private var dismiss

	@State private var count = 3
	@State private var durationText = "1.0s"
	@State private var value = 100.0
	@State private var low = 0.0
	@State private var isPulsing = false
	@State private var stopRequested = false
	@State private var result: String?
	@State private var errorText: String?

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text(devices.count > 1 ? "Pulse Group" : "Pulse \(devices.first?.name ?? "")")
				.font(.title3.bold())
			if devices.count > 1 {
				Text(devices.map(\.name).joined(separator: ", "))
					.font(.caption)
					.foregroundStyle(.secondary)
			}
			Text("Each pulse is one peak-to-minimum fade. Count 3 runs peak → minimum three times, then restores the original values. An off light starts and ends off: count 1 runs off → peak → off.")
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
			.disabled(isPulsing)

			HStack {
				Text("Duration")
				Spacer()
				TextField("1.0s", text: $durationText)
					.textFieldStyle(.roundedBorder)
					.frame(width: 86)
					.multilineTextAlignment(.trailing)
			}
			.disabled(isPulsing)

			Text("Use seconds by default, or add a unit: 1.0s, 500ms, 1.5, 10m, 1h. The duration is split by distance across the initial rise, each fade, and the final restore.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			VStack(alignment: .leading, spacing: 6) {
				HStack {
					Text("Low")
					Spacer()
					Text("Peak")
				}
				.font(.caption)
				.foregroundStyle(.secondary)
				PulseRangeSlider(
					low: $low,
					peak: $value,
					color: Color(red: 1.0, green: 0.76, blue: 0.28)
				)
				HStack {
					Text(lowLabel)
					Spacer()
					Text(peakLabel)
				}
				.font(.caption.monospacedDigit())
			}
			.disabled(isPulsing)

			Text("Low defaults to 0% (off). Set it to 2% or higher to keep the light on during the pulse. The light fades smoothly through 0 and 1.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			Text("The light uses more requests while it pulses. Refresh polling for \(devices.count == 1 ? "this light" : "these lights") pauses until the pulse ends.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			if let errorText {
				Label(errorText, systemImage: "exclamationmark.triangle.fill")
					.font(.caption)
					.foregroundStyle(.red)
					.fixedSize(horizontal: false, vertical: true)
			}

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
						cancelAll()
					}
					dismiss()
				}
				.keyboardShortcut(.cancelAction)

				if isPulsing {
					Button("Stop") {
						stopRequested = true
						result = "Stopping…"
						cancelAll()
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
				cancelAll()
			}
		}
	}

	private func cancelAll() {
		for device in devices {
			model.cancelPulse(device)
		}
	}

	private var lowLabel: String {
		low < 2 ? "Off" : "\(Int(low.rounded()))%"
	}

	private var peakLabel: String {
		"\(Int(value.rounded()))%"
	}

	private func startPulse() {
		guard !isPulsing, !devices.isEmpty else { return }
		stopRequested = false
		result = nil
		errorText = nil
		let count = min(20, max(1, count))
		let durationMs: UInt32
		do {
			durationMs = try parseDuration(input: durationText)
		} catch {
			errorText = "Invalid duration. Use 1.0s, 500ms, 10m, or 1h."
			return
		}
		let minimum = UInt32((count * 2 + 1) * 100)
		guard durationMs >= minimum else {
			errorText = "Duration must be at least \(minimum) ms for \(count) pulses at the 10 Hz frame limit."
			return
		}
		guard durationMs <= 3_600_000 else {
			errorText = "Pulse duration must be one hour or less."
			return
		}
		let peak = min(100, max(2, Int(value.rounded())))
		let lowValue = min(peak, max(0, Int(low.rounded())))
		isPulsing = true
		Task {
			let outcomes = await model.pulseGroup(
				devices,
				count: count,
				durationMs: Int(durationMs),
				value: peak,
				low: lowValue
			)
			isPulsing = false
			let completed = outcomes.filter { $0?.completed == true }.count
			if completed == devices.count {
				result = "Pulse completed."
			} else if stopRequested {
				result = "Pulse stopped."
			} else if outcomes.contains(where: { $0 == nil }) {
				errorText = "Pulse failed for \(devices.count - completed) of \(devices.count) lights."
			} else if devices.count == 1,
				let outcome = outcomes.first ?? nil,
				outcome.cancellation == "external-change"
			{
				result = "Pulse stopped after an external change."
			} else {
				result = "Pulse stopped on \(devices.count - completed) of \(devices.count) lights."
			}
		}
	}
}
