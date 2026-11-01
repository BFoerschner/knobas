# Keep a WSL distribution running 24/7 and reachable from the LAN, as the host
# of knobas' live products. Run in an ELEVATED PowerShell on the Windows box:
#
#   powershell -ExecutionPolicy Bypass -File setup-windows.ps1 -Distro Ubuntu [-Port 2222] [-MemoryGB 10]
#
# 1. %UserProfile%\.wslconfig: memory, swap, mirrored networking (WSL's sshd
#    answers on the Windows box's own address) and no idle shutdown of the
#    VM. MEMORY IS THE POINT on a 16 GB box: WSL gets physical memory less
#    6 GB (10 of 16) unless -MemoryGB says otherwise, so Windows keeps enough
#    to stay usable whatever runs inside -- the VM cannot take more than its
#    cap. 4 GB of swap catches a spike, and autoMemoryReclaim hands freed
#    cache back to Windows. live-lib.sh's memory budget plans against
#    whatever WSL gets, and docker-compose.remote.yml caps each container.
#    An existing file is backed up and only these keys are set in it.
# 2. A Hyper-V firewall rule letting -Port in to WSL.
# 3. A scheduled task that starts the distribution at boot and keeps it open,
#    so systemd (dockerd, sshd) runs with nobody logged in.
# Then: wsl --shutdown, and testenv/wsl/setup-wsl.sh inside the distribution.
# Needs Windows 11 22H2+ with WSL 2.0+ for mirrored networking.
param(
  [Parameter(Mandatory = $true)][string]$Distro,
  [int]$Port = 2222,
  [int]$MemoryGB = 0
)
$ErrorActionPreference = 'Stop'

$physGB = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
if ($MemoryGB -le 0) { $MemoryGB = [math]::Max(6, $physGB - 6) }
if ($MemoryGB -gt $physGB - 4) {
  throw "setup-windows: -MemoryGB $MemoryGB leaves Windows under 4 GB of $physGB GB; the box would freeze"
}
Write-Host "setup-windows: $physGB GB physical; WSL gets $MemoryGB GB"

# key=value per section, set in place or added; everything else kept.
function Set-WslKey([System.Collections.Generic.List[string]]$lines, [string]$section, [string]$key, [string]$value) {
  $start = -1
  for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i].Trim() -eq "[$section]") { $start = $i; break } }
  if ($start -lt 0) { $lines.Add(''); $lines.Add("[$section]"); $lines.Add("$key=$value"); return }
  for ($i = $start + 1; $i -lt $lines.Count -and -not $lines[$i].Trim().StartsWith('['); $i++) {
    if ($lines[$i] -match "^\s*$key\s*=") { $lines[$i] = "$key=$value"; return }
  }
  $lines.Insert($start + 1, "$key=$value")
}
$wslconfig = Join-Path $env:USERPROFILE '.wslconfig'
$lines = New-Object 'System.Collections.Generic.List[string]'
if (Test-Path $wslconfig) {
  Copy-Item $wslconfig "$wslconfig.bak-$(Get-Date -Format yyyyMMdd-HHmmss)"
  foreach ($l in Get-Content $wslconfig) { $lines.Add($l) }
}
Set-WslKey $lines 'wsl2' 'memory' "${MemoryGB}GB"
Set-WslKey $lines 'wsl2' 'swap' '4GB'
Set-WslKey $lines 'wsl2' 'networkingMode' 'mirrored'
Set-WslKey $lines 'wsl2' 'vmIdleTimeout' '-1'
Set-WslKey $lines 'experimental' 'autoMemoryReclaim' 'gradual'
Set-Content -Path $wslconfig -Value $lines -Encoding ASCII
Write-Host "setup-windows: $wslconfig set (memory=${MemoryGB}GB swap=4GB mirrored, no idle shutdown)"

# The WSL VM's creator id; Hyper-V firewall rules are keyed by it.
$wslVm = '{40E0AC32-46A5-438A-A0B2-2B479E8F2E90}'
if (-not (Get-NetFirewallHyperVRule -Name 'knobas-wsl-ssh' -ErrorAction SilentlyContinue)) {
  New-NetFirewallHyperVRule -Name 'knobas-wsl-ssh' -DisplayName 'knobas: ssh into WSL' `
    -Direction Inbound -VMCreatorId $wslVm -Protocol TCP -LocalPorts $Port | Out-Null
  Write-Host "setup-windows: firewall rule knobas-wsl-ssh (TCP $Port in)"
}
if (-not (Get-NetFirewallRule -Name 'knobas-wsl-ssh-host' -ErrorAction SilentlyContinue)) {
  New-NetFirewallRule -Name 'knobas-wsl-ssh-host' -DisplayName 'knobas: ssh into WSL (host)' `
    -Direction Inbound -Protocol TCP -LocalPort $Port -Action Allow | Out-Null
}

$task = 'knobas-wsl-keepalive'
$action = New-ScheduledTaskAction -Execute 'wsl.exe' -Argument "-d $Distro --exec /bin/sleep infinity"
$trigger = New-ScheduledTaskTrigger -AtStartup
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 `
  -RestartInterval (New-TimeSpan -Minutes 1) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType S4U -RunLevel Highest
Register-ScheduledTask -TaskName $task -Action $action -Trigger $trigger -Settings $settings `
  -Principal $principal -Force | Out-Null
Write-Host "setup-windows: scheduled task $task keeps '$Distro' running from boot"

Write-Host "setup-windows: now 'wsl --shutdown', then 'Start-ScheduledTask $task', then inside WSL: bash setup-wsl.sh --key '<the Mac key>' --port $Port"
Write-Host "setup-windows: also: Settings > System > Power, sleep 'Never' -- a sleeping box answers nothing"
