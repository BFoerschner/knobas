# Keeps the display on for as long as this process lives: SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED |
# ES_DISPLAY_REQUIRED) on its one thread. Ends when hold.stop appears or after three hours. Changes no setting.
$root = Split-Path -Parent $PSScriptRoot  # the scratch folder these scripts sit one below
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Hold { [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint flags); }
"@
$prev = [Hold]::SetThreadExecutionState([uint32]2147483651)
Set-Content "$root\hold.state" ("set at {0}, previous state {1}" -f (Get-Date -Format HH:mm:ss), $prev)
$end = (Get-Date).AddHours(3)
while ((Get-Date) -lt $end -and -not (Test-Path "$root\hold.stop")) { Start-Sleep -Seconds 2; [Hold]::SetThreadExecutionState([uint32]2147483651) | Out-Null }
[Hold]::SetThreadExecutionState([uint32]2147483648) | Out-Null
Add-Content "$root\hold.state" ("released at {0}" -f (Get-Date -Format HH:mm:ss))
