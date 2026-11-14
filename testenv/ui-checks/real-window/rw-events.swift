// Real macOS input for the knobas window: CGEvent mouse moves, presses, drags,
// wheels and keys, posted at the HID level, so the window server routes them
// as it would a person's. Coordinates are the page's CSS pixels (the webview's
// top-left is 0,0); the window's place on screen is read from the window list.
//
// Usage: rw-events <pid> <command>...
//   front                       bring the app forward
//   move X Y [STEPS]            glide from where the pointer is to X,Y
//   click X Y [COUNT]           press and let go (COUNT 2 is a double press)
//   down X Y | up X Y           half a press
//   drag X1 Y1 X2 Y2 [STEPS]    press, glide, let go
//   wheel DY X Y [N]            N wheel notches of DY pixels over X,Y (positive scrolls down)
//   key NAME [MODS]             a key, MODS any of cmd,shift,alt,ctrl joined by +
//   type TEXT                   each character as a key press
//   sleep S
//   where                       print the content origin
import AppKit
import CoreGraphics

let args = Array(CommandLine.arguments.dropFirst())
guard let pid = args.first.flatMap({ pid_t($0) }) else {
  FileHandle.standardError.write("usage: rw-events <pid> <command>...\n".data(using: .utf8)!)
  exit(2)
}

func contentOrigin() -> CGPoint {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  let mine = list.filter { ($0[kCGWindowOwnerPID as String] as? pid_t) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
  let biggest = mine.max { a, b in
    let ra = CGRect(dictionaryRepresentation: a[kCGWindowBounds as String] as! CFDictionary)!
    let rb = CGRect(dictionaryRepresentation: b[kCGWindowBounds as String] as! CFDictionary)!
    return ra.width * ra.height < rb.width * rb.height
  }
  guard let w = biggest, let r = CGRect(dictionaryRepresentation: w[kCGWindowBounds as String] as! CFDictionary) else {
    FileHandle.standardError.write("rw-events: no window for pid \(pid)\n".data(using: .utf8)!)
    exit(1)
  }
  // The webview fills the window below the title bar. RW_INNER_H is the page's
  // innerHeight, so the title bar is whatever is left over.
  let innerH = Double(ProcessInfo.processInfo.environment["RW_INNER_H"] ?? "") ?? Double(r.height - 28)
  return CGPoint(x: r.minX, y: r.maxY - innerH)
}

let origin = contentOrigin()
let src = CGEventSource(stateID: .hidSystemState)
func at(_ x: Double, _ y: Double) -> CGPoint { CGPoint(x: origin.x + x, y: origin.y + y) }
func now() -> CGPoint { CGEvent(source: nil)?.location ?? .zero }
func post(_ e: CGEvent?) { e?.post(tap: .cghidEventTap) }
func pause(_ s: Double) { usleep(useconds_t(s * 1_000_000)) }
var buttonDown = false

func glide(to p: CGPoint, steps: Int) {
  let from = now()
  let n = max(1, steps)
  for i in 1...n {
    let t = Double(i) / Double(n)
    let q = CGPoint(x: from.x + (p.x - from.x) * t, y: from.y + (p.y - from.y) * t)
    let e = CGEvent(mouseEventSource: src, mouseType: buttonDown ? .leftMouseDragged : .mouseMoved, mouseCursorPosition: q, mouseButton: .left)
    e?.setIntegerValueField(.mouseEventDeltaX, value: Int64((p.x - from.x) / Double(n)))
    e?.setIntegerValueField(.mouseEventDeltaY, value: Int64((p.y - from.y) / Double(n)))
    post(e)
    pause(0.012)
  }
}

func press(_ p: CGPoint, count: Int) {
  for c in 1...count {
    let d = CGEvent(mouseEventSource: src, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)
    d?.setIntegerValueField(.mouseEventClickState, value: Int64(c))
    post(d); pause(0.04)
    let u = CGEvent(mouseEventSource: src, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)
    u?.setIntegerValueField(.mouseEventClickState, value: Int64(c))
    post(u); pause(0.06)
  }
}

let keys: [String: CGKeyCode] = [
  "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9, "b": 11, "q": 12, "w": 13, "e": 14, "r": 15,
  "y": 16, "t": 17, "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25, "7": 26, "-": 27, "8": 28, "0": 29,
  "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35, "l": 37, "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42, ",": 43, "/": 44,
  "n": 45, "m": 46, ".": 47, "`": 50, " ": 49,
  "Return": 36, "Enter": 36, "Tab": 48, "Space": 49, "Delete": 51, "Backspace": 51, "Escape": 53, "Esc": 53,
  "Home": 115, "PageUp": 116, "ForwardDelete": 117, "End": 119, "PageDown": 121,
  "Left": 123, "Right": 124, "Down": 125, "Up": 126,
]

func key(_ name: String, mods: String) {
  guard let code = keys[name] ?? keys[name.lowercased()] else { print("rw-events: no key \(name)"); return }
  var flags: CGEventFlags = []
  for m in mods.split(separator: "+") {
    switch m { case "cmd": flags.insert(.maskCommand); case "shift": flags.insert(.maskShift)
    case "alt": flags.insert(.maskAlternate); case "ctrl": flags.insert(.maskControl); default: break }
  }
  if name.count == 1, name.uppercased() == name, name.lowercased() != name { flags.insert(.maskShift) }
  let d = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: true); d?.flags = flags; post(d); pause(0.03)
  let u = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: false); u?.flags = flags; post(u); pause(0.05)
}

var i = 1
func num(_ k: Int) -> Double { Double(args[k]) ?? 0 }
while i < args.count {
  let c = args[i]
  switch c {
  case "front":
    NSRunningApplication(processIdentifier: pid)?.activate(options: [])
    pause(0.4); i += 1
  case "where":
    print("origin \(origin.x) \(origin.y)"); i += 1
  case "move":
    let steps = (i + 3 < args.count ? Int(args[i + 3]) : nil)
    glide(to: at(num(i + 1), num(i + 2)), steps: steps ?? 20); i += steps == nil ? 3 : 4
  case "click":
    let count = (i + 3 < args.count ? Int(args[i + 3]) : nil)
    let p = at(num(i + 1), num(i + 2))
    glide(to: p, steps: 8); press(p, count: count ?? 1); i += count == nil ? 3 : 4
  case "down":
    let p = at(num(i + 1), num(i + 2)); glide(to: p, steps: 8)
    post(CGEvent(mouseEventSource: src, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)); buttonDown = true; i += 3
  case "up":
    let p = at(num(i + 1), num(i + 2)); glide(to: p, steps: 8)
    post(CGEvent(mouseEventSource: src, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)); buttonDown = false; i += 3
  case "drag":
    let steps = (i + 5 < args.count ? Int(args[i + 5]) : nil)
    let a = at(num(i + 1), num(i + 2)), b = at(num(i + 3), num(i + 4))
    glide(to: a, steps: 8)
    post(CGEvent(mouseEventSource: src, mouseType: .leftMouseDown, mouseCursorPosition: a, mouseButton: .left)); buttonDown = true; pause(0.08)
    glide(to: b, steps: steps ?? 25); pause(0.05)
    post(CGEvent(mouseEventSource: src, mouseType: .leftMouseUp, mouseCursorPosition: b, mouseButton: .left)); buttonDown = false
    i += steps == nil ? 5 : 6
  case "wheel":
    let n = (i + 4 < args.count ? Int(args[i + 4]) : nil)
    glide(to: at(num(i + 2), num(i + 3)), steps: 6)
    for _ in 0..<(n ?? 1) {
      // Pixel units, negative is down in CGEvent's sense.
      let e = CGEvent(scrollWheelEvent2Source: src, units: .pixel, wheelCount: 1, wheel1: Int32(-num(i + 1)), wheel2: 0, wheel3: 0)
      post(e); pause(0.03)
    }
    i += n == nil ? 4 : 5
  case "key":
    let modNames: Set<Substring> = ["cmd", "shift", "alt", "ctrl"]
    let mods = (i + 2 < args.count && !args[i + 2].isEmpty && args[i + 2].split(separator: "+").allSatisfy { modNames.contains($0) }) ? args[i + 2] : ""
    key(args[i + 1], mods: mods); i += mods.isEmpty ? 2 : 3
  case "type":
    for ch in args[i + 1] { key(String(ch), mods: "") }
    i += 2
  case "sleep":
    pause(num(i + 1)); i += 2
  default:
    print("rw-events: unknown command \(c)"); exit(2)
  }
}
