# May the screen be driven? Prints one line and exits 0 for yes, 1 for no.
# Yes when the last input in the session is one of mine (its tick is the one wake.ps1 recorded, or within 100 ms of
# it) or is at least five minutes old. Anything else is Björn's input: stop.
$root = Split-Path -Parent $PSScriptRoot  # the scratch folder these scripts sit one below
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Gd {
  [StructLayout(LayoutKind.Sequential)] struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LASTINPUTINFO p);
  [DllImport("kernel32.dll")] public static extern uint GetTickCount();
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static uint LastInput() { var l = new LASTINPUTINFO(); l.cbSize = (uint)Marshal.SizeOf(l); GetLastInputInfo(ref l); return l.dwTime; }
  public static uint ForegroundPid() { uint pid; GetWindowThreadProcessId(GetForegroundWindow(), out pid); return pid; }
}
"@
$last = [Gd]::LastInput(); $now = [Gd]::GetTickCount()
$mine = 0; if (Test-Path "$root\mine.tick") { $mine = [uint32](Get-Content "$root\mine.tick") }
$idle = ($now - $last) / 1000
$isMine = ($mine -ne 0) -and ([Math]::Abs([int64]$last - [int64]$mine) -le 100)
$fg = [Gd]::ForegroundPid(); $fgName = if ($fg) { (Get-Process -Id $fg -ErrorAction SilentlyContinue).ProcessName } else { "" }
$locked = [bool](Get-Process -Name LogonUI -ErrorAction SilentlyContinue)
$ok = ($isMine -or $idle -ge 300) -and -not $locked
"ok={0} idle_s={1:F0} last_input_is_mine={2} locked={3} foreground={4}" -f $ok, $idle, $isMine, $locked, $fgName
if ($ok) { exit 0 } else { exit 1 }
