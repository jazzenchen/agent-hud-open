# Where the current user's Start menu and desktop shortcuts point, for the
# executable names passed in $env:AGENTHUD_APP_NAMES (comma separated).
# Prints one JSON string per matching target. Reads shortcuts only.
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$names = @($env:AGENTHUD_APP_NAMES -split ',' | Where-Object { $_ } | ForEach-Object { $_.ToLowerInvariant() })
if ($names.Count -eq 0) { exit 0 }
$shell = New-Object -ComObject WScript.Shell
$roots = @(
    [Environment]::GetFolderPath('StartMenu'),
    [Environment]::GetFolderPath('CommonStartMenu'),
    [Environment]::GetFolderPath('Desktop'),
    [Environment]::GetFolderPath('CommonDesktopDirectory')
) | Where-Object { $_ -and (Test-Path $_) }
$seen = @{}
foreach ($root in $roots) {
    Get-ChildItem -LiteralPath $root -Recurse -Filter *.lnk -ErrorAction SilentlyContinue | ForEach-Object {
        $target = $shell.CreateShortcut($_.FullName).TargetPath
        if ($target -and -not $seen.ContainsKey($target)) {
            $leaf = [IO.Path]::GetFileName($target).ToLowerInvariant()
            if ($names -contains $leaf -and (Test-Path -LiteralPath $target -PathType Leaf)) {
                $seen[$target] = $true
                $target | ConvertTo-Json -Compress
            }
        }
    }
}
