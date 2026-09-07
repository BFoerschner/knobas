// The accessibility half of the desktop witness (issue #500).
//
// Everything the harness and its drivers need from macOS that shell cannot
// reach: whether this session can drive the desktop at all, where Launch
// Services has registered a bundle identifier, what the frontmost app has
// focused, and the two synthetic keystrokes a driver sends.
//
// **Why this and not AppleScript.** The obvious spelling of all of it is
// `osascript` against `System Events`, and that is an Apple Event, which TCC
// gates under *Automation*. Automation cannot be granted without a human at
// the machine: the grant is a modal prompt, and an unanswered prompt is
// recorded as a *denial* (`auth_reason 9`), which is what this Mac's TCC
// database holds for the terminal as of 2026-09-08. The AX API used here is
// gated under *Accessibility* instead -- one grant for the terminal, made
// once, with no per-target prompt afterwards -- so a scripted run that nobody
// is watching either works or says why. `testenv/README.md`, *The desktop
// witness (macOS)*, is where that prerequisite is written down.
//
// Compiled by `desktop-witness.sh` into a scratch directory on each run
// (about 2 s with `swiftc -O`); nothing here is committed as a binary.

import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

// MARK: - AX attribute reading

/// One attribute of one element, or `nil` when the element does not carry it.
///
/// `AnyObject?` rather than `CFTypeRef?` deliberately: the bridged form is
/// ARC-managed, so an element read out of a returned array outlives the array.
/// The hand-rolled `CFArrayGetValueAtIndex` + `unsafeBitCast` spelling reads
/// freed memory the moment the array is released, and the symptom is not a
/// crash -- every element answers as if it were the application element, which
/// looks exactly like an app that exposes no windows.
func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
        return nil
    }
    return value
}

func text(_ element: AXUIElement, _ name: String) -> String {
    (attribute(element, name) as? String) ?? ""
}

func children(_ element: AXUIElement, _ name: String) -> [AXUIElement] {
    guard let raw = attribute(element, name) as? [AnyObject] else { return [] }
    return raw.compactMap { item in
        CFGetTypeID(item) == AXUIElementGetTypeID() ? (item as! AXUIElement) : nil
    }
}

func element(_ element: AXUIElement, _ name: String) -> AXUIElement? {
    guard let raw = attribute(element, name), CFGetTypeID(raw) == AXUIElementGetTypeID() else {
        return nil
    }
    return (raw as! AXUIElement)
}

/// The attributes a driver asserts on. One per line, tab-separated, so a value
/// containing spaces survives the trip through `grep` and `cut`.
func describe(_ element: AXUIElement) -> String {
    let fields = [
        ("role", kAXRoleAttribute as String),
        ("subrole", kAXSubroleAttribute as String),
        ("description", kAXDescriptionAttribute as String),
        ("title", kAXTitleAttribute as String),
        ("placeholder", "AXPlaceholderValue"),
        ("dom-id", "AXDOMIdentifier"),
    ]
    return fields.map { "\($0.0)\t\(text(element, $0.1))" }.joined(separator: "\n")
}

// MARK: - Session state

/// Whether the login session's screen is locked.
///
/// A locked screen is the difference between a witness and a hang: no
/// synthetic keystroke reaches an application behind the lock screen, and the
/// window server answers every third-party window query with the application
/// element instead of the window. Both look like "the app did not respond".
func screenIsLocked() -> Bool {
    guard let session = CGSessionCopyCurrentDictionary() as? [String: Any] else { return false }
    return (session["CGSSessionScreenIsLocked"] as? Int) == 1
}

// MARK: - Subcommands

func probe() {
    // Printed even when everything is granted, so a green transcript records
    // what was true at the moment the driver ran rather than implying it.
    print("trusted\t\(AXIsProcessTrustedWithOptions(nil) ? 1 : 0)")
    print("post-events\t\(CGPreflightPostEventAccess() ? 1 : 0)")
    print("screen-locked\t\(screenIsLocked() ? 1 : 0)")
}

func registeredPath(bundleID: String) -> Int32 {
    guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else {
        FileHandle.standardError.write(Data("ax: no application registered for \(bundleID)\n".utf8))
        return 1
    }
    print(url.path)
    return 0
}

/// A real window, as opposed to the application element the window server
/// substitutes when it will not show one (a locked screen does this).
func windows(of pid: pid_t) -> [AXUIElement] {
    let app = AXUIElementCreateApplication(pid)
    return children(app, kAXWindowsAttribute as String).filter {
        text($0, kAXRoleAttribute as String) == (kAXWindowRole as String)
    }
}

func waitForWindow(pid: pid_t, seconds: Double) -> Int32 {
    let deadline = Date().addingTimeInterval(seconds)
    while Date() < deadline {
        if let window = windows(of: pid).first {
            print(text(window, kAXTitleAttribute as String))
            return 0
        }
        Thread.sleep(forTimeInterval: 0.25)
    }
    FileHandle.standardError.write(
        Data("ax: pid \(pid) showed no window within \(Int(seconds)) s\n".utf8))
    return 1
}

func activate(pid: pid_t) -> Int32 {
    guard let app = NSRunningApplication(processIdentifier: pid) else {
        FileHandle.standardError.write(Data("ax: no running application with pid \(pid)\n".utf8))
        return 1
    }
    app.activate()
    // Frontmost is asynchronous: the driver's keystroke goes to whatever is
    // front when it is posted, so waiting here is not a politeness.
    let deadline = Date().addingTimeInterval(5)
    while Date() < deadline {
        if app.isActive { return 0 }
        Thread.sleep(forTimeInterval: 0.1)
    }
    FileHandle.standardError.write(Data("ax: pid \(pid) did not become frontmost\n".utf8))
    return 1
}

func focused(pid: pid_t) -> Int32 {
    let app = AXUIElementCreateApplication(pid)
    guard let target = element(app, kAXFocusedUIElementAttribute as String) else {
        FileHandle.standardError.write(Data("ax: pid \(pid) reports no focused element\n".utf8))
        return 1
    }
    print(describe(target))
    return 0
}

func dump(pid: pid_t, depth: Int) -> Int32 {
    func walk(_ element: AXUIElement, _ level: Int) {
        let indent = String(repeating: "  ", count: level)
        let line = describe(element).split(separator: "\n").joined(separator: " ")
        print(indent + line)
        guard level < depth else { return }
        for child in children(element, kAXChildrenAttribute as String) { walk(child, level + 1) }
    }
    let found = windows(of: pid)
    if found.isEmpty {
        print("(no window: the app exposes none, or the screen is locked)")
        return 1
    }
    for window in found { walk(window, 0) }
    return 0
}

/// Post one key to the frontmost application.
///
/// `.cghidEventTap` rather than posting to the pid: an event posted to a
/// process is delivered whether or not that process is front, so a driver
/// could "press ⌘K" at an app that never had the keyboard and read a stale
/// tree as success. Through the HID tap the keystroke goes where a person's
/// would, which is the thing being witnessed.
func key(code: CGKeyCode, command: Bool) -> Int32 {
    guard let source = CGEventSource(stateID: .hidSystemState) else {
        FileHandle.standardError.write(Data("ax: no event source\n".utf8))
        return 1
    }
    let flags: CGEventFlags = command ? .maskCommand : []
    for isDown in [true, false] {
        guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: isDown)
        else {
            FileHandle.standardError.write(Data("ax: could not build a key event\n".utf8))
            return 1
        }
        event.flags = flags
        event.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.02)
    }
    return 0
}

// MARK: - Entry point

let arguments = Array(CommandLine.arguments.dropFirst())
func number(_ index: Int) -> Int? { Int(arguments[index]) }

var status: Int32 = 0
switch arguments.first {
case "probe":
    probe()
case "registered-path" where arguments.count == 2:
    status = registeredPath(bundleID: arguments[1])
case "wait-window" where arguments.count == 3:
    status = waitForWindow(pid: pid_t(number(1)!), seconds: Double(arguments[2])!)
case "activate" where arguments.count == 2:
    status = activate(pid: pid_t(number(1)!))
case "focused" where arguments.count == 2:
    status = focused(pid: pid_t(number(1)!))
case "dump" where arguments.count == 3:
    status = dump(pid: pid_t(number(1)!), depth: number(2)!)
case "key" where arguments.count >= 2:
    status = key(code: CGKeyCode(number(1)!), command: arguments.contains("command"))
default:
    FileHandle.standardError.write(
        Data(
            """
            usage: ax probe
                   ax registered-path <bundle-id>
                   ax wait-window <pid> <seconds>
                   ax activate <pid>
                   ax focused <pid>
                   ax dump <pid> <depth>
                   ax key <keycode> [command]

            """.utf8))
    status = 2
}
exit(status)
