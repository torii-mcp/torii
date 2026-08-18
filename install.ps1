<#
.SYNOPSIS
    Instalador do Torii para Windows x86_64.

.DESCRIPTION
    Baixa o pacote publicado, confere o SHA-256 e instala o binário em um diretório
    do usuário. Nada exige privilégio de administrador. O PATH do usuário é
    atualizado quando o destino ainda não está nele, a menos que -NoPathUpdate.

.PARAMETER Version
    Versão a instalar, como v0.2.0. O padrão é a última release publicada.

.PARAMETER InstallDir
    Destino do binário. O padrão é $env:LOCALAPPDATA\Programs\Torii.

.PARAMETER NoPathUpdate
    Não altera o PATH do usuário.

.EXAMPLE
    irm https://raw.githubusercontent.com/torii-mcp/torii/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version v0.2.0 -NoPathUpdate
#>

[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir,
    [switch]$NoPathUpdate
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = 'torii-mcp/torii'
$platform = 'windows-x86_64'

function Write-Step($message) { Write-Host "torii: $message" }

function Get-LatestVersion {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
        if ($release.tag_name) { return $release.tag_name }
    } catch {
        throw "could not resolve the latest Torii version ($($_.Exception.Message)); pass -Version vX.Y.Z"
    }
    throw 'the latest release carried no tag; pass -Version vX.Y.Z'
}

function Assert-Platform {
    $architecture = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $architecture = $env:PROCESSOR_ARCHITEW6432 }
    if ($architecture -ne 'AMD64') {
        throw "no Torii release for $architecture; build it from source with ``cargo build --release``"
    }
}

function Get-InstalledVersion($path) {
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    try { return (& $path --version 2>$null) } catch { return $null }
}

# O binário em execução não pode ser sobrescrito no Windows, mas pode ser
# renomeado: o processo atual segue com o arquivo antigo e o novo assume o nome.
function Install-Binary($source, $destination) {
    if (Test-Path -LiteralPath $destination) {
        $retired = "$destination.old"
        if (Test-Path -LiteralPath $retired) {
            try { Remove-Item -LiteralPath $retired -Force } catch { }
        }
        try {
            Rename-Item -LiteralPath $destination -NewName (Split-Path $retired -Leaf) -Force
        } catch {
            throw "could not replace $destination ($($_.Exception.Message)); close any running torii and retry"
        }
    }
    Copy-Item -LiteralPath $source -Destination $destination -Force
}

function Update-UserPath($directory) {
    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($current) { $entries = $current -split ';' | Where-Object { $_ -ne '' } }
    if ($entries -contains $directory) {
        Write-Step "$directory is already in your user PATH"
        return
    }
    if ($NoPathUpdate) {
        Write-Step "$directory is not in your PATH. Add it with:"
        Write-Host ""
        Write-Host "    [Environment]::SetEnvironmentVariable('Path', `"$directory;`" + [Environment]::GetEnvironmentVariable('Path','User'), 'User')"
        Write-Host ""
        return
    }
    $updated = (@($directory) + $entries) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
    $env:Path = "$directory;$env:Path"
    Write-Step "Added $directory to your user PATH; open a new terminal to pick it up"
}

Assert-Platform

if (-not $Version) { $Version = Get-LatestVersion }
if ($Version -notlike 'v*') { $Version = "v$Version" }
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Torii' }

$package = "torii-$Version-$platform"
$archive = "$package.zip"
$baseUrl = "https://github.com/$repo/releases/download/$Version"
$workdir = Join-Path ([IO.Path]::GetTempPath()) ("torii-install-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $workdir -Force | Out-Null

try {
    $archivePath = Join-Path $workdir $archive
    $checksumPath = "$archivePath.sha256"

    Write-Step "Downloading $archive"
    Invoke-WebRequest -Uri "$baseUrl/$archive" -OutFile $archivePath -UseBasicParsing
    Invoke-WebRequest -Uri "$baseUrl/$archive.sha256" -OutFile $checksumPath -UseBasicParsing

    # O arquivo publicado traz "<hash>  <nome>"; só o primeiro campo interessa, e a
    # quebra de linha pode vir com CR.
    $expected = ((Get-Content -LiteralPath $checksumPath -Raw) -split '\s+' |
        Where-Object { $_ -ne '' } | Select-Object -First 1)
    if (-not $expected) { throw 'the published checksum file was empty' }
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) {
        throw "checksum mismatch for ${archive}: expected $expected, got $actual"
    }
    Write-Step 'Checksum verified'

    Expand-Archive -LiteralPath $archivePath -DestinationPath $workdir -Force
    $binary = Join-Path $workdir "$package\torii.exe"
    if (-not (Test-Path -LiteralPath $binary)) {
        throw 'the archive did not contain the expected torii.exe'
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $destination = Join-Path $InstallDir 'torii.exe'
    $previous = Get-InstalledVersion $destination
    Install-Binary $binary $destination

    $installed = Get-InstalledVersion $destination
    if ($previous -and $previous -ne $installed) {
        Write-Step "Updated $previous to $installed at $destination"
    } else {
        Write-Step "Installed $installed at $destination"
    }

    # O binário anterior só some quando nada mais o mantém aberto; falhar aqui é
    # apenas um arquivo a mais no diretório, não um erro de instalação.
    Remove-Item -LiteralPath "$destination.old" -Force -ErrorAction SilentlyContinue

    Update-UserPath $InstallDir
    Write-Step 'Next: torii init, then torii provider install <name>.'
} finally {
    Remove-Item -LiteralPath $workdir -Recurse -Force -ErrorAction SilentlyContinue
}
