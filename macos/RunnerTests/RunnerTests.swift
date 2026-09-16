import Cocoa
import XCTest
@testable import sticker_manager

class RunnerTests: XCTestCase {
  private var directory: URL!
  private var pasteboard: NSPasteboard!

  override func setUpWithError() throws {
    directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    pasteboard = NSPasteboard.withUniqueName()
  }

  override func tearDownWithError() throws {
    pasteboard.releaseGlobally()
    try FileManager.default.removeItem(at: directory)
  }

  func testStaticImageWithoutExtensionIsCopiedAsPNG() throws {
    let bitmap = NSBitmapImageRep(
      bitmapDataPlanes: nil, pixelsWide: 2, pixelsHigh: 2, bitsPerSample: 8,
      samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
      colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    let data = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
    let url = directory.appendingPathComponent("hash.image")
    try data.write(to: url)
    XCTAssertTrue(MainFlutterWindow.copySticker(at: url, mediaType: "image", to: pasteboard))
    let copied = try XCTUnwrap(pasteboard.data(forType: .png))
    XCTAssertNotNil(NSBitmapImageRep(data: copied))
    XCTAssertNil(pasteboard.string(forType: .fileURL))
  }

  func testGIFPreservesOriginalBytesAndFileURL() throws {
    let data = Data(base64Encoded: "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7")!
    let url = directory.appendingPathComponent("animation with spaces.gif")
    try data.write(to: url)
    XCTAssertTrue(MainFlutterWindow.copySticker(at: url, mediaType: "gif", to: pasteboard))
    XCTAssertEqual(pasteboard.data(forType: NSPasteboard.PasteboardType("com.compuserve.gif")), data)
    XCTAssertEqual(pasteboard.string(forType: .fileURL), url.absoluteString)
    XCTAssertNil(pasteboard.data(forType: .png))
  }

  func testMissingAndInvalidFilesPreserveClipboard() throws {
    pasteboard.setString("existing content", forType: .string)
    let url = directory.appendingPathComponent("invalid.image")
    XCTAssertFalse(MainFlutterWindow.copySticker(at: url, mediaType: "image", to: pasteboard))
    try Data("not an image".utf8).write(to: url)
    XCTAssertFalse(MainFlutterWindow.copySticker(at: url, mediaType: "image", to: pasteboard))
    XCTAssertFalse(MainFlutterWindow.copySticker(at: url, mediaType: "gif", to: pasteboard))
    XCTAssertEqual(pasteboard.string(forType: .string), "existing content")
  }
}
