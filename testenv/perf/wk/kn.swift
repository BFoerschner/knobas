// kn: real scroll input for a window, and the frames the window server actually showed of it.
//
//   kn scroll <pid> X Y PX_PER_SEC SECONDS [--hz N] [--hid] [--capture] [--momentum] [--notches]
//       A two-finger scroll over page point X,Y (CSS pixels; RW_INNER_H = the page's innerHeight): wheel events
//       with scroll phases (began, changed..., ended), one a display refresh (from a display link), each the
//       pixels PX_PER_SEC covers in one refresh. --hz N posts them from a timer at N a second instead.
//       --raw       with --capture: also every shown frame as gap:step (ms since the frame before : pixels moved)
//       --hid       posts at the HID tap and moves the pointer there first (what a person does). Without it the
//                   events go to the app's process only (CGEvent.postToPid) and the pointer stays where it is.
//       --momentum  after the fingers lift, a momentum phase that decays over 1.2 s
//       --notches   plain wheel events with no phase (a mouse wheel), 30 a second
//       --lines N   a wheel mouse: line-unit events of N lines with no phase; PX_PER_SEC is then notches a second
//       --capture   records the window through ScreenCaptureKit while it scrolls and prints the shown frames
//   kn capture <pid> SECONDS      only the recording: for a scroll the page drives itself
//   --strip X Y0 Y1               with a recording: follow the column of pixels at page point X from row Y0 to Y1 and
//                                 say how far its content moved in each shown frame (`moved`: only the frames in
//                                 which the list was somewhere else; `stepPx`: how far it went in one)
//   kn front <pid>                bring the app forward
//   kn windowid <pid>             the id and frame of the process's own window, exit 1 when it has none
//   kn idle                       seconds since the last real input
//
// The recording: ScreenCaptureKit delivers a frame of the window with status `complete` each time the window's
// content changed on screen, with the time it was displayed, at up to 120 a second. So the gaps between complete
// frames while the list moves are the frame times a person sees, whatever thread or process drew them.
import AppKit
import CoreGraphics
import CoreMedia
import ScreenCaptureKit

setvbuf(stdout, nil, _IOLBF, 0)
// A command-line tool has no window server connection until AppKit makes one; ScreenCaptureKit asserts on it.
_ = NSApplication.shared
var args = Array(CommandLine.arguments.dropFirst())
func fail(_ s: String) -> Never { FileHandle.standardError.write((s + "\n").data(using: .utf8)!); exit(2) }
func flag(_ name: String) -> Bool { if let i = args.firstIndex(of: name) { args.remove(at: i); return true }; return false }
func option(_ name: String) -> String? { if let i = args.firstIndex(of: name), i + 1 < args.count { let v = args[i + 1]; args.removeSubrange(i...i + 1); return v }; return nil }

var timebase = mach_timebase_info_data_t()
mach_timebase_info(&timebase)
func seconds(_ mach: UInt64) -> Double { Double(mach) * Double(timebase.numer) / Double(timebase.denom) / 1e9 }
func nowS() -> Double { seconds(mach_absolute_time()) }

func windowBounds(_ pid: pid_t) -> CGRect {
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  let mine = list.filter { ($0[kCGWindowOwnerPID as String] as? pid_t) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
  let rects = mine.compactMap { CGRect(dictionaryRepresentation: $0[kCGWindowBounds as String] as! CFDictionary) }
  guard let r = rects.max(by: { $0.width * $0.height < $1.width * $1.height }) else { fail("kn: no window for pid \(pid)") }
  return r
}
func contentOrigin(_ pid: pid_t) -> CGPoint {
  let r = windowBounds(pid)
  let innerH = Double(ProcessInfo.processInfo.environment["RW_INNER_H"] ?? "") ?? Double(r.height - 28)
  return CGPoint(x: r.minX, y: r.maxY - innerH)
}

/** A column of the window to follow: page point X, rows Y0 to Y1 (CSS pixels). */
struct Strip { var x: Double; var y0: Double; var y1: Double }
var stripSpec: Strip? = nil

final class Recorder: NSObject, SCStreamOutput, SCStreamDelegate {
  var stream: SCStream?
  var shown: [Double] = []  // display times of complete frames, seconds
  /** For each complete frame: how far the strip's content moved since the frame before (pixels, + is content moving up, i.e. scrolling down), nil when it did not change at all. */
  var moved: [Double?] = []
  var prev: [Int32] = []
  var idle = 0
  var other = 0
  var windowSize = CGSize.zero
  var titleBar = 0.0
  let lock = NSLock()
  func start(pid: pid_t) async throws {
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    let mine = content.windows.filter { $0.owningApplication?.processID == pid && $0.windowLayer == 0 }
    guard let window = mine.max(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }) else { fail("kn: no window to record for pid \(pid)") }
    windowSize = window.frame.size
    let innerH = Double(ProcessInfo.processInfo.environment["RW_INNER_H"] ?? "") ?? Double(window.frame.height - 28)
    titleBar = Double(window.frame.height) - innerH
    let config = SCStreamConfiguration()
    // One buffer pixel a CSS pixel: enough to see a row move, and a quarter of the bytes of the backing store.
    config.width = Int(window.frame.width)
    config.height = Int(window.frame.height)
    config.minimumFrameInterval = CMTime(value: 1, timescale: 120)
    config.queueDepth = 8
    config.showsCursor = false
    config.pixelFormat = kCVPixelFormatType_32BGRA
    let s = SCStream(filter: SCContentFilter(desktopIndependentWindow: window), configuration: config, delegate: self)
    try s.addStreamOutput(self, type: .screen, sampleHandlerQueue: DispatchQueue(label: "kn.frames"))
    try await s.startCapture()
    stream = s
  }
  func stop() async { try? await stream?.stopCapture() }

  /** The strip's pixels of this frame, one value a row. */
  func column(_ buffer: CMSampleBuffer, _ strip: Strip) -> [Int32]? {
    guard let image = CMSampleBufferGetImageBuffer(buffer) else { return nil }
    CVPixelBufferLockBaseAddress(image, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(image, .readOnly) }
    guard let raw = CVPixelBufferGetBaseAddress(image) else { return nil }
    let base = raw.assumingMemoryBound(to: UInt8.self)
    let bpr = CVPixelBufferGetBytesPerRow(image), w = CVPixelBufferGetWidth(image), h = CVPixelBufferGetHeight(image)
    let sx = Double(w) / Double(windowSize.width), sy = Double(h) / Double(windowSize.height)
    let x = min(w - 20, max(20, Int(strip.x * sx)))
    let y0 = min(h - 1, max(0, Int((strip.y0 + titleBar) * sy))), y1 = min(h, max(y0 + 1, Int((strip.y1 + titleBar) * sy)))
    var col = [Int32](repeating: 0, count: y1 - y0)
    for y in y0..<y1 {
      var sum: Int32 = 0
      // Sixteen pixels across, so a row's text is in it, not only its rules.
      for dx in stride(from: -16, through: 16, by: 2) { let at = y * bpr + (x + dx) * 4; sum += Int32(base[at]) + Int32(base[at + 1]) + Int32(base[at + 2]) }
      col[y - y0] = sum
    }
    return col
  }

  /** How far `now` is `prev` moved: the shift with the least difference, nil when nothing changed. */
  func shift(_ prev: [Int32], _ now: [Int32]) -> Double? {
    let n = min(prev.count, now.count)
    guard n > 80 else { return nil }
    var same = true
    for i in 0..<n where abs(prev[i] - now[i]) > 12 { same = false; break }
    if same { return nil }
    let maxShift = min(n - 60, 420)
    var best = 0, bestScore = Double.infinity
    for s in -maxShift...maxShift {
      // now[i] == prev[i + s]: content moved up by s.
      let from = max(0, -s), to = min(n, n - s)
      var sad: Int64 = 0
      var i = from
      while i < to { sad += Int64(abs(now[i] - prev[i + s])); i += 1 }
      let score = Double(sad) / Double(to - from)
      if score < bestScore { bestScore = score; best = s }
    }
    return Double(best)
  }

  func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen, let list = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]], let info = list.first,
          let raw = info[.status] as? Int, let status = SCFrameStatus(rawValue: raw) else { return }
    lock.lock(); defer { lock.unlock() }
    switch status {
    case .complete:
      guard let t = info[.displayTime] as? UInt64 else { return }
      shown.append(seconds(t))
      if let strip = stripSpec, let col = column(buffer, strip) {
        moved.append(prev.isEmpty ? nil : shift(prev, col))
        prev = col
      } else { moved.append(nil) }
    case .idle: idle += 1
    default: other += 1
    }
  }

  /** Every shown frame's gap to the one before (ms) and how far the strip moved in it, as "gap:step" pairs. */
  func steps(from: Double, to: Double) -> String {
    lock.lock(); let all = zip(shown, moved).filter { $0.0 >= from && $0.0 <= to }.sorted { $0.0 < $1.0 }; lock.unlock()
    var parts: [String] = []
    for i in 1..<max(1, all.count) {
      let gap = Int(((all[i].0 - all[i - 1].0) * 1000).rounded())
      parts.append("\(gap):\(all[i].1.map { String(Int($0)) } ?? "-")")
    }
    return parts.joined(separator: " ")
  }

  static func stats(_ values: [Double]) -> (p50: Double, p90: Double, p99: Double, max: Double) {
    let sorted = values.sorted()
    func q(_ p: Double) -> Double { sorted.isEmpty ? 0 : sorted[min(sorted.count - 1, Int(p * Double(sorted.count)))] }
    return (q(0.5), q(0.9), q(0.99), sorted.last ?? 0)
  }

  /** The frames shown from `from` to `to` (seconds on the mach clock), as JSON. */
  func report(from: Double, to: Double, label: String) -> String {
    lock.lock()
    let all = zip(shown, moved).filter { $0.0 >= from && $0.0 <= to }.sorted { $0.0 < $1.0 }
    lock.unlock()
    func r(_ x: Double) -> Double { (x * 10).rounded() / 10 }
    func gapsOf(_ times: [Double]) -> [Double] {
      var gaps: [Double] = []
      if times.count >= 2 { for i in 1..<times.count { gaps.append((times[i] - times[i - 1]) * 1000) } }
      return gaps
    }
    func describe(_ times: [Double]) -> String {
      let gaps = gapsOf(times)
      let s = Recorder.stats(gaps)
      let span = (times.last ?? 0) - (times.first ?? 0)
      let fps = span > 0 ? Double(times.count - 1) / span : 0
      // Gaps by the refresh they are: 8.3 ms is 120 Hz, 16.7 is 60 Hz, more is a frame nobody saw.
      let at120 = gaps.filter { $0 < 12.5 }.count, at60 = gaps.filter { $0 >= 12.5 && $0 < 25 }.count
      let at30 = gaps.filter { $0 >= 25 && $0 < 50 }.count, slow = gaps.filter { $0 >= 50 }.count
      let worst = gaps.sorted().suffix(5).reversed().map { String(r($0)) }.joined(separator: ",")
      return "{\"frames\":\(times.count),\"fps\":\(r(fps)),\"gapMs\":{\"p50\":\(r(s.p50)),\"p90\":\(r(s.p90)),\"p99\":\(r(s.p99)),\"max\":\(r(s.max))},\"gaps\":{\"under12.5\":\(at120),\"12.5to25\":\(at60),\"25to50\":\(at30),\"over50\":\(slow)},\"worstMs\":[\(worst)]}"
    }
    var out = "{\"what\":\"\(label)\",\"seconds\":\(r(to - from)),\"shown\":\(describe(all.map { $0.0 }))"
    if stripSpec != nil {
      // The frames in which the list was somewhere else than in the frame before.
      let moving = all.filter { ($0.1 ?? 0) != 0 }
      let steps = moving.map { abs($0.1!) }
      let s = Recorder.stats(steps)
      let total = moving.reduce(0.0) { $0 + $1.1! }
      // A step is uneven when it is under half or over one and a half times the median step.
      let uneven = steps.filter { $0 < s.p50 * 0.5 || $0 > s.p50 * 1.5 }.count
      let changedOnly = all.filter { $0.1 != nil && $0.1! == 0 }.count
      out += ",\"moved\":\(describe(moving.map { $0.0 })),\"stepPx\":{\"p50\":\(r(s.p50)),\"p90\":\(r(s.p90)),\"max\":\(r(s.max))},\"unevenSteps\":\(uneven),\"travelledPx\":\(r(total)),\"changedInPlace\":\(changedOnly)"
    }
    return out + "}"
  }
}

func scrollEvent(_ dy: Double, phase: Int64, momentum: Int64, at p: CGPoint, source: CGEventSource?) -> CGEvent? {
  // Pixel units; negative is down in CGEvent's sense.
  guard let e = CGEvent(scrollWheelEvent2Source: source, units: .pixel, wheelCount: 1, wheel1: Int32((-dy).rounded()), wheel2: 0, wheel3: 0) else { return nil }
  e.setDoubleValueField(.scrollWheelEventPointDeltaAxis1, value: -dy)
  e.setDoubleValueField(.scrollWheelEventFixedPtDeltaAxis1, value: -dy / 10)
  e.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
  e.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase)
  e.setIntegerValueField(.scrollWheelEventMomentumPhase, value: momentum)
  e.location = p
  return e
}

/** Sleeps until `t` on the mach clock, to a fraction of a millisecond. */
func sleepUntil(_ t: Double) { let d = t - nowS(); if d > 0 { usleep(useconds_t(d * 1_000_000)) } }

if let i = args.firstIndex(of: "--strip"), i + 3 < args.count, let x = Double(args[i + 1]), let y0 = Double(args[i + 2]), let y1 = Double(args[i + 3]) { stripSpec = Strip(x: x, y0: y0, y1: y1); args.removeSubrange(i...i + 3) }
guard let cmd = args.first else { fail("usage: kn scroll|capture|front|idle ...") }
args.removeFirst()

switch cmd {
case "idle":
  print(CGEventSource.secondsSinceLastEventType(.hidSystemState, eventType: CGEventType(rawValue: ~0)!))
case "windowid":
  // The window server's id of the process's own largest on-screen window, or exit 1 when it has none. What a
  // screenshot must be taken by (`screencapture -l <id>`): a screen rectangle shows whatever is there, and when the
  // window I meant had not opened yet, that was the owner's browser.
  guard let pid = args.first.flatMap({ pid_t($0) }) else { fail("usage: kn windowid <pid>") }
  let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
  let mine = list.filter { ($0[kCGWindowOwnerPID as String] as? pid_t) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
  let best = mine.max { a, b in
    let ra = CGRect(dictionaryRepresentation: a[kCGWindowBounds as String] as! CFDictionary) ?? .zero
    let rb = CGRect(dictionaryRepresentation: b[kCGWindowBounds as String] as! CFDictionary) ?? .zero
    return ra.width * ra.height < rb.width * rb.height
  }
  guard let w = best, let id = w[kCGWindowNumber as String] as? Int, let r = CGRect(dictionaryRepresentation: w[kCGWindowBounds as String] as! CFDictionary) else {
    FileHandle.standardError.write("kn: pid \(pid) has no window on screen\n".data(using: .utf8)!)
    exit(1)
  }
  print("\(id) \(Int(r.minX)) \(Int(r.minY)) \(Int(r.width)) \(Int(r.height))")
case "front":
  guard let pid = args.first.flatMap({ pid_t($0) }) else { fail("usage: kn front <pid>") }
  NSRunningApplication(processIdentifier: pid)?.activate(options: [])
  usleep(400_000)
case "capture":
  guard args.count >= 2, let pid = pid_t(args[0]), let secs = Double(args[1]) else { fail("usage: kn capture <pid> SECONDS") }
  let rec = Recorder()
  Task {
    do { try await rec.start(pid: pid) } catch { fail("kn: recording failed: \(error)") }
    let from = nowS()
    try? await Task.sleep(nanoseconds: UInt64(secs * 1e9))
    let to = nowS()
    try? await Task.sleep(nanoseconds: 150_000_000)
    await rec.stop()
    print(rec.report(from: from, to: to, label: "capture"))
    exit(0)
  }
  dispatchMain()
case "scroll":
  let hid = flag("--hid"), capture = flag("--capture"), momentum = flag("--momentum"), notches = flag("--notches")
  let timer = args.contains("--hz")
  let hz = Double(option("--hz") ?? "120") ?? 120
  let raw = flag("--raw")
  let lines = option("--lines").flatMap { Int($0) }
  var refresh = 0.0
  guard args.count >= 5, let pid = pid_t(args[0]), let x = Double(args[1]), let y = Double(args[2]), let speed = Double(args[3]), let secs = Double(args[4]) else {
    fail("usage: kn scroll <pid> X Y PX_PER_SEC SECONDS [--hz N] [--hid] [--capture] [--momentum] [--notches]")
  }
  let origin = contentOrigin(pid)
  let p = CGPoint(x: origin.x + x, y: origin.y + y)
  let source = CGEventSource(stateID: hid ? .hidSystemState : .combinedSessionState)
  func post(_ e: CGEvent?) { if hid { e?.post(tap: .cghidEventTap) } else { e?.postToPid(pid) } }
  let rec = Recorder()
  Task {
    if capture { do { try await rec.start(pid: pid) } catch { fail("kn: recording failed: \(error)") } }
    if hid {
      let m = CGEvent(mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)
      m?.post(tap: .cghidEventTap)
    }
    try? await Task.sleep(nanoseconds: 500_000_000)
    let from = nowS()
    var sent = 0
    if let lines {
      // A wheel mouse: line-unit events with no phase, `lines` lines a notch, PX_PER_SEC read as notches a second.
      // The recording then says in how many frames the list moved for each notch: one (a jump) or many (animated).
      let n = max(1, Int(secs * speed))
      for i in 0..<n {
        sleepUntil(from + Double(i) / speed)
        let e = CGEvent(scrollWheelEvent2Source: source, units: .line, wheelCount: 1, wheel1: Int32(-lines), wheel2: 0, wheel3: 0)
        e?.location = p
        post(e); sent += 1
      }
      sleepUntil(from + Double(n) / speed + 0.5)
    } else if notches {
      let n = Int(secs * 30), step = speed / 30
      for i in 0..<n {
        sleepUntil(from + Double(i) / 30)
        let e = CGEvent(scrollWheelEvent2Source: source, units: .pixel, wheelCount: 1, wheel1: Int32((-step).rounded()), wheel2: 0, wheel3: 0)
        e?.location = p
        post(e); sent += 1
      }
    } else if timer {
      let n = Int(secs * hz), step = speed / hz
      for i in 0..<n {
        sleepUntil(from + Double(i) / hz)
        post(scrollEvent(step, phase: i == 0 ? 1 : 2, momentum: 0, at: p, source: source)); sent += 1
      }
      sleepUntil(from + Double(n) / hz)
      post(scrollEvent(0, phase: 4, momentum: 0, at: p, source: source))
    } else {
      // One event a display refresh, from the display link, as the trackpad's arrive: a free-running timer beats
      // against the refresh and puts two events in one frame and none in the next.
      var link: CVDisplayLink?
      CVDisplayLinkCreateWithActiveCGDisplays(&link)
      guard let link else { fail("kn: no display link") }
      let period = CVDisplayLinkGetNominalOutputVideoRefreshPeriod(link)
      let dt = period.timeScale > 0 ? Double(period.timeValue) / Double(period.timeScale) : 1.0 / 120
      let step = speed * dt
      let glide = momentum ? 1.2 : 0.0
      let finished = DispatchSemaphore(value: 0)
      var state = 0  // 0 fingers down, 1 lifted, 2 gliding, 3 over
      CVDisplayLinkSetOutputHandler(link) { _, _, _, _, _ in
        let t = nowS() - from
        switch state {
        case 0:
          if t < secs { post(scrollEvent(step, phase: sent == 0 ? 1 : 2, momentum: 0, at: p, source: source)); sent += 1 }
          else { post(scrollEvent(0, phase: 4, momentum: 0, at: p, source: source)); state = glide > 0 ? 1 : 3; if state == 3 { finished.signal() } }
        case 1, 2:
          let g = (t - secs) / glide
          if g < 1 {
            // Decays to a third of the speed, then stops: a glide that crawls its last pixels would read as stalls.
            post(scrollEvent(step * (0.33 + 0.67 * pow(1 - g, 2)), phase: 0, momentum: state == 1 ? 1 : 2, at: p, source: source)); sent += 1; state = 2
          } else { post(scrollEvent(0, phase: 0, momentum: 3, at: p, source: source)); state = 3; finished.signal() }
        default: break
        }
        return kCVReturnSuccess
      }
      CVDisplayLinkStart(link)
      finished.wait()
      CVDisplayLinkStop(link)
      refresh = 1 / dt
    }
    let to = nowS()
    try? await Task.sleep(nanoseconds: 400_000_000)
    if capture {
      await rec.stop()
      print(rec.report(from: from, to: to, label: lines != nil ? "wheel mouse, \(lines!) lines a notch, \(Int(speed)) notches a second, \(sent) notches" : "scroll \(Int(speed)) px/s \(notches ? "notches" : timer ? "phased, timer \(Int(hz)) Hz" : "phased, one event a refresh (\(Int(refresh.rounded())) Hz)")\(hid ? " hid" : " pid"), \(sent) events"))
      if raw { print(rec.steps(from: from, to: to)) }
    }
    else { print("{\"sent\":\(sent),\"seconds\":\((to - from))}") }
    exit(0)
  }
  dispatchMain()
default:
  fail("kn: unknown command \(cmd)")
}
