<#
.SYNOPSIS
  Optimization-level sweep on the TM4C123: rebuild the constant-time timing
  firmware at -O0/-O1/-O2/-O3, flash each, capture a dudect timing set, and save
  results/timing/safe_O{n}.npz. Then `python analysis/make_figures.py` builds
  results/figures/opt_sweep.png (peak |t| vs optimization level).

  Needs the board plugged in (COM20 by default) and LM Flash Programmer.
  Reproducibility helper for the paper; safe to re-run.
#>
[CmdletBinding()]
param(
  [string]$LmFlash = "C:\Program Files (x86)\Texas Instruments\Stellaris\LM Flash Programmer\LMFlash.exe",
  [int[]]$Levels = @(0, 1, 2, 3),
  [int]$N = 3000,
  [string]$Port = "COM20"
)
$ErrorActionPreference = 'Continue'
$repo = Split-Path $PSScriptRoot -Parent
$fw = Join-Path $repo 'firmware-tm4c'
$bin = Join-Path $fw 'fw.bin'

foreach ($lvl in $Levels) {
  Write-Host "===== optimization level -O$lvl ====="
  $env:CARGO_PROFILE_RELEASE_OPT_LEVEL = "$lvl"
  Push-Location $fw
  cargo objcopy --release --features timing -- -O binary $bin 2>&1 | Out-Null
  Pop-Location

  $flashed = $false
  for ($a = 1; $a -le 3; $a++) {
    $out = & $LmFlash -q manual -i ICDI -e all -v -r $bin 2>&1 | Out-String
    if ($out -match 'Verify Complete - Passed') { $flashed = $true; break }
    Start-Sleep -Milliseconds 500
  }
  if (-not $flashed) { Write-Host "  -O$lvl : FLASH FAILED, skipping"; continue }
  Start-Sleep -Milliseconds 1000

  $npz = Join-Path $repo "results\timing\safe_O$lvl.npz"
  python (Join-Path $repo 'capture\collect_timing.py') --port $Port --experiment tagcompare `
    --variant safe --n $N --out $npz 2>&1 | Select-String 'saved'
  $t = python -c "import sys; sys.path.insert(0, r'$repo\analysis'); from dudect import load, welch_scalar; c,l,v,e=load(r'$npz'); print(f'{abs(welch_scalar(c[l==0],c[l==1])):.2f}')"
  Write-Host "  -O$lvl : |t| = $t"
}
Remove-Item Env:\CARGO_PROFILE_RELEASE_OPT_LEVEL -ErrorAction SilentlyContinue
Write-Host 'SWEEP DONE'
