import SwiftUI

struct TaperedCapsule: Shape {
	let leftHeight: CGFloat
	let rightHeight: CGFloat

	func path(in rect: CGRect) -> Path {
		let left = min(leftHeight, rect.height)
		let right = min(rightHeight, rect.height)
		let leftRadius = left / 2
		let rightRadius = right / 2
		let centerY = rect.midY
		let leftCenterX = rect.minX + leftRadius
		let rightCenterX = rect.maxX - rightRadius

		var path = Path()
		path.move(to: CGPoint(x: leftCenterX, y: centerY - leftRadius))
		path.addLine(to: CGPoint(x: rightCenterX, y: centerY - rightRadius))
		path.addArc(
			center: CGPoint(x: rightCenterX, y: centerY),
			radius: rightRadius,
			startAngle: .degrees(-90),
			endAngle: .degrees(90),
			clockwise: false
		)
		path.addLine(to: CGPoint(x: leftCenterX, y: centerY + leftRadius))
		path.addArc(
			center: CGPoint(x: leftCenterX, y: centerY),
			radius: leftRadius,
			startAngle: .degrees(90),
			endAngle: .degrees(270),
			clockwise: false
		)
		path.closeSubpath()
		return path
	}
}

struct TaperedBrightnessSlider: View {
	@Binding var value: Double
	let range: ClosedRange<Double>
	let color: Color
	let isLightOn: Bool
	var onEditingChanged: (Bool) -> Void = { _ in }
	var onValueChanged: (Double) -> Void = { _ in }

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
			let travel = max(1, width - thumbSize)
			let x = fraction * travel
			let glowSize = 6 + fraction * 30
			let glowDiameter = thumbSize + glowSize

			ZStack(alignment: .leading) {
				if isLightOn || isDragging {
					Circle()
						.fill(color.opacity(0.55))
						.frame(width: glowDiameter, height: glowDiameter)
						.blur(radius: 7)
						.offset(x: x - glowSize / 2)
						.allowsHitTesting(false)
				}

				TaperedCapsule(leftHeight: 3, rightHeight: 12)
					.fill(color)
					.overlay {
						TaperedCapsule(leftHeight: 3, rightHeight: 12)
							.stroke(.primary.opacity(0.32), lineWidth: 1)
					}
					.frame(height: 14)

				Circle()
					.fill(color)
					.frame(width: thumbSize, height: thumbSize)
					.overlay(
						Circle()
							.stroke(.black.opacity(0.22), lineWidth: 0.75)
					)
					.offset(x: x)
			}
			.frame(height: 24)
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
						let newValue = range.lowerBound
							+ newFraction * (range.upperBound - range.lowerBound)
						value = newValue
						onValueChanged(newValue)
					}
					.onEnded { _ in
						isDragging = false
						onEditingChanged(false)
					}
			)
		}
		.frame(height: 24)
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
