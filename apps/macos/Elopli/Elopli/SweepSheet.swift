import SwiftUI

struct SweepSheet: View {
	@ObservedObject var model: AppModel
	let devices: [LightDevice]
	@Environment(\.dismiss) private var dismiss

	@State private var count = 1
	@State private var durationText = "1.0s"
	@State private var brightness = 0.0
	@State private var isSweeping = false
	@State private var stopRequested = false
	@State private var result: String?
	@State private var errorText: String?

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text(devices.count > 1 ? "Sweep Group" : "Sweep \(devices.first?.name ?? "")")
				.font(.title3.bold())
			if devices.count > 1 {
				Text(devices.map(\.name).joined(separator: ", "))
					.font(.caption)
					.foregroundStyle(.secondary)
			}
			Text("Each sweep is one warm-to-cool fade, left to right. Count 3 runs warm → cool three times, then restores the original values.")
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
				TextField("1.0s", text: $durationText)
					.textFieldStyle(.roundedBorder)
					.frame(width: 86)
					.multilineTextAlignment(.trailing)
			}
			.disabled(isSweeping)

			Text("Use seconds by default, or add a unit: 1.0s, 500ms, 1.5, 10m, 1h. Duration covers the entire sweep.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			HStack {
				Text("Brightness")
				Slider(value: $brightness, in: 0...100)
				Text(brightnessValue < 2 ? "N/A" : "\(brightnessValue)%")
					.font(.caption.monospacedDigit())
					.frame(width: 38, alignment: .trailing)
			}
			.disabled(isSweeping)

			Text("N/A keeps the current brightness. Values from 2% hold that brightness for the whole sweep.")
				.font(.caption2)
				.foregroundStyle(.secondary)

			Text("Refresh polling for \(devices.count == 1 ? "this light" : "these lights") pauses until the sweep ends.")
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

	private var brightnessValue: Int {
		let value = min(100, max(0, Int(brightness.rounded())))
		return value >= 2 ? value : 0
	}

	private func startSweep() {
		guard !isSweeping, !devices.isEmpty else { return }
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
			errorText = "Duration must be at least \(minimum) ms for \(count) sweeps at the 10 Hz frame limit."
			return
		}
		guard durationMs <= 3_600_000 else {
			errorText = "Sweep duration must be one hour or less."
			return
		}
		isSweeping = true
		Task {
			let outcomes = await model.sweepGroup(
				devices,
				count: count,
				durationMs: Int(durationMs),
				brightness: brightnessValue
			)
			isSweeping = false
			let completed = outcomes.filter { $0?.completed == true }.count
			if completed == devices.count {
				result = "Sweep completed."
			} else if stopRequested {
				result = "Sweep stopped."
			} else if outcomes.contains(where: { $0 == nil }) {
				errorText = "Sweep failed for \(devices.count - completed) of \(devices.count) lights."
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
