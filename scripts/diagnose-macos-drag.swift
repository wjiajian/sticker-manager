// 只观察拖拽类型，不读取文件名、文件内容或接收文件承诺。
import AppKit
import Foundation

final class DropProbe: NSView {
    private var status = "将一张表情拖到这里，再查看终端中的类型。"

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        // 只有匹配已注册类型的拖拽才会通知此视图；未知私有类型可能不会触发回调。
        let common = [
            "public.file-url", "public.url", "NSFilenamesPboardType",
            "public.image", "public.data", "public.png", "public.tiff", "public.jpeg",
            "com.compuserve.gif", "org.webmproject.webp", "public.heic",
        ].map { NSPasteboard.PasteboardType($0) }
        let promised = NSFilePromiseReceiver.readableDraggedTypes.map { NSPasteboard.PasteboardType($0) }
        registerForDraggedTypes(common + promised)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.setFill()
        bounds.fill()
        let text = "macOS 拖拽类型诊断\n\n\(status)\n\n仅记录类型，不读取文件名或内容。关闭窗口即可退出。"
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 16),
            .foregroundColor: NSColor.labelColor,
        ]
        (text as NSString).draw(in: bounds.insetBy(dx: 24, dy: 24), withAttributes: attributes)
    }

    private func report(_ phase: String, _ sender: NSDraggingInfo) {
        // 不使用全局命名拖拽剪贴板：跨进程操作不保证使用它。
        let board = sender.draggingPasteboard
        let items = board.pasteboardItems ?? []
        print("\n=== \(phase) ===")
        print("source operation mask: \(sender.draggingSourceOperationMask.rawValue)")
        print("pasteboard types:")
        for type in board.types ?? [] { print("  \(type.rawValue)") }
        print("items: \(items.count)")
        for (index, item) in items.enumerated() {
            print("item \(index + 1) types:")
            for type in item.types { print("  \(type.rawValue)") }
        }
        print("=== end ===")
        fflush(stdout)
        status = "已记录\(phase)；请回到终端查看类型。"
        needsDisplay = true
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        report("进入", sender)
        // 只接受复制，不以移动操作影响源文件。
        return sender.draggingSourceOperationMask.contains(.copy) ? .copy : []
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        sender.draggingSourceOperationMask.contains(.copy) ? .copy : []
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        report("松开", sender)
        return true
    }
}

final class ProbeAppDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        let frame = NSRect(x: 0, y: 0, width: 520, height: 240)
        let window = NSWindow(contentRect: frame, styleMask: [.titled, .closable, .resizable],
                              backing: .buffered, defer: false)
        window.title = "拖拽类型诊断"
        window.contentView = DropProbe(frame: frame)
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window
        print("诊断窗口已打开。请分别从微信、飞书、QQ 拖入表情；关闭窗口结束。")
        fflush(stdout)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

let app = NSApplication.shared
let delegate = ProbeAppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
