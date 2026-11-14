$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$packageRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$serverPath = Join-Path $packageRoot 'slimeybeats-mcp.exe'
$desktopPath = Join-Path $packageRoot 'SlimeyBeats-EZ-Master.exe'
if (!(Test-Path -LiteralPath $serverPath -PathType Leaf)) { throw 'Extract the complete companion ZIP before running setup.' }
function Select-AudioFolder([string]$description) {
    $dialog = New-Object System.Windows.Forms.FolderBrowserDialog
    $dialog.Description = $description
    $dialog.ShowNewFolderButton = $true
    try {
        if ($dialog.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) { throw 'Setup cancelled. No AI-client configuration was changed.' }
        return $dialog.SelectedPath
    } finally { $dialog.Dispose() }
}
$inputFolder = Select-AudioFolder 'Choose the audio folder this AI connection may read. Use a dedicated folder for the tracks you want to share.'
$exportFolder = Select-AudioFolder 'Choose a folder for new exports and temporary previews.'
$consoleOutputEncoding = [Console]::OutputEncoding
try {
    # Windows PowerShell 5 uses this encoding to decode native stdout; Rust emits UTF-8.
    [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
    $configuration = & $serverPath --input-root $inputFolder --output-root $exportFolder --desktop-exe $desktopPath --print-config
    if ($LASTEXITCODE -ne 0) { throw 'The companion could not generate a configuration.' }
} finally { [Console]::OutputEncoding = $consoleOutputEncoding }
$save = New-Object System.Windows.Forms.SaveFileDialog
$save.Title = 'Save your MCP client configuration example'
$save.Filter = 'JSON configuration (*.json)|*.json'
$save.FileName = 'slimeybeats-mcp-client.json'
$save.OverwritePrompt = $true
try {
    if ($save.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) { throw 'Setup cancelled. No AI-client configuration was changed.' }
    [System.IO.File]::WriteAllText($save.FileName, ($configuration -join [Environment]::NewLine), (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "Saved example: $($save.FileName)"
    Write-Host 'Add its command and arguments to your MCP client, then restart that connection.'
    Write-Host 'This helper does not edit any AI-client settings. See docs/MCP.md for setup and privacy.'
} finally { $save.Dispose() }
