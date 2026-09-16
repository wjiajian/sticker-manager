import Cocoa
import Carbon
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  private var platformChannel: FlutterMethodChannel?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    contentViewController = flutterViewController
    setFrame(windowFrame, display: true)
    title = "表情管家"

    RegisterGeneratedPlugins(registry: flutterViewController)
    let channel = FlutterMethodChannel(
      name: "sticker_manager/platform",
      binaryMessenger: flutterViewController.engine.binaryMessenger)
    platformChannel = channel
    channel.setMethodCallHandler { call, result in
      if call.method == "isHotKeyAvailable" {
        result(Self.isHotKeyAvailable(call.arguments as? [String: Any]))
        return
      }
      guard call.method == "copySticker" else {
        result(FlutterMethodNotImplemented)
        return
      }
      guard let arguments = call.arguments as? [String: Any],
            let path = arguments["path"] as? String,
            let mediaType = arguments["mediaType"] as? String,
            ["image", "gif"].contains(mediaType) else {
        result(FlutterError(code: "invalid_arguments", message: "Invalid sticker", details: nil))
        return
      }
      result(Self.copySticker(
        at: URL(fileURLWithPath: path), mediaType: mediaType, to: .general))
    }

    super.awakeFromNib()
  }

  // The hotkey plugin does not report Carbon registration failures on macOS.
  // Probe before replacing the saved shortcut so occupied keys remain visible.
  static func isHotKeyAvailable(_ arguments: [String: Any]?) -> Bool {
    guard let keyCode = arguments?["keyCode"] as? UInt32,
          let modifiers = arguments?["modifiers"] as? [String] else { return false }
    var flags: UInt32 = 0
    for modifier in modifiers {
      switch modifier {
      case "control": flags |= UInt32(controlKey)
      case "shift": flags |= UInt32(shiftKey)
      case "alt": flags |= UInt32(optionKey)
      case "meta": flags |= UInt32(cmdKey)
      default: return false
      }
    }
    guard flags != 0 else { return false }
    var reference: EventHotKeyRef?
    let status = RegisterEventHotKey(
      keyCode, flags, EventHotKeyID(signature: 0x53544B52, id: 1),
      GetApplicationEventTarget(), 0, &reference)
    guard status == noErr, let reference = reference else { return false }
    UnregisterEventHotKey(reference)
    return true
  }

  func restoreManagementWindow() {
    platformChannel?.invokeMethod("restoreManagementMode", arguments: nil)
    if isMiniaturized { deminiaturize(nil) }
    makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
  }

  /// Prepare every representation before replacing the user's clipboard.
  static func copySticker(
    at url: URL, mediaType: String, to pasteboard: NSPasteboard
  ) -> Bool {
    guard let data = try? Data(contentsOf: url), !data.isEmpty else { return false }
    let item = NSPasteboardItem()
    if mediaType == "gif" {
      let signature = String(data: data.prefix(6), encoding: .ascii)
      guard signature == "GIF87a" || signature == "GIF89a",
            item.setData(data, forType: NSPasteboard.PasteboardType("com.compuserve.gif")),
            item.setString(url.absoluteString, forType: .fileURL) else { return false }
    } else if mediaType == "image" {
      guard let bitmap = NSBitmapImageRep(data: data),
            let png = bitmap.representation(using: .png, properties: [:]),
            item.setData(png, forType: .png) else { return false }
    } else {
      return false
    }
    pasteboard.clearContents()
    return pasteboard.writeObjects([item])
  }
}
