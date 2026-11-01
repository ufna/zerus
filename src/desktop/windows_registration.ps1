# Functions only: hgs appends Invoke-RegistrationMain to its fixed encoded
# command. Tests can exercise the policy with an in-memory scheduler instead.
function Require-Registration([bool]$condition,[string]$message) {
    if (-not $condition) { throw $message }
}

function Get-DesktopTask($folder,[string]$name) {
    try { return $folder.GetTask($name) }
    catch {
        $exception=$_.Exception
        for ($i=0;$i -lt 4 -and $null -ne $exception.InnerException;$i++) { $exception=$exception.InnerException }
        if ($exception.HResult -eq -2147024894) { return $null } # ERROR_FILE_NOT_FOUND only
        throw
    }
}

function Resolve-TaskSid([string]$value) {
    Require-Registration ($value.Length -gt 0 -and $value.Length -le 256) 'Missing task user identity.'
    if ($value.StartsWith('S-1-',[StringComparison]::Ordinal)) {
        return ([Security.Principal.SecurityIdentifier]::new($value)).Value
    }
    return ([Security.Principal.NTAccount]::new($value)).Translate([Security.Principal.SecurityIdentifier]).Value
}

function Assert-DesktopTask($task,$plan,[string]$sid) {
    $definition=$task.Definition
    $description='Zerus desktop registration v1; configuration='+$plan.configuration
    Require-Registration ($definition.RegistrationInfo.Description -ceq $description) 'Existing task is not this managed configuration; it was kept.'
    Require-Registration ($definition.RegistrationInfo.URI -ceq ('\'+$plan.task_name)) 'Existing task URI differs; it was kept.'
    Require-Registration ((Resolve-TaskSid $definition.Principal.UserId) -ceq $sid) 'Existing task belongs to another user; it was kept.'
    Require-Registration ($definition.Principal.LogonType -eq 3 -and $definition.Principal.RunLevel -eq 0 -and [string]::IsNullOrEmpty($definition.Principal.GroupId)) 'Existing task is not an unelevated interactive-user task; it was kept.'
    Require-Registration ($definition.Principal.Id -ceq 'DesktopUser' -and $definition.Actions.Context -ceq 'DesktopUser') 'Existing task action context differs; it was kept.'
    Require-Registration ($definition.Actions.Count -eq 1 -and $definition.Triggers.Count -eq 1) 'Existing task has extra actions or triggers; it was kept.'
    $action=$definition.Actions.Item(1)
    $trigger=$definition.Triggers.Item(1)
    [xml]$wanted=$plan.definition
    $ns=[Xml.XmlNamespaceManager]::new($wanted.NameTable)
    $ns.AddNamespace('t','http://schemas.microsoft.com/windows/2004/02/mit/task')
    $expectedArguments=$wanted.SelectSingleNode('/t:Task/t:Actions/t:Exec/t:Arguments',$ns).InnerText
    Require-Registration ($action.Type -eq 0 -and $action.Path -ceq $plan.argv[0] -and $action.Arguments -ceq $expectedArguments -and [string]::IsNullOrEmpty($action.WorkingDirectory)) 'Existing task command differs; it was kept.'
    Require-Registration ($trigger.Type -eq 9 -and (Resolve-TaskSid $trigger.UserId) -ceq $sid -and $trigger.Enabled -eq $true) 'Existing task is not this user logon trigger; it was kept.'
    Require-Registration ([string]::IsNullOrEmpty($trigger.StartBoundary) -and [string]::IsNullOrEmpty($trigger.EndBoundary) -and [string]::IsNullOrEmpty($trigger.Delay)) 'Existing task has custom trigger timing; it was kept.'
    Require-Registration ([string]::IsNullOrEmpty($trigger.Repetition.Interval) -and [string]::IsNullOrEmpty($trigger.Repetition.Duration)) 'Existing task repeats its trigger; it was kept.'
    $settings=$definition.Settings
    Require-Registration ($task.Enabled -eq $true -and $settings.Enabled -eq $true -and $settings.Hidden -eq $true -and $settings.MultipleInstances -eq 2) 'Existing task lifecycle policy differs; it was kept.'
    Require-Registration ($settings.ExecutionTimeLimit -ceq 'PT0S' -and $settings.RestartInterval -ceq 'PT1M' -and $settings.RestartCount -eq 3) 'Existing task restart policy differs; it was kept.'
    Require-Registration ($settings.DisallowStartIfOnBatteries -eq $false -and $settings.StopIfGoingOnBatteries -eq $false -and $settings.StartWhenAvailable -eq $true) 'Existing task power policy differs; it was kept.'
    Require-Registration ($settings.RunOnlyIfIdle -eq $false -and $settings.RunOnlyIfNetworkAvailable -eq $false -and $settings.WakeToRun -eq $false) 'Existing task has extra runtime conditions; it was kept.'
    $descriptor=[Security.AccessControl.RawSecurityDescriptor]::new($task.GetSecurityDescriptor(5))
    Require-Registration ($descriptor.Owner.Value -ceq $sid -and $null -ne $descriptor.DiscretionaryAcl -and $descriptor.DiscretionaryAcl.Count -le 16) 'Existing task ownership/DACL is unknown; it was kept.'
    foreach ($ace in $descriptor.DiscretionaryAcl) {
        Require-Registration ($ace -is [Security.AccessControl.CommonAce] -and $ace.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessAllowed -and $ace.SecurityIdentifier.Value -cin @($sid,'S-1-5-18','S-1-5-32-544')) 'Existing task grants access to another principal; it was kept.'
    }
}

function Invoke-RegistrationOperation($request,$scheduler,[string]$sid) {
    Require-Registration ($sid -notin @('S-1-5-18','S-1-5-19','S-1-5-20') -and -not $sid.StartsWith('S-1-5-80-')) 'A system/service account cannot own a desktop logon registration.'
    $plan=$request.plan
    Require-Registration ($request.schema -eq 1 -and $request.operation -cin @('check','status','install','remove')) 'Unsupported registration request.'
    Require-Registration ($plan.schema -eq 1 -and $plan.operation -ceq 'preview' -and $plan.label -ceq 'hgs-desktop') 'Unsupported registration plan.'
    Require-Registration ($plan.user_sid -ceq $sid -and $plan.task_name -ceq ('hgs-desktop-'+$sid)) 'Registration does not select the actual current user.'
    Require-Registration ($plan.configuration -cmatch '^[0-9a-f]{64}$' -and $plan.definition -is [string] -and $plan.definition.Length -le 65536) 'Invalid registration definition.'
    Require-Registration ($plan.argv.Count -eq 7 -and $plan.argv[1] -ceq 'desktop' -and $plan.argv[2] -ceq 'serve' -and $plan.argv[3] -ceq '--config-dir' -and $plan.argv[5] -ceq '--state-dir') 'Only the passive desktop command may be registered.'
    Require-Registration ($plan.session.kind -ceq 'interactive_user' -and $plan.session.allow_session_zero -eq $false) 'Registration requires an interactive user session.'
    $folder=$scheduler.GetFolder('\')
    $task=Get-DesktopTask $folder $plan.task_name
    if ($null -ne $task) { Assert-DesktopTask $task $plan $sid }
    if ($null -eq $task -and $request.operation -cin @('check','install')) {
        Require-Registration ($null -eq (Get-DesktopTask $folder 'hgs-desktop')) 'A legacy global desktop task exists; inspect it before registering a per-user task.'
    }
    $changed=$false
    switch -CaseSensitive ($request.operation) {
        'install' {
            if ($null -eq $task) {
                $sddl='O:'+ $sid +'D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;'+$sid+')'
                # TASK_CREATE | TASK_IGNORE_REGISTRATION_TRIGGERS: never
                # replace a task which appeared after the ownership check.
                [void]$folder.RegisterTask($plan.task_name,$plan.definition,34,$sid,$null,3,$sddl)
                $task=Get-DesktopTask $folder $plan.task_name
                Require-Registration ($null -ne $task) 'Created task could not be read back; inspect registration status before retrying.'
                Assert-DesktopTask $task $plan $sid
                $changed=$true
            }
        }
        'remove' {
            if ($null -ne $task) {
                # Delete the registration only. Never call Run, Stop, or End:
                # existing service/native processes are outside this command.
                [void]$folder.DeleteTask($plan.task_name,0)
                Require-Registration ($null -eq (Get-DesktopTask $folder $plan.task_name)) 'Removed task is still present; inspect status before retrying.'
                $task=$null;$changed=$true
            }
        }
    }
    $taskState=if($null -eq $task){$null}else{[int]$task.State}
    return [ordered]@{schema=1;operation=$request.operation;task_name=$plan.task_name;configuration=$plan.configuration;registered=($null -ne $task);task_state=$taskState;changed=$changed}
}

function Invoke-RegistrationMain {
    $ErrorActionPreference='Stop'
    $ProgressPreference='SilentlyContinue'
    [Console]::InputEncoding=[Text.UTF8Encoding]::new($false,$true)
    [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false,$true)
    $mutex=$null;$held=$false
    try {
        $inputText=[Text.StringBuilder]::new()
        while (($next=[Console]::In.Read()) -ge 0) {
            Require-Registration ($inputText.Length -lt 65536) 'Registration input exceeds its bound.'
            [void]$inputText.Append([char]$next)
        }
        $request=$inputText.ToString()|ConvertFrom-Json
        $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $mutex=[Threading.Mutex]::new($false,('Global\hgs-desktop-registration-'+$sid))
        $held=$mutex.WaitOne(2000)
        Require-Registration $held 'Another desktop registration operation is in progress.'
        $scheduler=New-Object -ComObject 'Schedule.Service'
        [void]$scheduler.Connect()
        $result=Invoke-RegistrationOperation $request $scheduler $sid
        [Console]::WriteLine(($result|ConvertTo-Json -Compress -Depth 5))
    } catch {
        [Console]::WriteLine((@{schema=1;error=$_.Exception.Message}|ConvertTo-Json -Compress))
        exit 1
    } finally {
        if ($held) { $mutex.ReleaseMutex() }
        if ($null -ne $mutex) { $mutex.Dispose() }
    }
}
