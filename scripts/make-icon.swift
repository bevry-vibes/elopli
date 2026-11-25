#!/usr/bin/env swift
import AppKit

let output = CommandLine.arguments.dropFirst().first ?? "icon.png"
let size: CGFloat = 1024
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()

let bounds = NSRect(x: 0, y: 0, width: size, height: size)
let iconRect = bounds.insetBy(dx: 36, dy: 36)
let iconPath = NSBezierPath(
	roundedRect: iconRect,
	xRadius: 224,
	yRadius: 224
)
iconPath.addClip()

let background = NSGradient(colors: [
	NSColor(calibratedRed: 0.10, green: 0.12, blue: 0.32, alpha: 1),
	NSColor(calibratedRed: 0.28, green: 0.20, blue: 0.48, alpha: 1),
	NSColor(calibratedRed: 1.00, green: 0.56, blue: 0.18, alpha: 1),
])!
background.draw(in: iconRect, angle: 42)

let glowRect = NSRect(x: 220, y: 220, width: 584, height: 584)
let glow = NSGradient(colors: [
	NSColor(calibratedWhite: 1.0, alpha: 0.32),
	NSColor(calibratedWhite: 1.0, alpha: 0.0),
])!
glow.draw(in: NSBezierPath(ovalIn: glowRect), relativeCenterPosition: .zero)

if let symbol = NSImage(
	systemSymbolName: "lightbulb.fill",
	accessibilityDescription: nil
) {
	let configuration = NSImage.SymbolConfiguration(
		pointSize: 540,
		weight: .regular
	).applying(
		NSImage.SymbolConfiguration(paletteColors: [.white])
	)
	if let configured = symbol.withSymbolConfiguration(configuration) {
		let symbolRect = NSRect(x: 242, y: 242, width: 540, height: 540)
		configured.draw(in: symbolRect)
	}
}

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
	let bitmap = NSBitmapImageRep(data: tiff),
	let png = bitmap.representation(using: .png, properties: [:])
else {
	fputs("could not render icon\n", stderr)
	exit(1)
}

try png.write(to: URL(fileURLWithPath: output))
