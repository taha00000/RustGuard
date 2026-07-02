<#
.SYNOPSIS
  Build a firmware-tm4c image and flash it to the EK-TM4C123GXL LaunchPad with
  TI LM Flash Programmer (over the on-board ICDI debug port).

.DESCRIPTION
  One command for hardware day. Builds the chosen firmware mode, converts it to a
  .bin with `cargo objcopy`, and programs it via LMFlash.exe. Measurement data
  itself is read separately over a USB-UART dongle (RX<-PA1, TX->PA0, GND<-GND) at
  115200 8N1 -- NOT over the board's USB. See docs/flashing.md and docs/hardware_setup.md.

.PARAMETER Mode
  perf         : performance benchmark (default build)              [default]
  timing       : dudect timing-leakage harness (constant-time DUT)
  timing-leaky : timing harness with the variable-time control

.PARAMETER BuildOnly
  Build + objcopy only; skip flashing (useful with no board attached).

.EXAMPLE
  scripts\flash.ps1 -Mode timing-leaky
  scripts\flash.ps1 -Mode timing
#>
[CmdletBinding()]
param(
  [ValidateSet('perf', 'timing', 'timing-leaky')]
  [string]$Mode = 'perf',
  [string]$LmFlash = "C:\Program Files (x86)\Texas Instruments\Stellaris\LM Flash Programmer\LMFlash.exe",
  [switch]$BuildOnly
)
$ErrorActionPreference = 'Stop'
$fwDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'firmware-tm4c'
$bin = Join-Path $fwDir 'fw.bin'

$featArgs = switch ($Mode) {
  'perf' { @() }
  'timing' { @('--features', 'timing') }
  'timing-leaky' { @('--features', 'timing leaky') }
}

Write-Host "[1/2] Building + objcopy firmware-tm4c ($Mode) -> fw.bin" -ForegroundColor Cyan
Push-Location $fwDir
try {
  cargo objcopy --release @featArgs -- -O binary $bin
  if ($LASTEXITCODE -ne 0) { throw "cargo objcopy failed ($LASTEXITCODE)" }
  Write-Host ("      fw.bin = {0:N0} bytes" -f (Get-Item $bin).Length)

  if ($BuildOnly) {
    Write-Host "[2/2] -BuildOnly set; skipping flash." -ForegroundColor Yellow
    return
  }
  if (-not (Test-Path $LmFlash)) {
    throw "LM Flash Programmer not found at '$LmFlash'. Pass -LmFlash <path> or flash fw.bin via the GUI."
  }

  Write-Host "[2/2] Flashing via LM Flash Programmer (ICDI, erase+verify+reset)" -ForegroundColor Cyan
  & $LmFlash -q manual -i ICDI -e all -v -r $bin
  if ($LASTEXITCODE -ne 0) {
    Write-Warning "LMFlash returned $LASTEXITCODE. If CLI flashing is unreliable, open the GUI, pick board EK-TM4C123GXL, and program $bin."
  } else {
    Write-Host "Flashed OK." -ForegroundColor Green
  }
}
finally { Pop-Location }

Write-Host ""
Write-Host "Next: read the USB-UART dongle's COM port at 115200 8N1 (RX<-PA1, TX->PA0, GND<-GND)." -ForegroundColor Green
if ($Mode -eq 'perf') {
  Write-Host "  capture output to dump.txt, then: python analysis\parse_perf.py dump.txt --out results\perf_rust.csv"
} else {
  $variant = if ($Mode -eq 'timing-leaky') { 'leaky' } else { 'safe' }
  Write-Host "  python capture\collect_timing.py --port COM<N> --variant $variant --out results\timing\$variant.npz"
  Write-Host "  (the firmware prints a READY banner; a 'k'<32hex> command should reply 'z' -- that confirms the UART wiring)."
}
