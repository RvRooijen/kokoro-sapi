# Smoke test via native SAPI (SAPI.SpVoice COM object) - the same path Chrome
# takes for its "System text-to-speech voice".
#
# NOTE: SAPI only *enumerates* machine-wide (HKLM) voices; per-user (HKCU)
# tokens never show up in voice lists, but work fine when opened by id -
# which is what this script does, and what the SAPI default-voice mechanism
# does for Chrome (see set-default-voice.ps1). To make the voices enumerable
# (Edge read aloud, reader extensions), register them machine-wide with
# register.ps1 -Machine from an elevated prompt.
#
# Without assets downloaded the voice speaks a 440 Hz test tone, which still
# proves COM registration + the SAPI plumbing work.

param([string]$Token = 'KokoroHeart')

$ErrorActionPreference = 'Stop'
$sp = New-Object -ComObject SAPI.SpVoice

Write-Host 'Enumerable voices (HKLM only; per-user Kokoro voices will not appear here):'
$tokens = $sp.GetVoices()
for ($i = 0; $i -lt $tokens.Count; $i++) {
    Write-Host ('  ' + $tokens.Item($i).GetDescription())
}

$tok = New-Object -ComObject SAPI.SpObjectToken
$tok.SetId("HKEY_CURRENT_USER\SOFTWARE\Microsoft\Speech\Voices\Tokens\$Token", '', $false)

Write-Host ""
Write-Host ('Speaking with: ' + $tok.GetDescription())
$sp.Voice = $tok
$null = $sp.Speak('Hello Rick! This is Kokoro speaking through SAPI. If you can hear this, the bridge works.')
Write-Host 'Done. Verify word-boundary events via the highlighting in Chrome reading mode.'
