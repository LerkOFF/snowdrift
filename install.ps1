# Установка Snowdrift на Windows одной командой (PowerShell):
#   irm https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.ps1 | iex
# Удаление:
#   & "$env:LOCALAPPDATA\Programs\Snowdrift\Snowdrift.exe" --uninstall
# Права администратора не нужны: всё ставится в профиль пользователя.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

if ($env:OS -ne 'Windows_NT') {
    Write-Error 'Этот скрипт для Windows. На macOS: curl -fsSL https://raw.githubusercontent.com/LerkOFF/snowdrift/main/install.sh | bash'
    return
}

$repo = 'LerkOFF/snowdrift'
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('snowdrift-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $exe = Join-Path $tmp 'Snowdrift.exe'
    Write-Host 'Скачиваю последний релиз Snowdrift...'
    Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/$repo/releases/latest/download/Snowdrift.exe" -OutFile $exe
    # Сам копируется в %LOCALAPPDATA%\Programs\Snowdrift, регистрирует удаление и запускается.
    & $exe --install
    Write-Host 'Готово. Снежинка появилась в системном трее, там же настройки.'
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
