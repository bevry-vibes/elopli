import SwiftUI

struct SweepSheet: View {
	@ObservedObject var model: AppModel
	let devices: [LightDevice]
	@Environment(\.dismiss) private var dismiss

	@State private var count = 1
	@State private var durationSeconds = 1.0
	@State private var brightness = 0.0
	@State private var isSweeping = false
	@State private var stopRequested = false
	@State private var result: String?

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text(devices.count > 1 ? "Sweep Group" : "Sweep \(devices.first?.name ?? "")")
				.font(.title3.bold())
			if devices.count > 1 {
				Text(devices.map(\.name).joined(separator: ", "))
					.font(.caption)
					.foregroundStyle(.secondary)
			}
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
					value: $durationSeconds,
					format: .number.precision(.fractionLength(0...2))
				)
				.textFieldStyle(.roundedBorder)
				.frame(width: 76)
				.multilineTextAlignment(.trailing)
				Text("s")
					.foregroundStyle(.secondary)
			}
			.disabled(isSweeping)

			Text("Duration covers the entire sweep animation.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			HStack {
				Text("Brightness")
				Slider(value: $brightness, in: 0...100)
				Text(brightness < 1 ? "N/A" : "\(Int(brightness.rounded()))%")
					.font(.caption.monospacedDigit())
					.frame(width: 38, alignment: .trailing)
			}
			.disabled(isSweeping)

			Text("N/A keeps the current brightness. Any other value holds that brightness for the whole sweep.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			Text("Refresh polling for \(devices.count == 1 ? "this light" : "these lights") pauses until the sweep ends.")
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
						cancelAll()
					}
					dismiss()
				}
				.keyboardShortcut(.cancelAction)

				if isSweeping {
					Button("Stop") {
						stopRequested = true
						result = "Stopping…"
						cancelAll()
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
				cancelAll()
			}
		}
	}

	private func cancelAll() {
		for device in devices {
			model.cancelPulse(device)
		}
	}

	private func startSweep() {
		guard !isSweeping, !devices.isEmpty else { return }
		isSweeping = true
		stopRequested = false
		result = nil
		let count = min(20, max(1, count))
		let durationMs = min(60_000, max(100, Int((durationSeconds * 1000).rounded())))
		let brightness = min(100, max(0, Int(brightness.rounded())))
		Task {
			let outcomes = await model.sweepGroup(
				devices,
				count: count,
				durationMs: durationMs,
				brightness: brightness
			)
			isSweeping = false
			let completed = outcomes.filter { $0?.completed == true }.count
			if completed == devices.count {
				result = "Sweep completed."
			} else if stopRequested {
				result = "Sweep stopped."
			} else if outcomes.contains(where: { $0 == nil }) {
				result = "Sweep failed for \(devices.count - completed) of \(devices.count) lights."
			} else if devices.count == 1,
				let outcome = outcomes.first ?? nil,
				outcome.cancellation == "external-change"
			{
				result = "Sweep stopped after an external change."
			} else {
				result = "Sweep stopped on \(devices.count - completed) of \(devices.count) lights."
			}
		}
	}
}
