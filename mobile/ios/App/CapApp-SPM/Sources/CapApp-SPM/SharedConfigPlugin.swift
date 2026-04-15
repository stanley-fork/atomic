import Capacitor
import Foundation

/// Exposes the share-extension's App Group-backed config to JS. The JS side
/// pushes `{ serverURL, apiToken, databaseId }` whenever transport or active
/// database changes; the share extension reads the same keys synchronously
/// from `UserDefaults(suiteName: "group.com.atomic.mobile")` when invoked.
@objc(SharedConfigPlugin)
public class SharedConfigPlugin: CAPPlugin, CAPBridgedPlugin {
    public let identifier = "SharedConfigPlugin"
    public let jsName = "SharedConfig"
    public let pluginMethods: [CAPPluginMethod] = [
        CAPPluginMethod(name: "set", returnType: CAPPluginReturnPromise),
        CAPPluginMethod(name: "clear", returnType: CAPPluginReturnPromise),
    ]

    private static let appGroupID = "group.com.atomic.mobile"

    @objc func set(_ call: CAPPluginCall) {
        guard let defaults = UserDefaults(suiteName: Self.appGroupID) else {
            call.reject("Failed to access app group \(Self.appGroupID)")
            return
        }
        // Only touch keys the caller explicitly provided. Absent keys must
        // be left alone so partial updates (server URL now, databaseId
        // later) don't clobber each other.
        for key in ["serverURL", "apiToken", "databaseId"] {
            if let value = call.getString(key) {
                defaults.set(value, forKey: key)
            }
        }
        call.resolve()
    }

    @objc func clear(_ call: CAPPluginCall) {
        guard let defaults = UserDefaults(suiteName: Self.appGroupID) else {
            call.reject("Failed to access app group \(Self.appGroupID)")
            return
        }
        for key in ["serverURL", "apiToken", "databaseId"] {
            defaults.removeObject(forKey: key)
        }
        call.resolve()
    }
}
