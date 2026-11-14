# Turns the display on and keeps it on while the measurements run, without changing a setting.
#   -Do wake     one mouse move of zero pixels through SendInput (the pointer stays where it is), which is what
#                wakes a display Windows has turned off; the tick of that input is written to mine.tick so
#                guard.ps1 can tell it from Björn's. Refuses unless he has been idle five minutes.
#   -Do hold     starts a hidden PowerShell that asks Windows to keep the display on (SetThreadExecutionState,
#                which lasts only as long as that process) until hold.stop appears or three hours pass.
#   -Do release  ends the holder; the display then turns off again on its own timer.
param([string]$Do = "wake")
$root = Split-Path -Parent $PSScriptRoot  # the scratch folder these scripts sit one below
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Wk {
  [StructLayout(LayoutKind.Sequential)] struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LASTINPUTINFO p);
  [DllImport("kernel32.dll")] public static extern uint GetTickCount();
  [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public MOUSEINPUT mi; }
  [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
  [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint flags);
  public static uint LastInput() { var l = new LASTINPUTINFO(); l.cbSize = (uint)Marshal.SizeOf(l); GetLastInputInfo(ref l); return l.dwTime; }
  public static uint Nudge() { var i = new INPUT[1]; i[0].type = 0; i[0].mi.dwFlags = 0x0001; SendInput(1, i, Marshal.SizeOf(typeof(INPUT))); return GetTickCount(); }
}
"@
if ($Do -eq "wake") {
  $idle = ([Wk]::GetTickCount() - [Wk]::LastInput()) / 1000
  if ($idle -lt 300) { "refused: last input {0:F0} s ago" -f $idle; exit 1 }
  $t = [Wk]::Nudge()
  Start-Sleep -Milliseconds 200
  Set-Content "$root\mine.tick" ([Wk]::LastInput())
  "nudged at tick {0}; last input now at tick {1}" -f $t, [Wk]::LastInput()
} elseif ($Do -eq "hold") {
  Remove-Item "$root\hold.stop" -ErrorAction SilentlyContinue
  # The first holder was given its code on the command line; the quotes around the DLL name did not survive, the
  # call was never made, and the display went off again after its 15 minutes, in the middle of a run. The code is
  # a file now (hold.ps1), which also writes hold.state when the call has been made.
  $p = Start-Process powershell.exe -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File", "$PSScriptRoot\hold.ps1" -WindowStyle Hidden -PassThru
  Set-Content "$root\hold.pid" $p.Id
  "holder pid={0}" -f $p.Id
} elseif ($Do -eq "release") {
  Set-Content "$root\hold.stop" "stop"
  Start-Sleep -Seconds 4
  $id = Get-Content "$root\hold.pid" -ErrorAction SilentlyContinue
  $alive = if ($id) { [bool](Get-Process -Id $id -ErrorAction SilentlyContinue) } else { $false }
  if ($alive) { Stop-Process -Id $id -Force; "holder $id stopped by force" } else { "holder gone" }
}
