import SwiftUI

@main
struct RadioApp: App {
    @StateObject private var model = RadioModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
        }
    }
}
