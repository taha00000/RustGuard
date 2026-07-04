<#
.SYNOPSIS
  Build firmware-stm32-timing and flash it to the STM32F303 (STM32F3 Discovery)
  over ST-LINK with STM32CubeProgrammer.

.DESCRIPTION
  Cross-silicon counterpart to scripts/flash.ps1. Measurement data is read over a
  USB-UART dongle (RX<-PA2, TX->PA3, GND<-GND @115200), NOT over ST-LINK. See
  docs/cross_silicon.md.

.PARAMETER Mode  safe (constant-time DUT, default) | leaky (variable-time control)
.PARAMETER BuildOnly  build + objcopy only; skip flashing (no board needed)
#>
[CmdletBinding()]
param(
  [ValidateSet('safe', 'leaky')] [string]$Mode = 'safe',
  [string]$Cli = "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe",
  [switch]$BuildOnly
)
$ErrorActionPreference = 'Stop'
$fwDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'firmware-stm32-timing'
$bin = Join-Path $fwDir 'fw.bin'
$feat = if ($Mode -eq 'leaky') { @('--features', 'leaky') } else { @() }

Push-Location $fwDir
try {
  Write-Host "[1/2] Building + objcopy firmware-stm32-timing ($Mode) -> fw.bin" -ForegroundColor Cyan
  cargo objcopy --release @feat -- -O binary $bin
  Write-Host ("      fw.bin = {0:N0} bytes" -f (Get-Item $bin).Length)
  if ($BuildOnly) { Write-Host "[2/2] -BuildOnly; skipping flash." -ForegroundColor Yellow; return }
  if (-not (Test-Path $Cli)) {
    throw "STM32CubeProgrammer CLI not found at '$Cli'. Install STM32CubeProgrammer (free from ST), or flash fw.bin at 0x08000000 with its GUI."
  }
  Write-Host "[2/2] Flashing over ST-LINK (SWD, write+verify+reset)" -ForegroundColor Cyan
  & $Cli -c port=SWD -w $bin 0x08000000 -v -rst
}
finally { Pop-Location }

Write-Host ""
Write-Host "Next: read the USB-UART dongle's COM port @115200 8N1 (RX<-PA2, TX->PA3, GND<-GND)." -ForegroundColor Green
$v = if ($Mode -eq 'leaky') { 'leaky' } else { 'safe' }
Write-Host "  python capture\collect_timing.py --port COM<N> --variant $v --out results\timing\stm32_$v.npz"
