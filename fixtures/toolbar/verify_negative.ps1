$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

$repoPath = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$fixtureCss = Join-Path $PSScriptRoot 'unsupported.css'
$fixtureHtml = Join-Path $PSScriptRoot 'source.html'
$stderrPath = New-TemporaryFile

Push-Location -LiteralPath $repoPath
try {
    $rust = cargo run --locked --quiet --example generate_gpui -- $fixtureHtml $fixtureCss 2> $stderrPath
    $exitCode = $LASTEXITCODE
    $diagnostic = Get-Content -LiteralPath $stderrPath -Raw

    if ($exitCode -eq 0 -or $null -ne $rust) {
        throw 'The generator must fail without writing Rust for unsupported CSS.'
    }
    $location = [regex]::Escape($fixtureCss) + ':2:5: source loss in \.toolbar:'
    if ($diagnostic -notmatch $location -or $diagnostic -notmatch "property 'filter'.*will be dropped") {
        throw "Expected a source-linked filter diagnostic at line 2, column 5. Got:`n$diagnostic"
    }
    Write-Output $diagnostic.TrimEnd()
} finally {
    Pop-Location
    Remove-Item -LiteralPath $stderrPath
}
