// wkhost: a bare WKWebView window on a URL, to see what the system webview does with no Tauri around it.
//
//   wkhost URL [--uncap] [--size W H]
//
// --uncap turns off WebKit's internal feature PreferPageRenderingUpdatesNear60FPSEnabled through the private
// +[WKPreferences _features] / -[WKPreferences _setEnabled:forFeature:], before the web view is made. It prints
// whether the feature was found and what it read before and after, then runs until killed.
// The window is 1440 x 900 at 40,40 unless --size says otherwise. The web view is inspectable.
import AppKit
import WebKit

var args = Array(CommandLine.arguments.dropFirst())
func flag(_ name: String) -> Bool { if let i = args.firstIndex(of: name) { args.remove(at: i); return true }; return false }
let uncap = flag("--uncap")
var size = CGSize(width: 1440, height: 900)
if let i = args.firstIndex(of: "--size"), i + 2 < args.count, let w = Double(args[i + 1]), let h = Double(args[i + 2]) { size = CGSize(width: w, height: h); args.removeSubrange(i...i + 2) }
guard let target = args.first.flatMap({ URL(string: $0) }) else { FileHandle.standardError.write("usage: wkhost URL [--uncap] [--size W H]\n".data(using: .utf8)!); exit(2) }
setvbuf(stdout, nil, _IOLBF, 0)

final class Delegate: NSObject, NSApplicationDelegate {
  var window: NSWindow!
  var web: WKWebView!
  var url: URL
  var uncap: Bool
  var size: CGSize
  init(url: URL, uncap: Bool, size: CGSize) { self.url = url; self.uncap = uncap; self.size = size }
  func applicationDidFinishLaunching(_ note: Notification) {
    let config = WKWebViewConfiguration()
    let prefs = config.preferences
    let key = "PreferPageRenderingUpdatesNear60FPSEnabled"
    let featuresSel = NSSelectorFromString("_features")
    let isEnabledSel = NSSelectorFromString("_isEnabledForFeature:")
    let setSel = NSSelectorFromString("_setEnabled:forFeature:")
    var found = false
    if (WKPreferences.self as AnyObject).responds(to: featuresSel),
       let list = (WKPreferences.self as AnyObject).perform(featuresSel)?.takeUnretainedValue() as? [NSObject] {
      for feature in list where (feature.value(forKey: "key") as? String) == key {
        found = true
        typealias IsEnabled = @convention(c) (AnyObject, Selector, AnyObject) -> Bool
        typealias SetEnabled = @convention(c) (AnyObject, Selector, Bool, AnyObject) -> Void
        let read = unsafeBitCast(prefs.method(for: isEnabledSel), to: IsEnabled.self)
        let before = prefs.responds(to: isEnabledSel) ? read(prefs, isEnabledSel, feature) : true
        if uncap, prefs.responds(to: setSel) {
          unsafeBitCast(prefs.method(for: setSel), to: SetEnabled.self)(prefs, setSel, false, feature)
        }
        let after = prefs.responds(to: isEnabledSel) ? read(prefs, isEnabledSel, feature) : true
        print("wkhost: \(key) found among \(list.count) features; enabled before \(before), after \(after)")
      }
    }
    if !found { print("wkhost: \(key) not found (the private list is missing or the key was renamed)") }
    web = WKWebView(frame: NSRect(origin: .zero, size: size), configuration: config)
    if #available(macOS 13.3, *) { web.isInspectable = true }
    window = NSWindow(contentRect: NSRect(x: 40, y: 40, width: size.width, height: size.height), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
    window.title = uncap ? "wkhost (uncapped)" : "wkhost"
    window.contentView = web
    window.setFrameTopLeftPoint(NSPoint(x: 40, y: (NSScreen.main?.frame.height ?? 1000) - 40))
    window.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    web.load(URLRequest(url: url))
    print("wkhost: pid \(ProcessInfo.processInfo.processIdentifier) loading \(url)")
  }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = Delegate(url: target, uncap: uncap, size: size)
app.delegate = delegate
app.run()
