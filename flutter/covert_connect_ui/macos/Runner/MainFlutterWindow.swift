import Cocoa
import FlutterMacOS
import window_manager
import bitsdojo_window_macos

class MainFlutterWindow: BitsdojoWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    flutterViewController.mouseTrackingMode = .always
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
  
  override func bitsdojo_window_configure() -> UInt {
    return BDW_CUSTOM_FRAME | BDW_HIDE_ON_STARTUP
  }
}
