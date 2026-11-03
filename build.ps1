# Gera dist\Navi_Setup.exe (release otimizado + instalador).
# Sem 'Stop': o cargo escreve progresso no stderr; os erros são checados por $LASTEXITCODE.
Set-Location $PSScriptRoot

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build falhou' }

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 nao encontrado' }

& $iscc "/DAppVersion=$version" installer\navi.iss
if ($LASTEXITCODE -ne 0) { throw 'ISCC falhou' }
"Pronto: dist\Navi_Setup.exe (v$version)"
