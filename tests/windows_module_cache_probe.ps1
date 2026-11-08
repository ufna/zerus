# PRIVATE diagnostic only. No cmdlet runs before the measured variant.
$ErrorActionPreference='Stop';$script:records=0
function w([string]$key,[string]$value){
if(++$script:records -gt 32){throw 'record limit'}
$b=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($value))
[IO.File]::AppendAllText($env:MODULE_PROBE_OUT,$key+'|'+[Diagnostics.Stopwatch]::GetTimestamp()+'|'+$b+"`n")
}
function census {
$clock=[Diagnostics.Stopwatch]::StartNew();$roots=@{};$dirs=0;$modules=0;$units=0;$visited=0;$errors=0;$complete=$true
foreach($root in ($env:PSModulePath -split [IO.Path]::PathSeparator)){
if([string]::IsNullOrEmpty($root)-or$roots.ContainsKey($root)){continue};$roots[$root]=$true
try{
if(-not[IO.Directory]::Exists($root)){continue}
foreach($dir in [IO.Directory]::EnumerateDirectories($root)){
if(++$visited -gt 8192 -or $clock.ElapsedMilliseconds -gt 2000){$complete=$false;break}
$dirs++;$name=[IO.Path]::GetFileName($dir);$found=$false
foreach($ext in @('.psd1','.psm1','.dll')){if([IO.File]::Exists([IO.Path]::Combine($dir,$name+$ext))){$units++;$found=$true;break}}
foreach($version in [IO.Directory]::EnumerateDirectories($dir)){
if(++$visited -gt 8192 -or $clock.ElapsedMilliseconds -gt 2000){$complete=$false;break}
if([IO.Path]::GetFileName($version) -notmatch '^\d+\.\d+'){continue}
foreach($ext in @('.psd1','.psm1','.dll')){if([IO.File]::Exists([IO.Path]::Combine($version,$name+$ext))){$units++;$found=$true;break}}
}
if($found){$modules++};if(-not$complete){break}
}
}catch{$errors++;$complete=$false}
if(-not$complete -and ($visited -gt 8192 -or $clock.ElapsedMilliseconds -gt 2000)){break}
}
w 'module-census' ([string]::Join('|',[string[]]@($roots.Count,$dirs,$modules,$units,$complete,$errors,$visited,$clock.ElapsedMilliseconds)))
}
$failed=$false
try{
w 'entry' ([Diagnostics.Stopwatch]::Frequency)
w 'edition' $PSVersionTable.PSEdition;w 'version' $PSVersionTable.PSVersion.ToString()
w 'module-path-count' (($env:PSModulePath -split [IO.Path]::PathSeparator).Count)
$raw=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__PAYLOAD__'))
$mode=$env:MODULE_PROBE_MODE
if($mode -notin @('baseline','import','no-autoload','dotnet')){throw 'unknown fixed variant'}
if($mode -eq 'no-autoload'){$PSModuleAutoLoadingPreference='None'}
if($mode -eq 'import' -or $mode -eq 'no-autoload'){
w 'import-before' '';Import-Module Microsoft.PowerShell.Utility -ErrorAction Stop;w 'import-after' ''
}
if($mode -eq 'dotnet'){
w 'load-before' ''
[void][Reflection.Assembly]::Load('System.Web.Extensions, Version=4.0.0.0, Culture=neutral, PublicKeyToken=31bf3856ad364e35')
$parser=[System.Web.Script.Serialization.JavaScriptSerializer]::new();w 'load-after' ''
}
w 'parse-before' ''
if($mode -eq 'dotnet'){$value=$parser.DeserializeObject($raw)}else{$value=ConvertFrom-Json $raw}
w 'parse-after' ''
$expected=@(__EXPECTED__)
if($value.Count -ne $expected.Count){throw 'argument count mismatch'}
for($i=0;$i -lt $expected.Count;$i++){
if($value[$i] -isnot [string] -or -not[string]::Equals($value[$i],$expected[$i],[StringComparison]::Ordinal)){throw 'argument mismatch'}
}
w 'verified' $value.Count
}catch{w 'error-type' $_.Exception.GetType().FullName;$failed=$true}
try{
census
if($env:MODULE_CACHE_PROOF -eq '1'){
# The documented cache writer is asynchronous. Observe this one private file,
# never delay the timed first call or infer a cache hit from latency alone.
$wait=[Diagnostics.Stopwatch]::StartNew()
while(((-not[IO.File]::Exists($env:PSModuleAnalysisCachePath))-or([IO.FileInfo]::new($env:PSModuleAnalysisCachePath).Length -eq 0))-and$wait.ElapsedMilliseconds -lt 10000){[Threading.Thread]::Sleep(20)}
w 'cache-observed' ([IO.File]::Exists($env:PSModuleAnalysisCachePath));w 'cache-observation-ms' $wait.ElapsedMilliseconds
}
}catch{w 'observation-error-type' $_.Exception.GetType().FullName;$failed=$true}
w 'done' (-not$failed)
if($failed){exit 1};exit 0
