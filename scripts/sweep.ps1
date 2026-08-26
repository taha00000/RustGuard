<#
.SYNOPSIS
  Full ecosystem sweep: for each optimization level, rebuild the multi-primitive
  timing firmware, flash it, and capture verification timing for EVERY primitive
  in the registry.

.DESCRIPTION
  One firmware image carries all probes, so a complete sweep is one flash per
  optimization level (not one per crate). Results land in results/timing as
  `<board>_<opt>_<primitive>.npz`, which analysis/matrix.py turns into the
  leakage matrix figure and table.

  Requires the board plugged in and its flashing tool installed:
    tm4c  -> LM Flash Programmer (default)
    stm32 -> STM32CubeProgrammer (pass -Board stm32)

.EXAMPLE
  scripts\sweep.ps1 -Port COM20 -Board tm4c
  scripts\sweep.ps1 -Port COM22 -Board stm32 -Levels 0,3 -N 2000
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Port,
  [ValidateSet('tm4c', 'stm32')][string]$Board = 'tm4c',
  [int[]]$Levels = @(0, 1, 2, 3),
  [int]$N = 3000,
  [string]$LmFlash = "C:\Program Files (x86)\Texas Instruments\Stellaris\LM Flash Programmer\LMFlash.exe",
  [string]$CubeCli = "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe"
)
$ErrorActionPreference = 'Continue'
$repo = Split-Path $PSScriptRoot -Parent
$fw = if ($Board -eq 'tm4c') { Join-Path $repo 'firmware-tm4c' } else { Join-Path $repo 'firmware-stm32-timing' }
$feat = if ($Board -eq 'tm4c') { @('--features', 'timing leaky') } else { @('--features', 'leaky') }
$bin = Join-Path $fw 'fw.bin'

foreach ($lvl in $Levels) {
  Write-Host "===== $Board  optimization -O$lvl =====" -ForegroundColor Cyan
  $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = "$lvl"
  Push-Location $fw
  cargo objcopy --release @feat -- -O binary $bin 2>&1 | Out-Null
  Pop-Location
  if (-not (Test-Path $bin)) { Write-Host "  build failed, skipping"; continue }

  $flashed = $false
  for ($a = 1; $a -le 3; $a++) {
    if ($Board -eq 'tm4c') {
      $out = & $LmFlash -q manual -i ICDI -e all -v -r $bin 2>&1 | Out-String
      if ($out -match 'Verify Complete - Passed') { $flashed = $true; break }
    } else {
      $out = & $CubeCli -c port=SWD -w $bin 0x08000000 -v -rst 2>&1 | Out-String
      if ($out -match 'Download verified successfully|File download complete') { $flashed = $true; break }
    }
    Start-Sleep -Milliseconds 600
  }
  if (-not $flashed) { Write-Host "  FLASH FAILED at -O$lvl, skipping" -ForegroundColor Red; continue }
  Start-Sleep -Milliseconds 1200

  python (Join-Path $repo 'capture\collect_timing.py') --port $Port --board $Board `
    --opt "O$lvl" --n $N --outdir (Join-Path $repo 'results\timing')
}
Remove-Item Env:\CARGO_PROFILE_RELEASE_OPT_LEVEL -ErrorAction SilentlyContinue

Write-Host "`nBuilding leakage matrix..." -ForegroundColor Cyan
python (Join-Path $repo 'analysis\matrix.py')
Write-Host "SWEEP DONE" -ForegroundColor Green
