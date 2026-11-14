<#
Run with Windows PowerShell 5.1 (no Pester or GUI required):
powershell.exe -NoProfile -NonInteractive -STA -ExecutionPolicy Bypass -File .\mcp\tools\verify_connect_config.ps1 -Server <release-exe> -Evidence <new-private-folder>
Only the supplied helper and server are read; all writes stay in the new evidence folder.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Server,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [string]$Helper
)
$ErrorActionPreference = 'Stop'
if (!$Helper) { $Helper = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) 'Connect-AI-Assistant.ps1' }
if ($PSVersionTable.PSEdition -ne 'Desktop' -or $PSVersionTable.PSVersion.Major -ne 5) {
    throw 'Run this regression with Windows PowerShell 5.1 powershell.exe.'
}
if (Test-Path -LiteralPath $Evidence) { throw 'Evidence must be a new private folder.' }
$serverSource = (Resolve-Path -LiteralPath $Server).ProviderPath
$helperSource = (Resolve-Path -LiteralPath $Helper).ProviderPath
$null = New-Item -ItemType Directory -Path $Evidence
$Evidence = (Resolve-Path -LiteralPath $Evidence).ProviderPath
# ASCII source is intentional: Windows PowerShell 5 reads BOM-less scripts as ANSI.
$unicode = 'caf' + [char]0xE9 + ' ' + [char]0x97F3 + [char]0x697D
$package = Join-Path $Evidence ('Package ' + $unicode)
$inputFolder = Join-Path $Evidence ('Library ' + $unicode)
$exportFolder = Join-Path $Evidence ('Exports ' + $unicode)
foreach ($folder in @($package, $inputFolder, $exportFolder)) {
    $null = New-Item -ItemType Directory -Path $folder
}
$serverPath = Join-Path $package 'slimeybeats-mcp.exe'
$helperPath = Join-Path $package 'Connect-AI-Assistant.ps1'
$desktopPath = Join-Path $package 'SlimeyBeats-EZ-Master.exe'
Copy-Item -LiteralPath $serverSource -Destination $serverPath
Copy-Item -LiteralPath $helperSource -Destination $helperPath
# --print-config canonicalizes this path; it never executes the desktop.
[System.IO.File]::WriteAllBytes($desktopPath, [byte[]]@())
$utf8 = New-Object System.Text.UTF8Encoding($false, $true)
$marker = Join-Path $inputFolder 'untouched.txt'
[System.IO.File]::WriteAllText($marker, 'private input sentinel', $utf8)
$serverHash = (Get-FileHash -LiteralPath $serverPath -Algorithm SHA256).Hash
$serverVersion = & $serverPath --version
if ($LASTEXITCODE -ne 0) { throw 'Could not run the supplied MCP server.' }
Add-Type -AssemblyName System.Windows.Forms

function Assert-True([bool]$condition, [string]$message) {
    if (!$condition) { throw $message }
}

function New-TestDialog([string]$kind) {
    $dialog = [pscustomobject]@{
        Kind = $kind; State = $testState; Description = ''; ShowNewFolderButton = $false
        SelectedPath = ''; Title = ''; Filter = ''; FileName = ''
        OverwritePrompt = $false; Disposed = $false
    }
    $dialog | Add-Member -MemberType ScriptMethod -Name ShowDialog -Value {
        $this.State.Shown++
        if ($this.Kind -eq 'folder') {
            $this.SelectedPath = $this.State.Selections[$this.State.FolderIndex++]
        } else {
            Assert-True ($this.OverwritePrompt) 'Save overwrite authorization was disabled.'
            Assert-True ($this.FileName -ceq 'slimeybeats-mcp-client.json') 'Save default changed.'
            $this.FileName = $this.State.SavePath
        }
        if ($this.State.CancelAt -eq $this.State.Shown) { return [System.Windows.Forms.DialogResult]::Cancel }
        return [System.Windows.Forms.DialogResult]::OK
    }
    $dialog | Add-Member -MemberType ScriptMethod -Name Dispose -Value { $this.Disposed = $true }
    $testState.Dialogs.Add($dialog)
    return $dialog
}

# Shadow only GUI construction in this test process; execute the unchanged helper.
function New-Object {
    param([string]$TypeName, [object[]]$ArgumentList)
    switch ($TypeName) {
        'System.Windows.Forms.FolderBrowserDialog' { return New-TestDialog 'folder' }
        'System.Windows.Forms.SaveFileDialog' { return New-TestDialog 'save' }
        default { return Microsoft.PowerShell.Utility\New-Object @PSBoundParameters }
    }
}

$originalConsoleOutput = [Console]::OutputEncoding
$originalConsoleInput = [Console]::InputEncoding
$originalPipelineOutput = $OutputEncoding
$results = [System.Collections.Generic.List[object]]::new()
try {
    foreach ($case in @(
        @{ Name = 'unicode-oem437'; CodePage = 437; Mode = 'success'; Cancel = 0 },
        @{ Name = 'unicode-shiftjis932'; CodePage = 932; Mode = 'success'; Cancel = 0 },
        @{ Name = 'unicode-utf8'; CodePage = 65001; Mode = 'success'; Cancel = 0 },
        @{ Name = 'native-failure'; CodePage = 437; Mode = 'native-failure'; Cancel = 0 },
        @{ Name = 'launch-failure'; CodePage = 932; Mode = 'launch-failure'; Cancel = 0 },
        @{ Name = 'cancel-input'; CodePage = 437; Mode = 'cancel'; Cancel = 1 },
        @{ Name = 'cancel-export'; CodePage = 437; Mode = 'cancel'; Cancel = 2 },
        @{ Name = 'cancel-save'; CodePage = 437; Mode = 'cancel'; Cancel = 3 },
        @{ Name = 'save-failure'; CodePage = 437; Mode = 'save-failure'; Cancel = 0 }
    )) {
        $testState = [pscustomobject]@{
            Shown = 0; FolderIndex = 0; Dialogs = [System.Collections.Generic.List[object]]::new()
            CancelAt = $case.Cancel; Selections = @($inputFolder, $exportFolder)
            SavePath = Join-Path $Evidence ($case.Name + '.json')
        }
        if ($case.Mode -eq 'native-failure') { $testState.Selections[0] = Join-Path $Evidence 'missing-library' }
        if ($case.Mode -eq 'launch-failure') { [System.IO.File]::WriteAllBytes($serverPath, [byte[]]@(0)) }
        if ($case.Mode -eq 'cancel') { [System.IO.File]::WriteAllText($testState.SavePath, 'do not overwrite', $utf8) }
        if ($case.Mode -eq 'save-failure') {
            $null = New-Item -ItemType Directory -Path $testState.SavePath
        }
        [Console]::InputEncoding = [System.Text.Encoding]::GetEncoding(850)
        [Console]::OutputEncoding = [System.Text.Encoding]::GetEncoding($case.CodePage,
            [System.Text.EncoderReplacementFallback]::new('[encode]'),
            [System.Text.DecoderReplacementFallback]::new('[decode]'))
        $OutputEncoding = [System.Text.Encoding]::GetEncoding(1252)
        $beforeConsole = [Console]::OutputEncoding
        $beforeInput = [Console]::InputEncoding
        $beforePipeline = $OutputEncoding
        $caught = $null
        $failure = $null
        try {
            try { & $helperPath *> (Join-Path $Evidence ($case.Name + '.log')) } catch { $caught = $_ }
            Assert-True ([Console]::OutputEncoding.Equals($beforeConsole)) 'Console output encoding and fallbacks were not restored.'
            Assert-True ([Console]::InputEncoding.Equals($beforeInput)) 'Console input encoding changed.'
            Assert-True ([object]::ReferenceEquals($OutputEncoding, $beforePipeline)) 'Pipeline output encoding changed.'
            Assert-True (@($testState.Dialogs | Where-Object { !$_.Disposed }).Count -eq 0) 'A dialog was not disposed.'
            Assert-True ([System.IO.File]::ReadAllText($marker, $utf8) -ceq 'private input sentinel') 'Input contents changed.'
            Assert-True (@(Get-ChildItem -LiteralPath $exportFolder -Force).Count -eq 0) 'Setup wrote to the export folder.'
            if ($case.Mode -eq 'success') {
                if ($caught) { throw $caught }
                Assert-True ($testState.Shown -eq 3) 'Expected two folder authorizations and one save authorization.'
                Assert-True ($testState.Dialogs[0].Description -like '*may read*' -and $testState.Dialogs[1].Description -like '*exports*') 'Folder authorization prompts changed.'
                Assert-True ($testState.Dialogs[0].ShowNewFolderButton -and $testState.Dialogs[1].ShowNewFolderButton) 'Folder creation option changed.'
                $bytes = [System.IO.File]::ReadAllBytes($testState.SavePath)
                Assert-True (!($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191)) 'JSON has a UTF-8 BOM.'
                $config = ($utf8.GetString($bytes) | ConvertFrom-Json).mcpServers.slimeybeats
                Assert-True ($config.command -ceq $serverPath) 'Executable path failed exact Unicode roundtrip.'
                # Rust canonicalizes existing paths with the Windows extended path prefix.
                $expectedArgs = @('--input-root', ('\\?\' + $inputFolder), '--output-root', $exportFolder,
                    '--max-decoded-mib', '512', '--decode-timeout-seconds', '300', '--desktop-exe', ('\\?\' + $desktopPath))
                Assert-True ($config.args.Count -eq $expectedArgs.Count) 'Generated argument count changed.'
                for ($i = 0; $i -lt $expectedArgs.Count; $i++) {
                    Assert-True ($config.args[$i] -ceq $expectedArgs[$i]) ('Argument ' + $i + ' failed exact Unicode roundtrip.')
                }
            } else {
                Assert-True ($null -ne $caught) 'Expected setup to fail or cancel.'
                if ($case.Mode -eq 'cancel') {
                    Assert-True ($caught.Exception.Message -like '*Setup cancelled*') 'Cancellation error changed.'
                    Assert-True ($testState.Shown -eq $case.Cancel) 'Setup continued after cancellation.'
                    Assert-True ([System.IO.File]::ReadAllText($testState.SavePath, $utf8) -ceq 'do not overwrite') 'Cancellation overwrote an existing file.'
                } elseif ($case.Mode -eq 'save-failure') {
                    Assert-True ($testState.Shown -eq 3) 'Save failure did not reach save authorization.'
                } else {
                    Assert-True ($testState.Shown -eq 2) 'Setup offered to save after generation failed.'
                    Assert-True (!(Test-Path -LiteralPath $testState.SavePath)) 'Failure wrote a configuration.'
                }
            }
        } catch { $failure = $_.Exception.Message } finally {
            if ($case.Mode -eq 'launch-failure') { Copy-Item -LiteralPath $serverSource -Destination $serverPath -Force }
        }
        $status = if ($failure) { 'FAIL' } else { 'PASS' }
        $results.Add([pscustomobject]@{
            name = $case.Name; status = $status; console_code_page = $case.CodePage
            failure = $failure; helper_error = if ($caught) { $caught.Exception.Message } else { $null }
        })
        Write-Host ($status + ' ' + $case.Name + $(if ($failure) { ': ' + $failure }))
    }
} finally {
    [Console]::OutputEncoding = $originalConsoleOutput
    [Console]::InputEncoding = $originalConsoleInput
    $OutputEncoding = $originalPipelineOutput
    $report = [ordered]@{
        powershell = $PSVersionTable.PSVersion.ToString(); edition = $PSVersionTable.PSEdition
        server_version = $serverVersion; server_sha256 = $serverHash
        helper_sha256 = (Get-FileHash -LiteralPath $helperPath -Algorithm SHA256).Hash
        cases = @($results)
    }
    $logPath = Join-Path $Evidence 'results.json'
    [System.IO.File]::WriteAllText($logPath, ($report | ConvertTo-Json -Depth 6), $utf8)
    Write-Host ('Results: ' + $logPath)
}
if (@($results | Where-Object { $_.status -eq 'FAIL' }).Count -ne 0) { exit 1 }
