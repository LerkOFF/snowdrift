import AppKit
import ApplicationServices

/// Panels inside a window, which the window list knows nothing of: a sheet, a dialog, a popover, a picture opened over
/// the page. Only with Snow on Panels on, and the Accessibility permission granted: they are read from the app's
/// accessibility tree in the background, and each catches snow on its top edge like a window.
enum Panels {
    /// One panel, in the window list's coordinates, and the frame of the window it is in.
    struct Panel {
        let window: CGRect
        let frame: CGRect
        let kind: Kind
    }

    enum Kind { case dialog, picture }

    static var enabled = false {
        didSet {
            guard enabled != oldValue else { return }
            if enabled {
                if !AXIsProcessTrusted() {
                    _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
                }
            } else {
                lock.lock()
                found = [:]
                lock.unlock()
            }
        }
    }

    /// A window number no window has, stable for the same panel as long as it lives: from its app, its window and its place
    /// among that window's panels.
    static func id(pid: pid_t, window: CGWindowID, index: Int) -> CGWindowID {
        0xE000_0000 | UInt32(truncatingIfNeeded: pid & 0x7FF) << 17 | (window & 0xFF) << 9 | UInt32(truncatingIfNeeded: index & 0x1FF)
    }

    /// Whether the permission is there, so the menu can say so.
    static var trusted: Bool { AXIsProcessTrusted() }

    private static let lock = NSLock()
    private static var found: [pid_t: [Panel]] = [:]
    private static var scanning = false
    private static var lastScan: CFTimeInterval = 0
    private static var woken = Set<pid_t>()
    private static let queue = DispatchQueue(label: "snowdrift.panels", qos: .utility)

    private static let roles: Set<String> = ["AXSheet", "AXPopover", "AXDrawer"]
    private static let subroles: Set<String> = ["AXDialog", "AXSystemDialog", "AXApplicationDialog", "AXFloatingWindow", "AXSystemFloatingWindow"]

    /// The panels last found inside the window of `pid` with this frame.
    static func panels(of pid: pid_t, in window: CGRect) -> [Panel] {
        guard enabled else { return [] }
        lock.lock()
        defer { lock.unlock() }
        return (found[pid] ?? []).filter { near($0.window, window) }
    }

    /// Asks for a fresh look at the app in front, at most a few times a second and never two at once.
    static func refresh(now: CFTimeInterval) {
        guard enabled, AXIsProcessTrusted(), let front = NSWorkspace.shared.frontmostApplication, front.processIdentifier != getpid() else { return }
        lock.lock()
        if scanning || now - lastScan < 0.4 {
            lock.unlock()
            return
        }
        scanning = true
        lastScan = now
        lock.unlock()
        let pid = front.processIdentifier
        queue.async {
            let result = scan(pid: pid)
            lock.lock()
            found[pid] = result
            scanning = false
            lock.unlock()
        }
    }

    private static func near(_ a: CGRect, _ b: CGRect) -> Bool {
        abs(a.minX - b.minX) < 4 && abs(a.minY - b.minY) < 4 && abs(a.width - b.width) < 6 && abs(a.height - b.height) < 6
    }

    // MARK: Reading the tree

    private static func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
        var value: CFTypeRef?
        return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
    }

    private static func frame(of element: AXUIElement) -> CGRect? {
        guard let raw = attribute(element, "AXFrame"), CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
        var rect = CGRect.zero
        return AXValueGetValue(raw as! AXValue, .cgRect, &rect) ? rect : nil
    }

    /// Every panel in every window of the app: nothing if it has none or doesn't answer in time.
    static func scan(pid: pid_t) -> [Panel] {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 0.3)
        // Chromium and Electron build their accessibility tree only when asked, and this is how.
        if !woken.contains(pid) {
            woken.insert(pid)
            AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        }
        guard let windows = attribute(app, kAXWindowsAttribute) as? [AXUIElement] else { return [] }
        var panels: [Panel] = []
        let deadline = CACurrentMediaTime() + 0.25
        for window in windows.prefix(8) {
            guard let bounds = frame(of: window), bounds.width > 0 else { continue }
            var budget = 2500
            walk(window, depth: 0, window: bounds, panels: &panels, budget: &budget, deadline: deadline)
        }
        return panels
    }

    private static func walk(_ element: AXUIElement, depth: Int, window: CGRect, panels: inout [Panel], budget: inout Int, deadline: CFTimeInterval) {
        guard depth < 9, budget > 0, CACurrentMediaTime() < deadline,
              let children = attribute(element, kAXChildrenAttribute) as? [AXUIElement]
        else { return }
        for child in children {
            guard budget > 0 else { return }
            budget -= 1
            let role = attribute(child, kAXRoleAttribute) as? String ?? ""
            let subrole = attribute(child, kAXSubroleAttribute) as? String ?? ""
            let isDialog = roles.contains(role) || subroles.contains(subrole)
            let isPicture = role == "AXImage"
            guard isDialog || isPicture || depth < 7, let rect = frame(of: child) else {
                if depth < 7 { walk(child, depth: depth + 1, window: window, panels: &panels, budget: &budget, deadline: deadline) }
                continue
            }
            let inside = window.insetBy(dx: -2, dy: -2).contains(rect)
            let share = rect.width * rect.height / max(window.width * window.height, 1)
            if isDialog, inside, rect.width >= 120, rect.height >= 60, share < 0.95 {
                panels.append(Panel(window: window, frame: rect, kind: .dialog))
                continue
            }
            // A picture opened over the page is big; the thumbnails and icons in a page are not.
            if isPicture, inside, rect.width >= 360, rect.height >= 220, share < 0.9 {
                panels.append(Panel(window: window, frame: rect, kind: .picture))
                continue
            }
            walk(child, depth: depth + 1, window: window, panels: &panels, budget: &budget, deadline: deadline)
        }
    }
}
