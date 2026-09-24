[CmdletBinding()]
param([switch]$Release, [string]$Target)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location (Join-Path $projectRoot 'frontend')
try {
    & npm ci
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    & npm run lint
    if ($LASTEXITCODE -ne 0) { throw 'Frontend lint failed' }
    & npm run format:check
    if ($LASTEXITCODE -ne 0) { throw 'Frontend formatting failed' }
    & npm run typecheck
    if ($LASTEXITCODE -ne 0) { throw 'Frontend typecheck failed' }
    & npm test
    if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed' }
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Next.js export failed' }
} finally { Pop-Location }
Push-Location $projectRoot
try {
    $cargoArguments = @('build', '--locked', '--package', 'codex2api')
    if ($Release) { $cargoArguments += '--release' }
    if ($Target) { $cargoArguments += @('--target', $Target) }
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
} finally { Pop-Location }
