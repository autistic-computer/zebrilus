import AppKit

func color(_ hex: UInt32, alpha: CGFloat = 1) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255,
            green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255,
            alpha: alpha)
}

let rootPath = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let backdropPath = rootPath.appendingPathComponent("macos-backdrop.png")

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

func label(_ text: String, x: CGFloat, y: CGFloat, width: CGFloat,
           size: CGFloat = 13, weight: NSFont.Weight = .regular,
           secondary: Bool = false) -> NSTextField {
    let field = NSTextField(labelWithString: text)
    field.font = NSFont.systemFont(ofSize: size, weight: weight)
    field.textColor = secondary ? .secondaryLabelColor : .labelColor
    field.frame = NSRect(x: x, y: y, width: width, height: size + 10)
    return field
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var window: NSWindow!
    var toggle: NSButton!
    var neutral: NSView!
    var keyStatus: NSTextField!

    func applicationDidFinishLaunching(_ notification: Notification) {
        let mainMenu = NSMenu()
        let applicationItem = NSMenuItem()
        let applicationMenu = NSMenu()
        let focusItem = applicationMenu.addItem(withTitle: "Deactivate Reference",
                                               action: #selector(deactivateReference), keyEquivalent: "a")
        focusItem.keyEquivalentModifierMask = [.command, .shift]
        focusItem.target = self
        let referenceItem = applicationMenu.addItem(withTitle: "Activate Reference",
                                                   action: #selector(activateReference), keyEquivalent: "r")
        referenceItem.keyEquivalentModifierMask = [.command, .shift]
        referenceItem.target = self
        applicationMenu.addItem(.separator())
        applicationMenu.addItem(withTitle: "Quit macOS Liquid Glass Reference",
                                action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        applicationItem.submenu = applicationMenu
        mainMenu.addItem(applicationItem)
        NSApp.mainMenu = mainMenu
        window = NSWindow(contentRect: NSRect(x: 40, y: 100, width: 600, height: 720),
                          styleMask: [.titled, .closable, .miniaturizable],
                          backing: .buffered, defer: false)
        window.title = "macOS Liquid Glass Reference"
        if let screen = NSScreen.main {
            window.setFrameTopLeftPoint(NSPoint(x: 40, y: screen.visibleFrame.maxY - 100))
        }
        window.appearance = NSAppearance(named: .darkAqua)
        window.isReleasedWhenClosed = false
        window.delegate = self
        let root = FlippedView(frame: NSRect(x: 0, y: 0, width: 600, height: 720))
        window.contentView = root
        let backdrop = NSImageView(frame: root.bounds)
        backdrop.image = NSImage(contentsOf: backdropPath)
        backdrop.imageScaling = .scaleAxesIndependently
        root.addSubview(backdrop)
        neutral = NSView(frame: NSRect(x: 0, y: 0, width: 600, height: 430))
        neutral.wantsLayer = true
        neutral.layer?.backgroundColor = color(0x242424).cgColor
        root.addSubview(neutral)
        root.addSubview(label("macOS controls", x: 28, y: 30, width: 430, size: 26, weight: .semibold))
        root.addSubview(label("Native buttons and tabs on neutral charcoal", x: 28, y: 67, width: 500, size: 13, secondary: true))

        for (index, symbol) in ["sidebar.left", "square.and.pencil", "magnifyingglass"].enumerated() {
            let button = NSButton(image: NSImage(systemSymbolName: symbol, accessibilityDescription: symbol)!,
                                  target: self, action: #selector(controlAction(_:)))
            button.isEnabled = true
            button.bezelStyle = .glass
            button.borderShape = .circle
            button.controlSize = .extraLarge
            button.image = button.image?.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 18, weight: .regular))
            button.frame = NSRect(x: 28 + index * 60, y: 110, width: 48, height: 48)
            root.addSubview(button)
        }

        let grouped = NSGlassEffectView(frame: NSRect(x: 210, y: 110, width: 220, height: 48))
        grouped.style = .regular
        grouped.cornerRadius = 24
        grouped.effectIsInteractive = true
        let groupContent = FlippedView(frame: grouped.bounds)
        grouped.contentView = groupContent
        for (index, symbol) in ["archivebox", "trash", "arrowshape.turn.up.left"].enumerated() {
            let button = NSButton(image: NSImage(systemSymbolName: symbol, accessibilityDescription: symbol)!,
                                  target: self, action: #selector(controlAction(_:)))
            button.isBordered = false
            button.controlSize = .extraLarge
            button.image = button.image?.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 18, weight: .regular))
            button.frame = NSRect(x: CGFloat(index) * 220 / 3, y: 0, width: 220 / 3, height: 48)
            groupContent.addSubview(button)
        }
        root.addSubview(grouped)

        let tabs = NSSegmentedControl(labels: ["Terminal", "Browser", "Agent"],
                                      trackingMode: .selectOne, target: self, action: #selector(controlAction(_:)))
        tabs.isEnabled = true
        tabs.role = .tabs
        tabs.segmentStyle = .automatic
        tabs.borderShape = .capsule
        tabs.controlSize = .extraLarge
        tabs.segmentDistribution = .fillEqually
        tabs.selectedSegment = 0
        tabs.font = NSFont.systemFont(ofSize: 14, weight: .medium)
        tabs.frame = NSRect(x: 28, y: 200, width: 544, height: 54)
        root.addSubview(tabs)

        let regular = NSGlassEffectView(frame: NSRect(x: 28, y: 300, width: 544, height: 120))
        regular.style = .regular
        regular.cornerRadius = 24
        let panel = FlippedView(frame: regular.bounds)
        regular.contentView = panel
        panel.addSubview(label("Regular glass", x: 24, y: 25, width: 300, size: 18, weight: .semibold))
        panel.addSubview(label("Quiet fill, soft rim, lifted controls", x: 24, y: 58, width: 450, secondary: true))
        root.addSubview(regular)

        root.addSubview(label("COLOR BACKDROP", x: 28, y: 450, width: 300, size: 11, weight: .semibold, secondary: true))
        let clear = NSGlassEffectView(frame: NSRect(x: 28, y: 480, width: 544, height: 100))
        clear.style = .regular
        clear.cornerRadius = 24
        clear.effectIsInteractive = true
        let toolbar = FlippedView(frame: clear.bounds)
        clear.contentView = toolbar
        toolbar.addSubview(label("Glass over color", x: 24, y: 39, width: 220, size: 14, weight: .semibold))
        let back = NSButton(title: "Back", target: self, action: #selector(controlAction(_:)))
        back.isEnabled = true
        back.bezelStyle = .glass
        back.frame = NSRect(x: 328, y: 34, width: 80, height: 32)
        toolbar.addSubview(back)
        let next = NSButton(title: "Continue", target: self, action: #selector(controlAction(_:)))
        next.isEnabled = true
        next.bezelStyle = .glass
        next.keyEquivalent = "\r"
        next.frame = NSRect(x: 418, y: 34, width: 102, height: 32)
        toolbar.addSubview(next)
        root.addSubview(clear)

        toggle = NSButton(title: "Switch to light", target: self, action: #selector(toggleAppearance))
        toggle.bezelStyle = .rounded
        toggle.frame = NSRect(x: 408, y: 650, width: 164, height: 32)
        root.addSubview(toggle)
        keyStatus = label("Window inactive", x: 28, y: 657, width: 360, size: 12, weight: .semibold)
        root.addSubview(keyStatus)
        activateReference()
    }

    @objc func toggleAppearance() {
        let dark = window.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        window.appearance = NSAppearance(named: dark ? .aqua : .darkAqua)
        neutral.layer?.backgroundColor = color(dark ? 0xF5F5F7 : 0x242424).cgColor
        toggle.title = dark ? "Switch to dark" : "Switch to light"
    }

    @objc func controlAction(_ sender: NSControl) {
        activateReference()
    }

    @objc func activateReference() {
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        window.makeMain()
        window.makeFirstResponder(nil)
        updateKeyStatus()
    }

    @objc func deactivateReference() {
        NSApp.deactivate()
        updateKeyStatus()
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        updateKeyStatus()
    }

    func applicationDidResignActive(_ notification: Notification) {
        updateKeyStatus()
    }

    func windowDidBecomeKey(_ notification: Notification) {
        updateKeyStatus()
    }

    func windowDidResignKey(_ notification: Notification) {
        updateKeyStatus()
    }

    func updateKeyStatus() {
        guard let window else { return }
        let applicationState = NSApp.isActive ? "active" : "inactive"
        let windowState = window.isKeyWindow ? "key" : "not key"
        keyStatus?.stringValue = "App \(applicationState) · window \(windowState)"
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

@main
struct Main {
    @MainActor
    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.regular)
        let delegate = AppDelegate()
        app.delegate = delegate
        app.run()
    }
}
