import Foundation
import SwiftUI

@main
struct ElopliApp: App {
	@NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
	@StateObject private var model = AppModel()

	init() {
		// Show every system help tooltip after 500 ms instead of the default.
		UserDefaults.standard.register(defaults: ["NSInitialToolTipDelay": 500])
	}

	var body: some Scene {
		MenuBarExtra("Elopli", systemImage: "lightbulb") {
			ContentView(model: model)
				.onAppear {
					appDelegate.model = model
				}
		}
		.menuBarExtraStyle(.window)
	}
}
