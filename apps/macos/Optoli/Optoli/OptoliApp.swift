import SwiftUI

@main
struct OptoliApp: App {
	@NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
	@StateObject private var model = AppModel()

	var body: some Scene {
		MenuBarExtra("Optoli", systemImage: "lightbulb") {
			ContentView(model: model)
		}
		.menuBarExtraStyle(.window)
	}
}
