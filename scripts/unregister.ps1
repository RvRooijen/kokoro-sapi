# Removes the per-user registration created by register.ps1. Leaves the assets
# in %LOCALAPPDATA%\KokoroSapi alone (delete that folder manually if wanted).

$ErrorActionPreference = 'SilentlyContinue'

$clsid = '{6A2C7F52-3B19-4E5D-9C01-8F4A2D7B61E3}'
Remove-Item -Recurse -Force "HKCU:\Software\Classes\CLSID\$clsid"

$tokenRoots = @(
    'HKCU:\SOFTWARE\Microsoft\Speech\Voices\Tokens',
    'HKLM:\SOFTWARE\Microsoft\Speech\Voices\Tokens',
    'HKLM:\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens'
)
foreach ($t in 'KokoroHeart', 'KokoroBella', 'KokoroMichael', 'KokoroEmma') {
    foreach ($root in $tokenRoots) {
        Remove-Item -Recurse -Force "$root\$t"
        if (Test-Path "$root\$t") {
            Write-Host "Could not remove $root\$t - run elevated to remove -Machine registrations."
        }
    }
}

# If a Kokoro voice was made the default (set-default-voice.ps1), clear it so
# the default does not point at a token that no longer exists.
$voicesKey = 'HKCU:\SOFTWARE\Microsoft\Speech\Voices'
$default = (Get-ItemProperty -Path $voicesKey -Name 'DefaultTokenId').DefaultTokenId
if ($default -like '*\Tokens\Kokoro*') {
    Remove-ItemProperty -Path $voicesKey -Name 'DefaultTokenId'
    Write-Host 'Cleared default voice (pointed at a Kokoro token).'
}

Write-Host 'Unregistered.'
