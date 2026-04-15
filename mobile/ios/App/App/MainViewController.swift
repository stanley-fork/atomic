import Capacitor
@_exported import CapApp_SPM
import UIKit

/// Subclass of `CAPBridgeViewController` whose sole purpose is to register
/// in-app Capacitor plugins. SPM-based apps don't reliably auto-discover
/// Swift plugin classes via the ObjC runtime, so we hand the bridge the
/// instance here.
class MainViewController: CAPBridgeViewController {
    override open func capacitorDidLoad() {
        bridge?.registerPluginInstance(SharedConfigPlugin())
    }
}
