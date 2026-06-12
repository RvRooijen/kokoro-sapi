# Smoke test via native SAPI (SAPI.SpVoice COM object) - the same path Chrome
# and Edge use. Unlike System.Speech (.NET), native SAPI also enumerates
# per-user (HKCU) voices, so the Kokoro voices show up here. Works in both
# Windows PowerShell 5.1 and pwsh 7.
#
# Without assets downloaded the voice speaks a 440 Hz test tone, which still
# proves COM registration + the SAPI plumbing work.

param([string]$Voice = 'Kokoro Heart (en-US)')

$sp = New-Object -ComObject SAPI.SpVoice
$tokens = $sp.GetVoices()

Write-Host 'Installed voices (native SAPI, incl. HKCU):'
for ($i = 0; $i -lt $tokens.Count; $i++) {
    Write-Host ('  ' + $tokens.Item($i).GetDescription())
}

$match = $null
for ($i = 0; $i -lt $tokens.Count; $i++) {
    if ($tokens.Item($i).GetDescription() -like "*$Voice*") { $match = $tokens.Item($i); break }
}
if (-not $match) { throw "Voice '$Voice' not found - did register.ps1 run?" }

Write-Host ""
Write-Host ('Speaking with: ' + $match.GetDescription())
$sp.Voice = $match
$null = $sp.Speak('Hello Rick! This is Kokoro speaking through SAPI. If you can hear this, the bridge works.')
Write-Host 'Done. Verify word-boundary events via the highlighting in Chrome reading mode.'
