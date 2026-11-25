import SwiftUI

struct GradientSlider: View {
	@Binding var value: Double
	let range: ClosedRange<Double>
	let gradient: Gradient
	let thumbColor: Color
	var onEditingChanged: (Bool) -> Void = { _ in }

	@State private var isDragging = false

	private var fraction: Double {
		let span = range.upperBound - range.lowerBound
		guard span > 0 else { return 0 }
		return min(1, max(0, (value - range.lowerBound) / span))
	}

	var body: some View {
		GeometryReader { geometry in
			let width = geometry.size.width
			let thumbSize: CGFloat = 16
			let trackHeight: CGFloat = 6
			let travel = max(1, width - thumbSize)
			let x = fraction * travel

			ZStack(alignment: .leading) {
				Capsule()
					.fill(
						LinearGradient(
							gradient: gradient,
							startPoint: .leading,
							endPoint: .trailing
						)
					)
					.frame(height: trackHeight)
				Circle()
					.fill(thumbColor)
					.frame(width: thumbSize, height: thumbSize)
					.overlay(
						Circle()
							.stroke(.white.opacity(0.7), lineWidth: 1)
					)
					.shadow(color: .black.opacity(0.25), radius: 1, x: 0, y: 1)
					.offset(x: x)
			}
			.frame(maxHeight: .infinity)
			.contentShape(Rectangle())
			.gesture(
				DragGesture(minimumDistance: 0)
					.onChanged { gesture in
						if !isDragging {
							isDragging = true
							onEditingChanged(true)
						}
						let location = gesture.location.x - thumbSize / 2
						let newFraction = min(1, max(0, location / travel))
						value = range.lowerBound
							+ newFraction * (range.upperBound - range.lowerBound)
					}
					.onEnded { _ in
						isDragging = false
						onEditingChanged(false)
					}
			)
		}
		.frame(height: 20)
		.accessibilityElement(children: .ignore)
		.accessibilityAdjustableAction { direction in
			let step = (range.upperBound - range.lowerBound) / 100
			switch direction {
			case .increment:
				value = min(range.upperBound, value + step)
			case .decrement:
				value = max(range.lowerBound, value - step)
			@unknown default:
				break
			}
			onEditingChanged(false)
		}
	}
}
