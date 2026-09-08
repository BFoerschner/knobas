// The accessibility half of the desktop witness (issue #500).
//
// Everything the harness and its drivers need from macOS that shell cannot
// reach: whether this session can drive the desktop at all, where Launch
// Services has registered a bundle identifier, what the frontmost app has
// focused, the synthetic keystrokes and typing a driver sends, and the two
// things it does to a named element -- focus it, press it.
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
func matching(pid: pid_t, label: String) -> [AXUIElement] {
    var found: [AXUIElement] = []
    for window in windows(of: pid) {
        visit(window, maxDepth: 25) { element, _ in
            if text(element, kAXDescriptionAttribute as String) == label
                || text(element, kAXTitleAttribute as String) == label
            {
                found.append(element)
            }
        }
    }
    return found
}

func find(pid: pid_t, label: String) -> Int32 {
    print(matching(pid: pid, label: label).count)
    return 0
}

/// The one element carrying `label`, or a refusal that says how many there were.
///
/// **Exactly one, never "the first".** A driver that acted on the first match
/// would act on whichever the walk reached first, which is an ordering nothing
/// promises -- so two matches is as much a failure as none, and the message
/// says which so the driver's dump has somewhere to start.
func single(pid: pid_t, label: String, what: String) -> AXUIElement? {
    let found = matching(pid: pid, label: label)
    guard found.count == 1 else {
        FileHandle.standardError.write(
            Data("ax: \(found.count) elements labelled '\(label)' to \(what), wanted 1\n".utf8))
        return nil
    }
    return found[0]
}

/// Give the keyboard to the element carrying `label`.
///
/// `AXFocused` rather than a synthetic click: a click needs a screen position,
/// and a position needs a layout this harness has no business knowing. The
/// focus is what the typing that follows depends on, and it is directly
/// observable afterwards through `ax focused`.
func focus(pid: pid_t, label: String) -> Int32 {
    guard let target = single(pid: pid, label: label, what: "focus") else { return 1 }
    let status = AXUIElementSetAttributeValue(
        target, kAXFocusedAttribute as CFString, kCFBooleanTrue)
    guard status == .success else {
        FileHandle.standardError.write(
            Data("ax: could not focus '\(label)' (AXError \(status.rawValue))\n".utf8))
        return 1
    }
    return 0
}

/// Press the element carrying `label` -- a button, or anything else that
/// answers `AXPress`.
func press(pid: pid_t, label: String) -> Int32 {
    guard let target = single(pid: pid, label: label, what: "press") else { return 1 }
    let status = AXUIElementPerformAction(target, kAXPressAction as CFString)
    guard status == .success else {
        FileHandle.standardError.write(
            Data("ax: could not press '\(label)' (AXError \(status.rawValue))\n".utf8))
        return 1
    }
    return 0
}

/// Every string **value** under this app's windows, one per line.
///
/// The third way an element can carry text, and the one `find` deliberately
/// does not read: `AXDescription` and `AXTitle` are what an `aria-label` and a
/// button's own words arrive as, and `AXValue` is what a paragraph of rendered
/// text and a text field's contents arrive as. `find` counts *named* elements,
/// which is what a driver presses and focuses; this prints *what is written on
/// the screen*, which is what a driver reads back.
///
/// Kept apart rather than folded into `find` on purpose. Widening `find` to
/// match values would change what every existing driver's "exactly one" means
/// -- a field whose value happened to equal its own label would suddenly be two
/// -- and those counts are load-bearing (`open-in-editor`'s `exactly_one`).
///
/// One line per value, with empty ones dropped: the caller matches whole lines,
/// because the readings this exists for come in pairs where one contains the
/// other ("captured from", "captured from here") and a substring test would
/// accept the wrong end of the link.
func values(pid: pid_t) -> Int32 {
    for window in windows(of: pid) {
        visit(window, maxDepth: 25) { element, _ in
            guard let value = attribute(element, kAXValueAttribute as String) as? String,
                !value.isEmpty
            else { return }
            print(value)
        }
    }
    return 0
}

/// The frontmost application: its pid and its bundle identifier, tab-separated.
///
/// What makes a *global* shortcut's witness a witness. Every other assertion in
/// these drivers is about knobas' own window, and this is the one that says the
/// keystroke was sent while somebody else had the screen -- without it a driver
/// would prove only that a key works in the application that is already
/// listening for keys.
func frontmost() -> Int32 {
    guard let app = NSWorkspace.shared.frontmostApplication else {
        FileHandle.standardError.write(Data("ax: no application is frontmost\n".utf8))
        return 1
    }
    print("pid\t\(app.processIdentifier)")
    print("bundle\t\(app.bundleIdentifier ?? "")")
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

/// Type `string` into whatever has the keyboard, character by character.
///
/// `keyboardSetUnicodeString` rather than a keycode table: a keycode is a
/// position on a keyboard layout, and this Mac's layout is not something a
/// driver may assume. A filesystem path is full of `/`, `-` and `.`, all of
/// which move between layouts; the unicode string does not.
///
/// Posted through the HID tap, like `key`, so the characters go where a
/// person's would -- to the frontmost application, whose window the driver has
/// already brought forward.
func type(_ string: String) -> Int32 {
    guard let source = CGEventSource(stateID: .hidSystemState) else {
        FileHandle.standardError.write(Data("ax: no event source\n".utf8))
        return 1
    }
    for character in string {
        let utf16 = Array(String(character).utf16)
        for isDown in [true, false] {
            guard let event = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: isDown)
            else {
                FileHandle.standardError.write(Data("ax: could not build a key event\n".utf8))
                return 1
            }
            event.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: utf16)
            event.post(tap: .cghidEventTap)
            // The webview's input handler runs on its own turn; typing faster
            // than it can read has dropped characters on every framework that
            // has ever been driven this way.
            Thread.sleep(forTimeInterval: 0.012)
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
func key(code: CGKeyCode, modifiers: [String]) -> Int32 {
    guard let source = CGEventSource(stateID: .hidSystemState) else {
        FileHandle.standardError.write(Data("ax: no event source\n".utf8))
        return 1
    }
    var flags: CGEventFlags = []
    for modifier in modifiers {
        switch modifier {
        case "command": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "option": flags.insert(.maskAlternate)
        case "control": flags.insert(.maskControl)
        default:
            FileHandle.standardError.write(Data("ax: unknown modifier '\(modifier)'\n".utf8))
            return 1
        }
    }
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
case "values" where arguments.count == 2 && integer(1) != nil:
    status = values(pid: pid_t(integer(1)!))
case "frontmost" where arguments.count == 1:
    status = frontmost()
case "dump" where arguments.count == 3 && integer(1) != nil && integer(2) != nil:
    status = dump(pid: pid_t(integer(1)!), depth: integer(2)!)
case "focus" where arguments.count == 3 && integer(1) != nil:
    status = focus(pid: pid_t(integer(1)!), label: arguments[2])
case "press" where arguments.count == 3 && integer(1) != nil:
    status = press(pid: pid_t(integer(1)!), label: arguments[2])
case "type" where arguments.count == 2:
    status = type(arguments[1])
// Every modifier is spelled out and an unknown word is refused inside `key`,
// so a typo is a refusal instead of a keystroke sent without ⌘. More than one
// is allowed since #503: a capture shortcut a person would actually choose has
// two or three, and a driver that could only send one would be witnessing a
// combination nobody sets.
case "key" where arguments.count >= 2 && integer(1) != nil:
    status = key(code: CGKeyCode(integer(1)!), modifiers: Array(arguments.dropFirst(2)))
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
                   ax values <pid>
                   ax frontmost
                   ax dump <pid> <depth>
                   ax focus <pid> <label>
                   ax press <pid> <label>
                   ax type <text>
                   ax key <keycode> [command|shift|option|control ...]

            """.utf8))
    status = 2
}
exit(status)
