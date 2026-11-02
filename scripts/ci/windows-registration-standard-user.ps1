param(
    [ValidateSet('Controller', 'Child', 'Admin')][string]$Mode = 'Controller',
    [Parameter(Mandatory=$true)][string]$Stage,
    [string]$Hgs,
    [ValidateSet('raw', 'app-local')][string]$Layout = 'raw'
)
# Disposable hosted Windows only. No service, logon item or product task is run.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($Mode -eq 'Controller' -and ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted')) {
    throw 'This account-creation fixture requires a disposable GitHub-hosted runner'
}
if ($Mode -ne 'Controller') {
    $script:PSModuleAutoLoadingPreference = 'None'
    $phasePath = [IO.Path]::Combine($Stage, 'child-phases.txt')
    [IO.File]::WriteAllText($phasePath, "script_start`n")
}

$captureSource = @'
using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
public static class RegistrationCapture {
    public static Task<string> Read(Stream stream) {
        return Task.Run(() => {
            var bytes = new byte[65537]; int count = 0;
            while (count < bytes.Length) {
                int n = stream.Read(bytes, count, bytes.Length - count);
                if (n == 0) return new UTF8Encoding(false, true).GetString(bytes, 0, count);
                count += n;
            }
            throw new InvalidDataException("capture exceeded 64 KiB");
        });
    }
    public static Task Stop(Process child) {
        // A native termination request can itself stall. Keep it off the
        // caller deadline and retain the original Process until a real wait.
        return Task.Run(() => { child.Kill(); child.WaitForExit(); });
    }
}
'@

$serializerAssembly = [Reflection.Assembly]::Load('System.Web.Extensions, Version=4.0.0.0, Culture=neutral, PublicKeyToken=31bf3856ad364e35')
$reportJson = [Activator]::CreateInstance($serializerAssembly.GetType('System.Web.Script.Serialization.JavaScriptSerializer', $true))
$reportJson.MaxJsonLength = 262144

function Write-Report($Path, $Value) {
    [IO.File]::WriteAllText($Path, $reportJson.Serialize($Value), [Text.UTF8Encoding]::new($false))
}

if ($Mode -ne 'Controller') {
    [IO.File]::AppendAllText($phasePath, "serializer_ready`n")
    # The task supplies a real new standard-user token, not an unelevated admin.
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $administratorSidPresent = $false
    foreach ($group in $identity.Groups) {
        if ($group.Value -eq 'S-1-5-32-544') { $administratorSidPresent = $true }
    }
    $report = [ordered]@{
        layout = $Layout
        sid = $identity.User.Value
        administrator_sid_present = $administratorSidPresent
        session = [Diagnostics.Process]::GetCurrentProcess().SessionId
        calls = @()
        complete = $false
    }
    $reportPath = [IO.Path]::Combine($Stage, 'result.json')
    Write-Report $reportPath $report
    if ($administratorSidPresent -ne ($Mode -eq 'Admin')) { throw 'Unexpected account privilege class' }
    # Keep Windows' actual account profile. Only Zerus roots and temporary files
    # are private, matching the cold installer acceptance environment.
    foreach ($name in @('home', 'config', 'state', 'tmp')) {
        [void][IO.Directory]::CreateDirectory(([IO.Path]::Combine($Stage, $name)))
    }
    $env:HOME = [IO.Path]::Combine($Stage, 'home')
    $env:HGS_CONFIG_DIR = [IO.Path]::Combine($Stage, 'config')
    $env:HGS_STATE_DIR = [IO.Path]::Combine($Stage, 'state')
    $env:TEMP = [IO.Path]::Combine($Stage, 'tmp')
    $env:TMP = $env:TEMP
    $env:HGS_SELF = 'windows-registration-standard-fixture'
    $env:HGS_PEERS = ''
    $env:HGS_DESKTOP_REGISTRATION_TRACE = '1'
    # Read loops retain at most 64 KiB each, even if a failed child keeps writing.
    [void][Reflection.Assembly]::Load([IO.File]::ReadAllBytes([IO.Path]::Combine($Stage, 'capture.dll')))
    foreach ($arguments in @('--version', 'desktop registration status --json')) {
        $child = [Diagnostics.Process]::new()
        $child.StartInfo.FileName = [IO.Path]::Combine($Stage, 'hgs.exe')
        $child.StartInfo.Arguments = $arguments
        $child.StartInfo.UseShellExecute = $false
        $child.StartInfo.CreateNoWindow = $true
        $child.StartInfo.RedirectStandardInput = $true
        $child.StartInfo.RedirectStandardOutput = $true
        $child.StartInfo.RedirectStandardError = $true
        $call = [ordered]@{ arguments=$arguments; phase='spawn'; physical_wait=$false }
        $report.calls += $call
        Write-Report $reportPath $report
        $watch = [Diagnostics.Stopwatch]::StartNew()
        [IO.File]::AppendAllText($phasePath, "hgs_spawn`n")
        [void]$child.Start()
        # Opening the ORIGINAL Process handle fixes PID reuse before recording.
        $originalHandle = $child.Handle
        $call.pid = $child.Id
        $call.birth = $child.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
        $call.phase = 'started'
        $child.StandardInput.Close() # Real EOF, never the runner's synchronous pipe.
        $stdout = [RegistrationCapture]::Read($child.StandardOutput.BaseStream)
        $stderr = [RegistrationCapture]::Read($child.StandardError.BaseStream)
        Write-Report $reportPath $report
        if (-not $child.WaitForExit(20000)) {
            $stop = [RegistrationCapture]::Stop($child)
            try { [void]$stop.Wait(2000) } catch { }
            $call.phase = 'timeout-or-termination-pending'
            Write-Report $reportPath $report
            throw 'Original release child exceeded the fixed test deadline; preserve fixture'
        }
        [IO.File]::AppendAllText($phasePath, "hgs_exit`n")
        $call.physical_wait = $true
        $call.exit = $child.ExitCode
        $call.elapsed_ms = $watch.ElapsedMilliseconds
        if (-not [Threading.Tasks.Task]::WaitAll(@($stdout, $stderr), 2000)) {
            $call.phase = 'capture-pending'
            Write-Report $reportPath $report
            throw 'Original release output is incomplete; preserve fixture'
        }
        $call.stdout = $stdout.GetAwaiter().GetResult()
        $call.stderr = $stderr.GetAwaiter().GetResult()
        $call.phase = 'exited'
        Write-Report $reportPath $report
        $child.Dispose()
        if ($call.exit -ne 0) { throw 'Original release command failed; preserve fixture' }
        if ($arguments -ne '--version') {
            $value = $reportJson.DeserializeObject($call.stdout)
            if ($value.registered -ne $false) { throw 'Fresh fixture must have no registration' }
        }
    }
    $report.complete = $true
    Write-Report $reportPath $report
    return
}

$source = (Resolve-Path $Hgs).Path
if ([IO.Path]::GetFileName($source) -ne 'hgs.exe' -or [IO.Path]::GetFileName([IO.Path]::GetDirectoryName($source)) -ne 'release') {
    throw 'Exact release hgs.exe required'
}
$nonce = [Guid]::NewGuid().ToString('N')
$userName = 'zr' + $nonce.Substring(0, 12)
$taskName = 'ZerusRegistrationCI-' + $nonce
$Stage = [IO.Path]::GetFullPath($Stage)
if (Test-Path $Stage) { throw 'Fixture stage must not exist' }
[void][IO.Directory]::CreateDirectory($Stage)
$digest = (Get-FileHash $source -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "Registration RELEASE image SHA256 $digest"
$passwordBytes = New-Object byte[] 32
[Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($passwordBytes)
$password = [Convert]::ToBase64String($passwordBytes) + '!aA1'
$secure = ConvertTo-SecureString $password -AsPlainText -Force
$ownedUser = $null
$ownedTask = $false
$complete = $false
try {
    # Compile the bounded reader once under the warm controller. The fresh
    # account loads bytes only; no Add-Type/JSON/path cmdlet autoloading.
    $capturePath = Join-Path $Stage 'capture.dll'
    Add-Type -TypeDefinition $captureSource -OutputAssembly $capturePath
    # Same RELEASE image and command wrapper under the existing admin runner,
    # before the actual standard-account comparison. Both records stay separate.
    if ($Layout -eq 'raw') {
        $adminStage = Join-Path $Stage 'admin-control'
        [void][IO.Directory]::CreateDirectory($adminStage)
        Copy-Item $source (Join-Path $adminStage 'hgs.exe')
        Copy-Item $capturePath (Join-Path $adminStage 'capture.dll')
        & $PSCommandPath -Mode Admin -Stage $adminStage -Layout $Layout
    }
    $ownedUser = New-LocalUser -Name $userName -Password $secure -Description 'Disposable Zerus CI fixture' -AccountNeverExpires
    # Explicit ordinary Users membership; never add an administrator membership.
    Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $ownedUser
    & icacls.exe $Stage /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' "*$($ownedUser.SID.Value):(OI)(CI)M" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Private account ACL setup failed' }
    Copy-Item $source (Join-Path $Stage 'hgs.exe')
    if ($Layout -eq 'app-local') {
        # Exact PC package sidecars, already independently hash-checked by the
        # fixed artifact preparer. Record copied bytes, never use a system DLL.
        foreach ($name in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll')) {
            $original = Join-Path ([IO.Path]::GetDirectoryName($source)) $name
            $destination = Join-Path $Stage $name
            $expected = (Get-FileHash $original -Algorithm SHA256).Hash
            Copy-Item $original $destination
            if ((Get-FileHash $destination -Algorithm SHA256).Hash -cne $expected) { throw 'CRT copy differs' }
            Write-Output "App-local $name SHA256 $($expected.ToLowerInvariant())"
        }
    }
    Copy-Item $PSCommandPath (Join-Path $Stage 'fixture.ps1')
    if ((Get-FileHash (Join-Path $Stage 'hgs.exe') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $digest) {
        throw 'Release image copy differs'
    }
    $program = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $scriptPath = (Join-Path $Stage 'fixture.ps1').Replace("'", "''")
    $stageLiteral = $Stage.Replace("'", "''")
    # Fixed owned source loader, like the maintained policy corpus. This does
    # not change any execution-policy setting for the new account or machine.
    $entry = "& ([ScriptBlock]::Create([IO.File]::ReadAllText('$scriptPath'))) -Mode Child -Layout $Layout -Stage '$stageLiteral'"
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($entry))
    if ($encoded.Length -ge 30000) { throw 'Fixture entry exceeds command-line bound' }
    $arguments = '-NoLogo -NoProfile -NonInteractive -EncodedCommand ' + $encoded
    $action = New-ScheduledTaskAction -Execute $program -Argument $arguments -WorkingDirectory $Stage
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 1)
    # No trigger; exactly one explicit run. Password stays in controller memory
    # and Task Scheduler's credential store until this exact task is removed.
    Register-ScheduledTask -TaskName $taskName -Action $action -Settings $settings -User "$env:COMPUTERNAME\$userName" -Password $password -RunLevel Limited | Out-Null
    $ownedTask = $true
    $password = $null
    $secure = $null
    Start-ScheduledTask -TaskName $taskName
    $deadline = [DateTime]::UtcNow.AddSeconds(65)
    do {
        Start-Sleep -Milliseconds 250
        $task = Get-ScheduledTask -TaskName $taskName
        $info = Get-ScheduledTaskInfo -TaskName $taskName
    } while (($info.LastRunTime.Year -lt 2000 -or $task.State -eq 'Running') -and [DateTime]::UtcNow -lt $deadline)
    $launch = [ordered]@{
        state = [string]$task.State
        logon_type = [string]$task.Principal.LogonType
        last_run = $info.LastRunTime.ToUniversalTime().ToString('o')
        last_result = [uint32]$info.LastTaskResult
        result_present = [IO.File]::Exists((Join-Path $Stage 'result.json'))
    }
    $phasePath = Join-Path $Stage 'child-phases.txt'
    if ([IO.File]::Exists($phasePath)) {
        if ((Get-Item $phasePath).Length -gt 1024) { throw 'Fixture phase bound exceeded' }
        $launch.phases = [IO.File]::ReadAllText($phasePath)
    }
    Write-Report (Join-Path $env:GITHUB_WORKSPACE ('artifacts/test-results/registration-' + $Layout + '-launch.json')) $launch
    Write-Output ($reportJson.Serialize($launch))
    $path = Join-Path $Stage 'result.json'
    if (Test-Path $path) {
        $text = [IO.File]::ReadAllText($path)
        if ($text.Length -gt 200000) { throw 'Fixture result exceeds bound' }
        [IO.File]::WriteAllText((Join-Path $env:GITHUB_WORKSPACE ('artifacts/test-results/registration-' + $Layout + '-standard-user.json')), $text)
        Write-Output $text
        $result = $text | ConvertFrom-Json
        $complete = $result.complete -eq $true -and $result.sid -eq $ownedUser.SID.Value -and $result.administrator_sid_present -eq $false -and $result.calls.Count -eq 2 -and $task.State -ne 'Running' -and $info.LastTaskResult -eq 0
    }
    if (-not $complete) { throw 'Standard-user release comparison failed; retained only on disposable VM' }
} finally {
    $adminReport = Join-Path $Stage 'admin-control/result.json'
    if (Test-Path $adminReport) {
        $text = [IO.File]::ReadAllText($adminReport)
        if ($text.Length -gt 200000) { throw 'Admin fixture result exceeds bound' }
        [IO.File]::WriteAllText((Join-Path $env:GITHUB_WORKSPACE 'artifacts/test-results/registration-release-admin.json'), $text)
        Write-Output $text
    }
    # Remove only our exact triggerless task; never stop or adopt another task.
    if ($ownedTask) {
        $actual = Get-ScheduledTask -TaskName $taskName
        if ($actual.Actions.Execute -ne $program -or $actual.Actions.Arguments -ne $arguments) { throw 'Fixture task changed; preserve account and stage' }
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
    if ($complete) {
        $profileDeadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $profile = Get-CimInstance Win32_UserProfile -Filter "SID='$($ownedUser.SID.Value)'"
            if (-not $profile.Loaded) { break }
            Start-Sleep -Milliseconds 100
        } while ([DateTime]::UtcNow -lt $profileDeadline)
        if ($profile.Loaded) { throw 'Original fixture profile remains loaded' }
        if ($profile) { $profile | Remove-CimInstance }
        Remove-LocalUser -SID $ownedUser.SID
        Remove-Item -LiteralPath $Stage -Recurse
        Write-Output 'Standard-user account, profile, task and stage removed'
    } else {
        Write-Output "Incomplete fixture preserved on disposable VM only: $Stage; no product effects replayed"
    }
}
