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

/// Walk every element under a window, depth-first.
///
/// Bounded twice over, because an accessibility tree is not a tree: Finder's
/// desktop element lists the *application* among its children, and a walk that
/// trusted the shape would recurse until it ran out of stack. The budget is
/// the second wall, for a cycle that hides below the depth limit.
func visit(_ root: AXUIElement, maxDepth: Int, _ body: (AXUIElement, Int) -> Void) {
    var budget = 5000
    func walk(_ element: AXUIElement, _ level: Int) {
        guard budget > 0 else { return }
        budget -= 1
        body(element, level)
        guard level < maxDepth else { return }
        for child in children(element, kAXChildrenAttribute as String) { walk(child, level + 1) }
    }
    walk(root, 0)
}

/// How many elements under this app's windows carry `label`.
///
/// Description *or* title: WebKit is the thing under a Tauri window, and which
/// of the two an `aria-label` arrives as is a WebKit detail rather than a
/// promise. A driver that insisted on one would fail on a correct app for a
/// reason that has nothing to do with what it is witnessing.
func find(pid: pid_t, label: String) -> Int32 {
    var count = 0
    for window in windows(of: pid) {
        visit(window, maxDepth: 25) { element, _ in
            if text(element, kAXDescriptionAttribute as String) == label
                || text(element, kAXTitleAttribute as String) == label
            {
                count += 1
            }
        }
    }
    print(count)
    return 0
}

func dump(pid: pid_t, depth: Int) -> Int32 {
    let found = windows(of: pid)
    if found.isEmpty {
        print("(no window: the app exposes none, or the screen is locked)")
        return 1
    }
    for window in found {
        visit(window, maxDepth: depth) { element, level in
            let indent = String(repeating: "  ", count: level)
            print(indent + describe(element).split(separator: "\n").joined(separator: " "))
        }
    }
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

/// A numeric argument, or `nil`. Nothing here force-unwraps: a mistyped pid
/// would crash with a Swift trap, and a harness reads a trap as "the app is
/// broken" rather than "the command line was". Bad arguments land on the usage
/// block below like an unknown subcommand does.
func integer(_ index: Int) -> Int? {
    index < arguments.count ? Int(arguments[index]) : nil
}
func seconds(_ index: Int) -> Double? {
    index < arguments.count ? Double(arguments[index]) : nil
}

var status: Int32 = 0
switch arguments.first {
case "probe" where arguments.count == 1:
    probe()
case "registered-path" where arguments.count == 2:
    status = registeredPath(bundleID: arguments[1])
case "wait-window" where arguments.count == 3 && integer(1) != nil && seconds(2) != nil:
    status = waitForWindow(pid: pid_t(integer(1)!), seconds: seconds(2)!)
case "activate" where arguments.count == 2 && integer(1) != nil:
    status = activate(pid: pid_t(integer(1)!))
case "focused" where arguments.count == 2 && integer(1) != nil:
    status = focused(pid: pid_t(integer(1)!))
case "find" where arguments.count == 3 && integer(1) != nil:
    status = find(pid: pid_t(integer(1)!), label: arguments[2])
case "dump" where arguments.count == 3 && integer(1) != nil && integer(2) != nil:
    status = dump(pid: pid_t(integer(1)!), depth: integer(2)!)
// The modifier is spelled out rather than taken from "any second word", so a
// typo is a refusal instead of a keystroke sent without ⌘.
case "key" where (arguments.count == 2 || (arguments.count == 3 && arguments[2] == "command"))
    && integer(1) != nil:
    status = key(code: CGKeyCode(integer(1)!), command: arguments.count == 3)
default:
    FileHandle.standardError.write(
        Data(
            """
            usage: ax probe
                   ax registered-path <bundle-id>
                   ax wait-window <pid> <seconds>
                   ax activate <pid>
                   ax focused <pid>
                   ax find <pid> <label>
                   ax dump <pid> <depth>
                   ax key <keycode> [command]

            """.utf8))
    status = 2
}
exit(status)
