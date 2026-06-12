# Makes a Kokoro voice the default SAPI voice for the current user.
#
# Chrome's reading mode does not list individual SAPI voices - it shows a
# single "System text-to-speech voice" entry that uses the SAPI default.
# Setting the default is therefore how Kokoro gets selected there. Note this
# affects every app that speaks with the default voice.
#
# Usage:  .\set-default-voice.ps1                   (Kokoro Heart)
#         .\set-default-voice.ps1 -Token KokoroEmma
#         .\set-default-voice.ps1 -Reset            (back to the system fallback)

param(
    [string]$Token = 'KokoroHeart',
    [switch]$Reset
)

$ErrorActionPreference = 'Stop'
$key = 'HKCU:\SOFTWARE\Microsoft\Speech\Voices'

if ($Reset) {
    Remove-ItemProperty -Path $key -Name 'DefaultTokenId' -ErrorAction SilentlyContinue
    Write-Host 'Default voice reset to the system fallback.'
    return
}

if (-not (Test-Path "$key\Tokens\$Token")) {
    throw "Voice token '$Token' is not registered - run register.ps1 first"
}

if (-not (Test-Path $key)) { New-Item -Path $key | Out-Null }
Set-ItemProperty -Path $key -Name 'DefaultTokenId' `
    -Value "HKEY_CURRENT_USER\SOFTWARE\Microsoft\Speech\Voices\Tokens\$Token"

Write-Host "Default SAPI voice is now '$Token'. Verify with:"
Write-Host '  (New-Object -ComObject SAPI.SpVoice).Voice.GetDescription()'
Write-Host 'Restart Chrome, then pick "System text-to-speech voice" in reading mode.'
