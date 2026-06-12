# Makes a Kokoro voice the default voice for the current user, in both stacks:
#
# - Classic SAPI (HKCU\...\Speech\Voices): used by SpVoice without an explicit
#   SetVoice. Works with the per-user tokens from plain register.ps1.
# - OneCore (HKCU\...\Speech_OneCore\Voices): SAPI enumeration returns the
#   default token *first*, and Chrome's reading mode keeps one system voice
#   per language (the first one) - so this is what makes Chrome's "System
#   text-to-speech voice" resolve to Kokoro for en-US. Requires the HKLM
#   OneCore tokens from register.ps1 -Machine.
#
# Usage:  .\set-default-voice.ps1                   (Kokoro Heart)
#         .\set-default-voice.ps1 -Token KokoroEmma
#         .\set-default-voice.ps1 -Reset            (back to the system fallback)

param(
    [string]$Token = 'KokoroHeart',
    [switch]$Reset
)

$ErrorActionPreference = 'Stop'
$sapiKey = 'HKCU:\SOFTWARE\Microsoft\Speech\Voices'
$oneCoreKey = 'HKCU:\SOFTWARE\Microsoft\Speech_OneCore\Voices'

if ($Reset) {
    Remove-ItemProperty -Path $sapiKey -Name 'DefaultTokenId' -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $oneCoreKey -Name 'DefaultTokenId' -ErrorAction SilentlyContinue
    Write-Host 'Default voice reset to the system fallback (both SAPI and OneCore).'
    return
}

if (-not (Test-Path "$sapiKey\Tokens\$Token")) {
    throw "Voice token '$Token' is not registered - run register.ps1 first"
}

Set-ItemProperty -Path $sapiKey -Name 'DefaultTokenId' `
    -Value "HKEY_CURRENT_USER\SOFTWARE\Microsoft\Speech\Voices\Tokens\$Token"
Write-Host "Classic SAPI default voice is now '$Token'."

if (Test-Path "HKLM:\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens\$Token") {
    if (-not (Test-Path $oneCoreKey)) { New-Item -Path $oneCoreKey -Force | Out-Null }
    Set-ItemProperty -Path $oneCoreKey -Name 'DefaultTokenId' `
        -Value "HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens\$Token"
    Write-Host "OneCore default voice is now '$Token' (used by Chrome)."
} else {
    Write-Host 'No OneCore token found - run register.ps1 -Machine (elevated) to cover Chrome.'
}

Write-Host 'Restart Chrome fully, then pick "System text-to-speech voice" in reading mode.'
