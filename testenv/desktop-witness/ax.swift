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
    // front when it is posted, so waiting here is not a politeness. Under the
    // harness's guard, KNOBAS_WITNESS_WAIT_S (60 s; `witness_guard` refuses a
    // value that is not whole seconds before this runs), not a fixed 5 s: a
    // loaded machine brings a window forward slowly (#1961).
    let guardSeconds = Double(ProcessInfo.processInfo.environment["KNOBAS_WITNESS_WAIT_S"] ?? "") ?? 60
    let deadline = Date().addingTimeInterval(guardSeconds)
    while Date() < deadline {
        if app.isActive { return 0 }
        Thread.sleep(forTimeInterval: 0.1)
    }
    FileHandle.standardError.write(
        Data("ax: pid \(pid) did not become frontmost within \(Int(guardSeconds)) s (KNOBAS_WITNESS_WAIT_S)\n".utf8))
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

/// Close the application's one window with its close button (#1084).
///
/// For the window a driver made by accident of how macOS hands over the
/// screen: `open -a Finder` on a Finder with no window open makes one, and the
/// capture driver needs `open` because an application a background process
/// merely activates is refused the screen. By the window's
/// `AXCloseButton` attribute, because that button carries no name to `press`.
///
/// **Exactly one window or nothing**, like `single`: with a second one -- a
/// window the reader opened meanwhile -- which to close is not this helper's
/// guess to make.
func closeWindow(pid: pid_t) -> Int32 {
    let found = windows(of: pid)
    guard found.count == 1 else {
        FileHandle.standardError.write(
            Data("ax: pid \(pid) has \(found.count) windows, wanted 1 to close\n".utf8))
        return 1
    }
    guard let button = element(found[0], kAXCloseButtonAttribute as String) else {
        FileHandle.standardError.write(Data("ax: the window of pid \(pid) has no close button\n".utf8))
        return 1
    }
    let status = AXUIElementPerformAction(button, kAXPressAction as CFString)
    guard status == .success else {
        FileHandle.standardError.write(
            Data("ax: could not press the close button (AXError \(status.rawValue))\n".utf8))
        return 1
    }
    return 0
}

/// The application's windows, one per line: `window<tab><its title>`.
///
/// **The question `find` cannot answer, which is the whole of #548.**
/// `matching()` walks every element under every window and counts the two
/// attributes a *name* can arrive in. That is the right question for a button
/// and the wrong one for a window: the capture window puts the word `Capture`
/// on three elements at once -- the `AXWindow`'s title from `capture.rs`'s
/// `.title("Capture")`, the `AXWebArea`'s description from `capture.html`'s
/// `<title>`, and the `AXTextArea`'s from `CaptureWindow.svelte`'s
/// `aria-label` -- and each of the three is correct. A driver asking *is that
/// window open?* wants the windows, not the named elements underneath them.
///
/// The key is repeated rather than numbered, in the same tab-separated shape
/// every other subcommand answers in: what a caller does with this is count
/// the windows answering to a title, and a repeated key needs no parser. An
/// untitled window still prints its line -- `window<tab>` -- so a list cannot
/// lose its last entry to `$( )`'s trailing-newline strip, and a caller can
/// tell *no windows* from *one window with no title*.
///
/// A refusal when there are none, like `dump`'s. No window at all is what a
/// locked screen, a quit app and a pid that was never an application all look
/// like, and a caller that read an empty list as *the window has closed* would
/// pass its closing assertion on an app that had died mid-run.
///
/// `windowTitles` rather than `windows`, which is the subcommand's name and is
/// already taken by the element lookup every other subcommand is built on --
/// the same reason `wait-window` is `waitForWindow`.
func windowTitles(pid: pid_t) -> Int32 {
    let found = windows(of: pid)
    if found.isEmpty {
        FileHandle.standardError.write(
            Data("ax: pid \(pid) exposes no window (it has none, or the screen is locked)\n".utf8))
        return 1
    }
    for window in found {
        print("window\t\(text(window, kAXTitleAttribute as String))")
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
            // **No modifiers, explicitly.** An event built from
            // `.hidSystemState` inherits the session's current modifier flags,
            // and a synthetic `key` posted just before this one leaves ⌘
            // *held* as far as that state is concerned -- there is no
            // `flagsChanged` to let it go. Measured on 2026-09-08: a driver
            // that pressed ⌘A to select a settings field and then typed
            // `CmdOrCtrl+Alt+Shift+K` into it sent ⌘C, ⌘A, ⌘S … ⌘K, and the
            // last of those opened the launcher over the pane it was typing
            // in. Clearing the flags per event makes typing mean typing
            // whatever was pressed before it.
            event.flags = []
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

// MARK: - The screenshot driver's reach (#1483)
//
// `testenv/live-shot.sh` drives the app by hand-written steps rather than a
// fixed scenario, so it needs what the witness's drivers never did: the one
// match a person can *see*, a pointer click for controls only a hover reveals,
// and the window to photograph. The witness's own `press` and `focus` above
// are left as they are; their drivers count on exactly-one-in-the-tree.

func fail(_ message: String) -> Int32 {
    FileHandle.standardError.write(Data("ax: \(message)\n".utf8))
    return 1
}

/// An element's frame in screen points (top-left origin, as CGEvent and the
/// window list use), or `nil` when it carries none.
func frame(_ element: AXUIElement) -> CGRect? {
    guard let position = attribute(element, kAXPositionAttribute as String),
        let size = attribute(element, kAXSizeAttribute as String),
        CFGetTypeID(position) == AXValueGetTypeID(), CFGetTypeID(size) == AXValueGetTypeID()
    else { return nil }
    var point = CGPoint.zero
    var extent = CGSize.zero
    guard AXValueGetValue(position as! AXValue, .cgPoint, &point),
        AXValueGetValue(size as! AXValue, .cgSize, &extent)
    else { return nil }
    return CGRect(origin: point, size: extent)
}

/// The elements labelled `label` -- by description, title or placeholder --
/// that a person could see: a frame with area, inside one of the app's
/// windows. WebKit's tree reaches a table cell twice and keeps the rows a
/// filter hid, so matches sharing one frame count once and those outside every
/// window not at all. Tree order, which is reading order. `prose` also takes a
/// run of static text whose value is `label`: a pointer can click words (a
/// radio's label, a list row's name) that accessibility cannot press.
/// `inside: false` takes the matches with a box outside every window instead.
func visibleMatching(pid: pid_t, label: String, prose: Bool = false, inside: Bool = true) -> [(AXUIElement, CGRect)] {
    let names = [kAXDescriptionAttribute as String, kAXTitleAttribute as String, "AXPlaceholderValue"]
    let isProse = { (element: AXUIElement) in
        prose && text(element, kAXRoleAttribute as String) == (kAXStaticTextRole as String)
            && text(element, kAXValueAttribute as String) == label
    }
    var found: [(AXUIElement, CGRect)] = []
    for window in windows(of: pid) {
        guard let bounds = frame(window) else { continue }
        visit(window, maxDepth: 40) { element, _ in
            guard names.contains(where: { text(element, $0) == label }) || isProse(element),
                let box = frame(element), box.width > 0, box.height > 0,
                bounds.intersects(box) == inside, !found.contains(where: { $0.1 == box })
            else { return }
            found.append((element, box))
        }
    }
    return found
}

/// The `nth` visible match (from 1), or the only one when `nth` is `nil`,
/// scrolled into view first as a person would. With none in the window, the
/// matches with a box outside it count instead: a dialog field below its
/// scroller's fold (measured 2026-10-21: *Add a source*'s Token field under
/// Gitea's configuration).
func pick(pid: pid_t, label: String, nth: Int?, what: String, prose: Bool = false) -> (AXUIElement, CGRect)? {
    var found = visibleMatching(pid: pid, label: label, prose: prose)
    if found.isEmpty { found = visibleMatching(pid: pid, label: label, prose: prose, inside: false) }
    guard let (element, box) = choose(found, label: label, nth: nth, what: what) else { return nil }
    AXUIElementPerformAction(element, "AXScrollToVisible" as CFString)
    Thread.sleep(forTimeInterval: 0.3)
    return (element, frame(element) ?? box)
}

func choose(_ found: [(AXUIElement, CGRect)], label: String, nth: Int?, what: String) -> (AXUIElement, CGRect)? {
    if let nth = nth {
        guard nth >= 1, nth <= found.count else {
            _ = fail("\(found.count) visible elements labelled '\(label)' to \(what), wanted number \(nth)")
            return nil
        }
        return found[nth - 1]
    }
    guard found.count == 1 else {
        _ = fail("\(found.count) visible elements labelled '\(label)' to \(what), wanted 1 (add <n> to pick one)")
        return nil
    }
    return found[0]
}

func pressVisible(pid: pid_t, label: String, nth: Int?) -> Int32 {
    guard let (target, _) = pick(pid: pid, label: label, nth: nth, what: "press") else { return 1 }
    let status = AXUIElementPerformAction(target, kAXPressAction as CFString)
    return status == .success ? 0 : fail("could not press '\(label)' (AXError \(status.rawValue))")
}

func focusVisible(pid: pid_t, label: String, nth: Int?) -> Int32 {
    guard let (target, _) = pick(pid: pid, label: label, nth: nth, what: "focus") else { return 1 }
    let status = AXUIElementSetAttributeValue(target, kAXFocusedAttribute as CFString, kCFBooleanTrue)
    return status == .success ? 0 : fail("could not focus '\(label)' (AXError \(status.rawValue))")
}

/// A left click with the pointer at the element's centre, for a control that
/// only a hover reveals and accessibility cannot press, or for words on screen. Refused unless the
/// app is frontmost: the click lands on whatever window is under the point.
func clickAt(pid: pid_t, label: String, nth: Int?) -> Int32 {
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else {
        return fail("pid \(pid) is not frontmost; not clicking into another app")
    }
    guard let (_, box) = pick(pid: pid, label: label, nth: nth, what: "click", prose: true) else { return 1 }
    let point = CGPoint(x: box.midX, y: box.midY)
    for type in [CGEventType.mouseMoved, .leftMouseDown, .leftMouseUp] {
        guard let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)
        else { return fail("could not build a mouse event") }
        event.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: type == .mouseMoved ? 0.15 : 0.03)
    }
    return 0
}

/// Waits until a window's web area holds anything. A window is up long before
/// the webview has drawn the page (measured 2026-10-21 on `just demo`: an empty
/// AXWebArea and a blank window three seconds after the window appeared).
func waitContent(pid: pid_t, seconds: Double) -> Int32 {
    let deadline = Date().addingTimeInterval(seconds)
    while Date() < deadline {
        var drawn = false
        for window in windows(of: pid) {
            visit(window, maxDepth: 6) { element, _ in
                if text(element, kAXRoleAttribute as String) == "AXWebArea",
                    !children(element, kAXChildrenAttribute as String).isEmpty
                {
                    drawn = true
                }
            }
        }
        if drawn { return 0 }
        Thread.sleep(forTimeInterval: 0.5)
    }
    return fail("pid \(pid)'s web view drew nothing within \(Int(seconds)) s")
}

/// The largest of several framed things, by area: the window `window-id`
/// measures and the one `resize` sizes are picked the same way.
func largest<T>(_ framed: [(T, CGRect)]) -> (T, CGRect)? {
    framed.max(by: { $0.1.width * $0.1.height < $1.1.width * $1.1.height })
}

/// The app's largest on-screen window, as the window server knows it:
/// `<window id>\t<width>\t<height>` in points, for `screencapture -l`.
func windowID(pid: pid_t) -> Int32 {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? []
    let mine = list.compactMap { info -> (Int, CGRect)? in
        guard (info[kCGWindowOwnerPID as String] as? Int) == Int(pid),
            (info[kCGWindowLayer as String] as? Int) == 0,
            let id = info[kCGWindowNumber as String] as? Int,
            let raw = info[kCGWindowBounds as String] as? NSDictionary,
            let bounds = CGRect(dictionaryRepresentation: raw)
        else { return nil }
        return (id, bounds)
    }
    guard let (id, bounds) = largest(mine) else {
        return fail("pid \(pid) has no on-screen window (none, another Space, or the screen is locked)")
    }
    print("\(id)\t\(Int(bounds.width))\t\(Int(bounds.height))")
    return 0
}

/// Sets the app's largest window to `width` x `height` points (#2056): the
/// window-size step widens a window a window manager left too narrow. The
/// window manager may refuse or undo it, so the caller reads the size again.
func resize(pid: pid_t, width: Int, height: Int) -> Int32 {
    let framed = windows(of: pid).compactMap { window in frame(window).map { (window, $0) } }
    guard let (window, _) = largest(framed) else {
        return fail("pid \(pid) has no window to resize")
    }
    var size = CGSize(width: width, height: height)
    guard let value = AXValueCreate(.cgSize, &size) else { return fail("could not make a size value") }
    let result = AXUIElementSetAttributeValue(window, kAXSizeAttribute as CFString, value)
    return result == .success ? 0 : fail("the window refused the size \(width)x\(height) (AXError \(result.rawValue))")
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

/// `<subcommand> <pid> <label> [n]`, each number a number.
let labelled = integer(1) != nil
    && (arguments.count == 3 || (arguments.count == 4 && integer(3) != nil))

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
case "windows" where arguments.count == 2 && integer(1) != nil:
    status = windowTitles(pid: pid_t(integer(1)!))
case "close-window" where arguments.count == 2 && integer(1) != nil:
    status = closeWindow(pid: pid_t(integer(1)!))
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
// The screenshot driver's (#1483): `<pid> <label> [n]`, n picking the n-th
// visible match.
case "press-visible" where labelled:
    status = pressVisible(pid: pid_t(integer(1)!), label: arguments[2], nth: integer(3))
case "focus-visible" where labelled:
    status = focusVisible(pid: pid_t(integer(1)!), label: arguments[2], nth: integer(3))
case "click-at" where labelled:
    status = clickAt(pid: pid_t(integer(1)!), label: arguments[2], nth: integer(3))
case "window-id" where arguments.count == 2 && integer(1) != nil:
    status = windowID(pid: pid_t(integer(1)!))
case "wait-content" where arguments.count == 3 && integer(1) != nil && seconds(2) != nil:
    status = waitContent(pid: pid_t(integer(1)!), seconds: seconds(2)!)
case "resize" where arguments.count == 4 && integer(1) != nil && (integer(2) ?? 0) > 0 && (integer(3) ?? 0) > 0:
    status = resize(pid: pid_t(integer(1)!), width: integer(2)!, height: integer(3)!)
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
                   ax windows <pid>
                   ax close-window <pid>
                   ax frontmost
                   ax dump <pid> <depth>
                   ax focus <pid> <label>
                   ax press <pid> <label>
                   ax type <text>
                   ax key <keycode> [command|shift|option|control ...]
                   ax press-visible <pid> <label> [n]
                   ax focus-visible <pid> <label> [n]
                   ax click-at <pid> <label> [n]
                   ax window-id <pid>
                   ax wait-content <pid> <seconds>
                   ax resize <pid> <width> <height>

            """.utf8))
    status = 2
}
exit(status)
