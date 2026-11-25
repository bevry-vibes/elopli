import SwiftUI

@main
struct ElopliApp: App {
	@NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
	@StateObject private var model = AppModel()

	var body: some Scene {
		MenuBarExtra("Elopli", systemImage: "lightbulb") {
			ContentView(model: model)
		}
		.menuBarExtraStyle(.window)
	}
}
