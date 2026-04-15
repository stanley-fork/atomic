import Foundation

enum SharedConfig {
    static let appGroupID = "group.com.atomic.mobile"

    private static var defaults: UserDefaults? {
        UserDefaults(suiteName: appGroupID)
    }

    static var serverURL: String? {
        get { defaults?.string(forKey: "serverURL") }
        set { defaults?.set(newValue, forKey: "serverURL") }
    }

    static var apiToken: String? {
        get { defaults?.string(forKey: "apiToken") }
        set { defaults?.set(newValue, forKey: "apiToken") }
    }

    static var databaseId: String? {
        get { defaults?.string(forKey: "databaseId") }
        set { defaults?.set(newValue, forKey: "databaseId") }
    }

    static var isConfigured: Bool {
        guard let url = serverURL, let token = apiToken else { return false }
        return !url.isEmpty && !token.isEmpty
    }
}
