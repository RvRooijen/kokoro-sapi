# Downloads everything the engine needs at runtime into %LOCALAPPDATA%\KokoroSapi:
#   model.onnx        Kokoro 82M (onnx-community export)
#   tokenizer.json    phoneme -> token id vocab
#   voices\*.bin      style vectors per voice
#   onnxruntime.dll   inference runtime (loaded dynamically by the engine)
#   espeak-ng.dll + espeak-ng-data\   phonemizer (taken from the piper release zip)
#
# Usage:  .\download-assets.ps1            (fp32 model, ~326 MB)
#         .\download-assets.ps1 -Quantized (int8 model, ~92 MB, slightly lower quality)

param(
    [switch]$Quantized,
    [string]$Dir = "$env:LOCALAPPDATA\KokoroSapi"
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # massively speeds up Invoke-WebRequest

New-Item -ItemType Directory -Force -Path $Dir, "$Dir\voices" | Out-Null
$hf = 'https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main'

function Get-IfMissing($url, $dest) {
    if (Test-Path $dest) { Write-Host "skip (exists): $dest"; return }
    Write-Host "downloading: $url"
    Invoke-WebRequest -Uri $url -OutFile $dest
}

# --- Kokoro model + tokenizer + voices ---
$modelFile = if ($Quantized) { 'model_quantized.onnx' } else { 'model.onnx' }
Get-IfMissing "$hf/onnx/$modelFile" "$Dir\model.onnx"
Get-IfMissing "$hf/tokenizer.json" "$Dir\tokenizer.json"

# Must match the voices registered in register.ps1 (af=US female, am=US male, bf=GB female)
foreach ($v in 'af_heart', 'af_bella', 'am_michael', 'bf_emma') {
    Get-IfMissing "$hf/voices/$v.bin" "$Dir\voices\$v.bin"
}

# --- ONNX Runtime (engine needs API version >= 17, i.e. ORT 1.17+) ---
# NOTE: pin to 1.26.0+. Older builds (incl. 1.22.0) deadlock during CreateEnv:
# ORT's WindowsTelemetry registers an ETW provider, and when the DiagTrack
# telemetry session is subscribed (the default on Windows) the enable callback
# fires synchronously and self-deadlocks. There is no runtime opt-out; the ETW
# handling was fixed in later ORT releases. Do not downgrade.
if (-not (Test-Path "$Dir\onnxruntime.dll")) {
    $ortVer = '1.26.0'
    $zip = "$env:TEMP\onnxruntime-win-x64-$ortVer.zip"
    Get-IfMissing "https://github.com/microsoft/onnxruntime/releases/download/v$ortVer/onnxruntime-win-x64-$ortVer.zip" $zip
    $tmp = "$env:TEMP\ort-extract"
    Expand-Archive -Force $zip $tmp
    Copy-Item "$tmp\onnxruntime-win-x64-$ortVer\lib\onnxruntime.dll" $Dir
    Copy-Item "$tmp\onnxruntime-win-x64-$ortVer\lib\onnxruntime_providers_shared.dll" $Dir -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $tmp, $zip
}

# --- espeak-ng (dll + phoneme data), conveniently bundled in the piper release ---
if (-not (Test-Path "$Dir\espeak-ng.dll")) {
    $zip = "$env:TEMP\piper_windows_amd64.zip"
    Get-IfMissing 'https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip' $zip
    $tmp = "$env:TEMP\piper-extract"
    Expand-Archive -Force $zip $tmp
    Copy-Item "$tmp\piper\espeak-ng.dll" $Dir
    Copy-Item -Recurse -Force "$tmp\piper\espeak-ng-data" "$Dir\espeak-ng-data"
    Remove-Item -Recurse -Force $tmp, $zip
}

Write-Host "`nDone. Assets in $Dir"
Get-ChildItem $Dir | Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table
