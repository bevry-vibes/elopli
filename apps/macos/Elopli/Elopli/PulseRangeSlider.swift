import SwiftUI

/// A two-thumb brightness range from 0 to 100 percent.
/// The low thumb may sit at 0 or 1, which turns the light off during a pulse.
struct PulseRangeSlider: View {
    @Binding var low: Double
    @Binding var peak: Double
    let color: Color

    @State private var dragging: Thumb?

    private let thumbSize: CGFloat = 18
    private let trackHeight: CGFloat = 8

    private enum Thumb {
        case low
        case peak
    }

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            let travel = max(1, width - thumbSize)
            let lowX = fraction(low) * travel
            let peakX = fraction(peak) * travel

            ZStack(alignment: .leading) {
                Capsule()
                    .fill(.quaternary)
                    .frame(height: trackHeight)

                Capsule()
                    .fill(color)
                    .frame(width: max(0, peakX - lowX), height: trackHeight)
                    .offset(x: lowX + thumbSize / 2)

                Circle()
                    .fill(.background)
                    .overlay {
                        Circle()
                            .stroke(color, lineWidth: 2)
                    }
                    .frame(width: thumbSize, height: thumbSize)
                    .offset(x: lowX)

                Circle()
                    .fill(color)
                    .overlay {
                        Circle()
                            .stroke(.black.opacity(0.22), lineWidth: 0.75)
                    }
                    .frame(width: thumbSize, height: thumbSize)
                    .offset(x: peakX)
            }
            .frame(height: 24)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { gesture in
                        if dragging == nil {
                            dragging = nearestThumb(
                                to: gesture.startLocation.x,
                                lowX: lowX,
                                peakX: peakX
                            )
                        }
                        let location = min(
                            travel,
                            max(0, gesture.location.x - thumbSize / 2)
                        )
                        let value = (location / travel * 100).rounded()
                        switch dragging {
                        case .low:
                            low = min(value, peak)
                        case .peak:
                            peak = max(max(2, value), low)
                        case nil:
                            break
                        }
                    }
                    .onEnded { _ in
                        dragging = nil
                    }
            )
        }
        .frame(height: 24)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Pulse brightness range")
        .accessibilityValue("\(lowLabel) to \(peakLabel)")
    }

    private var lowLabel: String {
        low < 2 ? "Off" : "\(Int(low.rounded())) percent"
    }

    private var peakLabel: String {
        "\(Int(peak.rounded())) percent"
    }

    private func fraction(_ value: Double) -> Double {
        min(1, max(0, value / 100))
    }

    private func nearestThumb(to x: CGFloat, lowX: CGFloat, peakX: CGFloat) -> Thumb {
        let lowCenter = lowX + thumbSize / 2
        let peakCenter = peakX + thumbSize / 2
        return abs(x - lowCenter) <= abs(x - peakCenter) ? .low : .peak
    }
}
