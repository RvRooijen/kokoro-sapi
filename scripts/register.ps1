# Registers the engine DLL and the Kokoro voices per-user (HKCU) — no admin needed.
# 64-bit apps only (Chrome, Edge, 64-bit PowerShell), which is what we care about.
#
# SAPI never enumerates HKCU voice tokens, so per-user voices do not show up in
# voice lists; they work via the default-voice mechanism (set-default-voice.ps1,
# which is how Chrome reading mode uses them) or when opened by token id. To
# also make them appear in voice lists (Edge read aloud, reader extensions),
# add -Machine from an elevated prompt: that writes the tokens to HKLM as well.
# The COM class itself stays per-user, so this still only works for this
# Windows account.
#
# Usage:  .\register.ps1                       (finds the release DLL automatically)
#         .\register.ps1 -DllPath C:\path\kokoro_sapi.dll
#         .\register.ps1 -Machine               (elevated: voices in HKLM too)

param(
    [string]$DllPath,
    [string]$AssetsDir = "$env:LOCALAPPDATA\KokoroSapi",
    [switch]$Machine
)

$ErrorActionPreference = 'Stop'

# Must match CLSID_KOKORO_ENGINE in src/lib.rs.
$clsid = '{6A2C7F52-3B19-4E5D-9C01-8F4A2D7B61E3}'

if (-not $DllPath) {
    $root = Split-Path $PSScriptRoot -Parent
    $candidates = @(
        "$root\target\release\kokoro_sapi.dll",
        "$root\target\x86_64-pc-windows-msvc\release\kokoro_sapi.dll"
    )
    $DllPath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $DllPath) { throw "kokoro_sapi.dll not found — build first (cargo build --release) or pass -DllPath" }
}
$DllPath = (Resolve-Path $DllPath).Path
Write-Host "registering $DllPath"

# COM class (per-user)
$comKey = "HKCU:\Software\Classes\CLSID\$clsid\InprocServer32"
New-Item -Force -Path $comKey | Out-Null
Set-ItemProperty -Path $comKey -Name '(default)' -Value $DllPath
Set-ItemProperty -Path $comKey -Name 'ThreadingModel' -Value 'Both'

# Voice tokens. Language: 409 = en-US, 809 = en-GB.
$voices = @(
    @{ Token = 'KokoroHeart';   Display = 'Kokoro Heart (en-US)';   VoiceName = 'af_heart';   Gender = 'Female'; Language = '409' },
    @{ Token = 'KokoroBella';   Display = 'Kokoro Bella (en-US)';   VoiceName = 'af_bella';   Gender = 'Female'; Language = '409' },
    @{ Token = 'KokoroMichael'; Display = 'Kokoro Michael (en-US)'; VoiceName = 'am_michael'; Gender = 'Male';   Language = '409' },
    @{ Token = 'KokoroEmma';    Display = 'Kokoro Emma (en-GB)';    VoiceName = 'bf_emma';    Gender = 'Female'; Language = '809' }
)

# Chrome enumerates voices from HKLM\...\Speech_OneCore\Voices (hardcoded in
# content/browser/speech/tts_win.cc, classic SAPI only as fallback), so -Machine
# writes the tokens there too. It skips tokens without an Attributes\Language
# value, which is why that attribute is required below.
$tokenRoots = @('HKCU:\SOFTWARE\Microsoft\Speech\Voices\Tokens')
if ($Machine) {
    $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $admin) { throw '-Machine requires an elevated prompt' }
    $tokenRoots += 'HKLM:\SOFTWARE\Microsoft\Speech\Voices\Tokens'
    $tokenRoots += 'HKLM:\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens'
}

foreach ($root in $tokenRoots) {
    foreach ($v in $voices) {
        $base = "$root\$($v.Token)"
        New-Item -Force -Path "$base\Attributes" | Out-Null
        Set-ItemProperty -Path $base -Name '(default)' -Value $v.Display
        Set-ItemProperty -Path $base -Name 'CLSID' -Value $clsid
        Set-ItemProperty -Path $base -Name 'VoiceName' -Value $v.VoiceName
        Set-ItemProperty -Path $base -Name 'AssetsDir' -Value $AssetsDir
        Set-ItemProperty -Path "$base\Attributes" -Name 'Name' -Value $v.Display
        Set-ItemProperty -Path "$base\Attributes" -Name 'Gender' -Value $v.Gender
        Set-ItemProperty -Path "$base\Attributes" -Name 'Age' -Value 'Adult'
        Set-ItemProperty -Path "$base\Attributes" -Name 'Vendor' -Value 'Kokoro'
        Set-ItemProperty -Path "$base\Attributes" -Name 'Language' -Value $v.Language
        Write-Host "  voice: $($v.Display) [$root]"
    }
}

Write-Host "`nDone. Test with scripts\test-speak.ps1, then scripts\set-default-voice.ps1 for Chrome."
