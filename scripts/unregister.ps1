# Removes the per-user registration created by register.ps1. Leaves the assets
# in %LOCALAPPDATA%\KokoroSapi alone (delete that folder manually if wanted).

$ErrorActionPreference = 'SilentlyContinue'

$clsid = '{6A2C7F52-3B19-4E5D-9C01-8F4A2D7B61E3}'
Remove-Item -Recurse -Force "HKCU:\Software\Classes\CLSID\$clsid"

foreach ($t in 'KokoroHeart', 'KokoroBella', 'KokoroMichael', 'KokoroEmma') {
    Remove-Item -Recurse -Force "HKCU:\SOFTWARE\Microsoft\Speech\Voices\Tokens\$t"
}

Write-Host 'Unregistered.'
