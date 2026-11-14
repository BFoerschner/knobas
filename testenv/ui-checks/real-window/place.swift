// place <pid> X Y W H : move and size the app's main window through the accessibility API
import ApplicationServices
let a = CommandLine.arguments
let app = AXUIElementCreateApplication(pid_t(a[1])!)
var wins: CFTypeRef?
AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &wins)
guard let list = wins as? [AXUIElement], let w = list.first else { print("no window"); exit(1) }
var p = CGPoint(x: Double(a[2])!, y: Double(a[3])!), s = CGSize(width: Double(a[4])!, height: Double(a[5])!)
AXUIElementSetAttributeValue(w, kAXPositionAttribute as CFString, AXValueCreate(.cgPoint, &p)!)
AXUIElementSetAttributeValue(w, kAXSizeAttribute as CFString, AXValueCreate(.cgSize, &s)!)
AXUIElementSetAttributeValue(w, kAXPositionAttribute as CFString, AXValueCreate(.cgPoint, &p)!)
print("placed")
