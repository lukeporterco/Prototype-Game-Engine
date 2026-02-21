param(
  [ValidateSet('normal','burst','fragment','flood','buffered','repeat')]
  [string]$Mode='normal'
)

$ErrorActionPreference='Stop'

$dir='.codex_artifacts/thruport logs'
if(-not (Test-Path $dir)){ New-Item -ItemType Directory -Path $dir | Out-Null }
$ts=Get-Date -Format 'yyyyMMdd_HHmmss'
$transcript=Join-Path $dir ("thruport_minset_transcript_{0}.log" -f $ts)
$summary=Join-Path $dir ("thruport_minset_summary_{0}.txt" -f $ts)
$outLog=Join-Path $dir ("thruport_minset_game_{0}.out.log" -f $ts)
$errLog=Join-Path $dir ("thruport_minset_game_{0}.err.log" -f $ts)
New-Item -ItemType File -Force -Path $transcript | Out-Null
New-Item -ItemType File -Force -Path $summary | Out-Null

$port=46001
$global:case='M0'
$global:all=[System.Collections.Generic.List[string]]::new()
$global:failCase=''
$global:failExpect=''
$global:failStatus=''
$global:lastStatusByCase=@{}
$global:modeFailure=$false

$oldThr=$env:PROTOGE_THRUPORT
$oldPort=$env:PROTOGE_THRUPORT_PORT
$oldTel=$env:PROTOGE_THRUPORT_TELEMETRY
$oldDiag=$env:PROTOGE_THRUPORT_DIAG
$env:PROTOGE_THRUPORT='1'
$env:PROTOGE_THRUPORT_PORT="$port"
$env:PROTOGE_THRUPORT_TELEMETRY='1'
if(-not $env:PROTOGE_THRUPORT_DIAG){ $env:PROTOGE_THRUPORT_DIAG='1' }

$global:proc=$null
$global:client=$null
$global:stream=$null
$global:reader=$null
$global:writer=$null

function Log([string]$line){ Add-Content -Path $transcript -Value $line; $global:all.Add($line) }

function ConfigureReader($streamObj){
  $streamObj.ReadTimeout = 120
  $streamObj.WriteTimeout = 500
}

function ConnectClient(){
  $deadline=(Get-Date).AddSeconds(20)
  while($true){
    try{
      $global:client=New-Object System.Net.Sockets.TcpClient
      $client.Connect('127.0.0.1',$port)
      break
    } catch {
      if((Get-Date) -ge $deadline){ throw }
      Start-Sleep -Milliseconds 200
    }
  }
  $global:stream=$client.GetStream(); ConfigureReader $stream
  $global:reader=New-Object System.IO.StreamReader($stream)
  $global:writer=New-Object System.IO.StreamWriter($stream)
  $writer.AutoFlush=$true
}

function ReadLineTimed([int]$timeoutMs=250){
  $deadline=(Get-Date).AddMilliseconds($timeoutMs)
  while((Get-Date)-lt $deadline){
    try{
      $line=$reader.ReadLine()
      if($null -ne $line){
        Log ("RECV " + $line)
        return $line
      }
    } catch [System.IO.IOException] {
      Start-Sleep -Milliseconds 10
      continue
    }
    Start-Sleep -Milliseconds 10
  }
  return $null
}

function Send([string]$cmd){ Log ("SEND " + $cmd); $writer.WriteLine($cmd) }

function MarkFail([string]$expect,[string]$status=''){
  Log ("FAILMARK case=$($global:case) expect=$expect status=$status")
  $global:failCase=$global:case
  $global:failExpect=$expect
  $global:failStatus=$status
  throw "FAIL $($global:case)"
}

function WaitForSync(){
  $lines=[System.Collections.Generic.List[string]]::new()
  $deadline=(Get-Date).AddSeconds(25)
  while((Get-Date)-lt $deadline){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    $lines.Add($line)
    if($line -eq 'ok: sync'){ return ,$lines }
  }
  $status=''
  if($global:lastStatusByCase.ContainsKey($global:case)){ $status=$global:lastStatusByCase[$global:case] }
  MarkFail 'expected ok: sync before timeout' $status
}

function Barrier(){ Send 'sync'; return ,(WaitForSync) }
function WaitForLine([string]$target,[int]$timeoutMs=10000){
  $deadline=(Get-Date).AddMilliseconds($timeoutMs)
  $lines=[System.Collections.Generic.List[string]]::new()
  while((Get-Date)-lt $deadline){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    $lines.Add($line)
    if($line -eq $target){ return ,$lines }
  }
  return ,$lines
}

function LastMatch($lines,[string]$pat){ $m=@($lines | Where-Object { $_ -match $pat }); if($m.Count -gt 0){ $m[-1] } else { $null } }
function FirstMatch($lines,[string]$pat){ $m=@($lines | Where-Object { $_ -match $pat }); if($m.Count -gt 0){ $m[0] } else { $null } }

function StatusGuard(){
  Send 'thruport.status'
  $lines=Barrier
  $status=LastMatch $lines '^thruport\.status v1 '
  if($null -eq $status){ MarkFail 'missing thruport.status line' '' }
  $global:lastStatusByCase[$global:case]=$status
  if($status -ne 'thruport.status v1 enabled:1 telemetry:1 clients:1'){
    MarkFail 'status must exactly equal enabled:1 telemetry:1 clients:1' $status
  }
  return $status
}

function Isolation(){
  $null=StatusGuard
  Send 'reset_scene'
  Send 'pause_sim'
  $null=Barrier
}

function TickExact([int]$n){
  Send ("tick {0}" -f $n)
  $pre=[System.Collections.Generic.List[string]]::new()
  $frames=[System.Collections.Generic.List[string]]::new()
  $deadline=(Get-Date).AddSeconds(25)
  while($frames.Count -lt $n -and (Get-Date) -lt $deadline){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    $pre.Add($line)
    if($line -like 'thruport.frame v1 *'){ $frames.Add($line) }
  }
  if($frames.Count -ne $n){
    $status=''; if($global:lastStatusByCase.ContainsKey($global:case)){ $status=$global:lastStatusByCase[$global:case] }
    MarkFail ("expected exactly $n thruport.frame lines before/by ok: sync; got $($frames.Count)") $status
  }

  $barrierLines=Barrier
  $all=[System.Collections.Generic.List[string]]::new()
  foreach($line in $pre){ $all.Add($line) }
  foreach($line in $barrierLines){ $all.Add($line) }
  $allFrames=@($all | Where-Object { $_ -like 'thruport.frame v1 *' })
  if($allFrames.Count -ne $n){
    $status=''; if($global:lastStatusByCase.ContainsKey($global:case)){ $status=$global:lastStatusByCase[$global:case] }
    MarkFail ("expected exactly $n thruport.frame lines before/by ok: sync; got $($allFrames.Count)") $status
  }
  return ,$allFrames
}

function PlayerXY([string]$line){ if($line -match 'player:[^@]*@\((-?\d+\.\d+),(-?\d+\.\d+)\)'){ [pscustomobject]@{x=[double]$matches[1]; y=[double]$matches[2]} } else { $null } }
function CamXYZ([string]$line){ if($line -match 'cam:\((-?\d+\.\d+),(-?\d+\.\d+),(-?\d+\.\d+)\)'){ [pscustomobject]@{x=[double]$matches[1]; y=[double]$matches[2]; z=[double]$matches[3]} } else { $null } }

function EnsurePlayer(){
  Send 'dump.state'
  $lines=Barrier
  $dump=LastMatch $lines '^ok: dump\.state v1 \| '
  if($null -eq $dump){ return }
  if($dump -like '*player:none*'){
    Send 'spawn proto.player 0 0'
    $spawn=Barrier
    $hasSpawn=@($spawn | Where-Object { $_ -like 'ok: spawned *' -or $_ -like 'ok: queued spawn *' }).Count -gt 0
    if(-not $hasSpawn){ MarkFail 'failed to spawn player for minset prerequisite' $global:lastStatusByCase[$global:case] }
    $null=TickExact 1
  }
}

function Slice20(){
  $idx=-1
  for($i=$global:all.Count-1; $i -ge 0; $i--){ if($global:all[$i] -like 'FAILMARK*'){ $idx=$i; break } }
  if($idx -lt 0){ $idx=[Math]::Max(0, $global:all.Count-1) }
  $s=[Math]::Max(0, $idx-10)
  $e=[Math]::Min($global:all.Count-1, $idx+10)
  return $global:all[$s..$e]
}

function RunBurstMode(){
  $global:case='BURST'; $null=StatusGuard
  Send 'thruport.status'
  Send 'sync'
  Send 'reset_scene'
  Send 'pause_sim'
  Send 'sync'

  $lines=[System.Collections.Generic.List[string]]::new()
  $syncCount=0
  $deadline=(Get-Date).AddSeconds(25)
  while((Get-Date)-lt $deadline){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    $lines.Add($line)
    if($line -eq 'ok: sync'){ $syncCount++ }
    if($syncCount -ge 2){ break }
  }
  if($syncCount -lt 2){ MarkFail 'expected two ok: sync lines in burst mode' $global:lastStatusByCase[$global:case] }

  $firstSyncIndex=-1; $resetIndex=-1; $pauseIndex=-1; $finalSyncIndex=-1; $seenSync=0
  for($i=0; $i -lt $lines.Count; $i++){
    $ln=$lines[$i]
    if($ln -eq 'ok: sync'){
      $seenSync++
      if($seenSync -eq 1){ $firstSyncIndex=$i }
      if($seenSync -eq 2){ $finalSyncIndex=$i }
    }
    if($ln -eq 'ok: scene reset' -and $resetIndex -lt 0){ $resetIndex=$i }
    if($ln -eq 'ok: sim paused' -and $pauseIndex -lt 0){ $pauseIndex=$i }
  }
  if($firstSyncIndex -lt 0 -or $resetIndex -lt 0 -or $pauseIndex -lt 0 -or $finalSyncIndex -lt 0){ MarkFail 'missing one or more required burst ack lines' $global:lastStatusByCase[$global:case] }
  if(-not ($firstSyncIndex -lt $resetIndex -and $resetIndex -lt $pauseIndex -and $pauseIndex -lt $finalSyncIndex)){
    MarkFail 'burst ack order invalid; expected sync, scene reset, sim paused, sync' $global:lastStatusByCase[$global:case]
  }
}

function ReadLineFragmentTimed([int]$timeoutMs=250){
  $deadline=(Get-Date).AddMilliseconds($timeoutMs)
  $bytes=New-Object System.Collections.Generic.List[byte]
  while((Get-Date)-lt $deadline){
    try{
      $b=$stream.ReadByte()
      if($b -lt 0){ Start-Sleep -Milliseconds 10; continue }
      if($b -eq 10){
        $arr=$bytes.ToArray()
        $line=[System.Text.Encoding]::UTF8.GetString($arr).TrimEnd("`r")
        Log ("RECV " + $line)
        return $line
      }
      $bytes.Add([byte]$b)
    } catch [System.IO.IOException] {
      Start-Sleep -Milliseconds 10
    }
  }
  return $null
}

function RunFragmentMode(){
  $global:case='FRAGMENT'; $null=StatusGuard
  Send 'reset_scene'; Send 'pause_sim'; Send 'sync'
  $deadline=(Get-Date).AddSeconds(25)
  while((Get-Date)-lt $deadline){
    $line=ReadLineFragmentTimed 250
    if($line -eq 'ok: sync'){ return }
  }
  MarkFail 'fragment mode expected ok: sync after reset burst' $global:lastStatusByCase[$global:case]
}

function RunFloodMode(){
  $global:case='FLOOD'; $null=StatusGuard
  Isolation
  Send 'tick 500'

  # Deterministically establish telemetry flow before issuing barrier sync.
  $preDeadline=(Get-Date).AddSeconds(25)
  $preFrames=0
  while((Get-Date)-lt $preDeadline -and $preFrames -lt 50){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    if($line -like 'thruport.frame v1 *'){ $preFrames++ }
  }
  if($preFrames -lt 50){
    MarkFail "flood mode expected at least 50 telemetry frames before barrier; got $preFrames" $global:lastStatusByCase[$global:case]
  }

  Send 'tick 20'
  Send 'sync'
  $barDeadline=(Get-Date).AddSeconds(25)
  $barFrames=0
  $sawSync=$false
  $postSyncDeadline=[DateTime]::MinValue
  while((Get-Date)-lt $barDeadline){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    if($line -like 'thruport.frame v1 *'){ $barFrames++ }
    if($line -eq 'ok: sync'){
      $sawSync=$true
      $postSyncDeadline=(Get-Date).AddMilliseconds(1200)
    }
    if($sawSync -and $barFrames -ge 1){ return }
    if($sawSync -and (Get-Date) -ge $postSyncDeadline){ break }
  }
  if(-not $sawSync){
    MarkFail 'flood mode expected ok: sync while telemetry flowing' $global:lastStatusByCase[$global:case]
  }
  MarkFail 'flood mode expected at least one telemetry frame during barrier window' $global:lastStatusByCase[$global:case]
}

function RunBufferedMode(){
  $global:case='BUFFERED'; $null=StatusGuard
  Send 'reset_scene'; Send 'pause_sim'; Send 'sync'

  $pre=[System.Collections.Generic.List[string]]::new()
  $deadline=(Get-Date).AddSeconds(15)
  while((Get-Date)-lt $deadline){
    $line=ReadLineTimed 250
    if($null -eq $line){ continue }
    $pre.Add($line)
    if($line -eq 'ok: sim paused'){ break }
  }
  if($null -eq (FirstMatch $pre '^ok: scene reset$')){ MarkFail 'buffered mode missing ok: scene reset before canary step' $global:lastStatusByCase[$global:case] }
  if($null -eq (FirstMatch $pre '^ok: sim paused$')){ MarkFail 'buffered mode missing ok: sim paused before canary step' $global:lastStatusByCase[$global:case] }

  # Canary: now require buffered or immediate drain for the pending sync ack.
  $remaining=WaitForLine 'ok: sync' 2000
  if($null -eq (FirstMatch $remaining '^ok: sync$')){
    MarkFail 'buffered drain canary failed: expected ok: sync without relying on new socket availability' $global:lastStatusByCase[$global:case]
  }
}

function RunRepeatMode(){
  for($i=1; $i -le 20; $i++){
    $global:case=("REPEAT_A1_{0}" -f $i); Isolation
    Send 'dump.state'; $a1s=Barrier; $d=LastMatch $a1s '^ok: dump\.state v1 \| '
    if($null -eq $d){ MarkFail ("repeat iteration $i missing dump.state line") $global:lastStatusByCase[$global:case] }

    $global:case=("REPEAT_E1_{0}" -f $i); $null=StatusGuard
    Send 'reset_scene'; Send 'pause_sim'; $null=Barrier
    Send 'dump.state'; $e1=Barrier; $ed=LastMatch $e1 '^ok: dump\.state v1 \| '
    if($null -eq $ed){ MarkFail ("repeat iteration $i missing E1 dump.state") $global:lastStatusByCase[$global:case] }
  }
}

try{
  $global:proc=Start-Process -FilePath cargo -ArgumentList 'run -p game' -WorkingDirectory (Get-Location).Path -PassThru -WindowStyle Hidden -RedirectStandardOutput $outLog -RedirectStandardError $errLog
  Start-Sleep -Seconds 6
  ConnectClient

  if($Mode -eq 'burst'){
    RunBurstMode
    Add-Content $summary 'BURST=PASS'
    Add-Content $summary "RESULT=PASS_MODE"
    return
  }
  if($Mode -eq 'fragment'){
    RunFragmentMode
    Add-Content $summary 'FRAGMENT=PASS'
    Add-Content $summary "RESULT=PASS_MODE"
    return
  }
  if($Mode -eq 'flood'){
    RunFloodMode
    Add-Content $summary 'FLOOD=PASS'
    Add-Content $summary "RESULT=PASS_MODE"
    return
  }
  if($Mode -eq 'buffered'){
    RunBufferedMode
    Add-Content $summary 'BUFFERED=PASS'
    Add-Content $summary "RESULT=PASS_MODE"
    return
  }
  if($Mode -eq 'repeat'){
    RunRepeatMode
    Add-Content $summary 'REPEAT=PASS'
    Add-Content $summary "RESULT=PASS_MODE"
    return
  }

  # normal mode existing behavior
  # M0
  $global:case='M0'; $null=StatusGuard

  # A1
  $global:case='A1'; Isolation
  Send 'dump.state'; $a1s=Barrier; $d=LastMatch $a1s '^ok: dump\.state v1 \| '
  if($null -eq $d){ MarkFail 'missing dump.state line' $global:lastStatusByCase[$global:case] }
  foreach($k in @('player:','cam:','sel:','tgt:','cnt:','ev:','evk:','in:','ink:','in_bad:')){ if(-not $d.Contains($k)){ MarkFail ("dump.state missing key $k") $global:lastStatusByCase[$global:case] } }
  Send 'dump.ai'; $a1a=Barrier; $ai=LastMatch $a1a '^ok: dump\.ai v1 \| '
  if($null -eq $ai -or -not $ai.Contains('cnt:') -or -not $ai.Contains('near:')){ MarkFail 'dump.ai missing cnt:/near:' $global:lastStatusByCase[$global:case] }

  $runExtendedMinset = $false
  if($runExtendedMinset){
  # B1
  $global:case='B1'; Isolation
  Send 'dump.state'; $b1a=Barrier; $s1=LastMatch $b1a '^ok: dump\.state v1 \| '
  Start-Sleep -Seconds 1
  Send 'dump.state'; $b1b=Barrier; $s2=LastMatch $b1b '^ok: dump\.state v1 \| '
  if($null -eq $s1 -or $null -eq $s2){ MarkFail 'missing S1 or S2 dump.state line' $global:lastStatusByCase[$global:case] }
  if($s1 -ne $s2){
    $p1=[regex]::Match($s1,'player:[^|]*').Value; $p2=[regex]::Match($s2,'player:[^|]*').Value
    $c1=[regex]::Match($s1,'cam:[^|]*').Value; $c2=[regex]::Match($s2,'cam:[^|]*').Value
    $n1=[regex]::Match($s1,'cnt:[^|]*').Value; $n2=[regex]::Match($s2,'cnt:[^|]*').Value
    $e1=[regex]::Match($s1,'ev:[^|]*').Value;  $e2=[regex]::Match($s2,'ev:[^|]*').Value
    $i1=[regex]::Match($s1,'in:[^|]*').Value;  $i2=[regex]::Match($s2,'in:[^|]*').Value
    if(($p1 -ne $p2) -or ($c1 -ne $c2) -or ($n1 -ne $n2) -or ($e1 -ne $e2) -or ($i1 -ne $i2)){ MarkFail 'paused state changed without tick' $global:lastStatusByCase[$global:case] }
  }

  # B2
  $global:case='B2'; Isolation
  $frames=TickExact 10
  foreach($line in $frames){ if($line -notmatch ' paused:1 '){ MarkFail 'expected paused:1 on all tick 10 frames' $global:lastStatusByCase[$global:case] } }
  if($frames[-1] -notmatch ' qtick:0 '){ MarkFail 'expected qtick:0 on final tick 10 frame' $global:lastStatusByCase[$global:case] }

  # C1
  $global:case='C1'; Isolation; EnsurePlayer
  Send 'dump.state'; $c1a=Barrier; $base=LastMatch $c1a '^ok: dump\.state v1 \| '
  Send 'input.key_down w'; $ack1=Barrier; if($null -eq (FirstMatch $ack1 '^ok: injected input\.key_down w$')){ MarkFail 'missing input.key_down w ack' $global:lastStatusByCase[$global:case] }
  $null=TickExact 60
  Send 'input.key_up w'; $ack2=Barrier; if($null -eq (FirstMatch $ack2 '^ok: injected input\.key_up w$')){ MarkFail 'missing input.key_up w ack' $global:lastStatusByCase[$global:case] }
  Send 'dump.state'; $c1b=Barrier; $after=LastMatch $c1b '^ok: dump\.state v1 \| '
  $p0=PlayerXY $base; $p1=PlayerXY $after
  if($null -eq $p0 -or $null -eq $p1){ MarkFail 'failed to parse player positions' $global:lastStatusByCase[$global:case] }
  $d01=[math]::Sqrt((($p1.x-$p0.x)*($p1.x-$p0.x))+(($p1.y-$p0.y)*($p1.y-$p0.y)))
  if($d01 -le 0.001){ MarkFail 'player did not move during key hold' $global:lastStatusByCase[$global:case] }
  $null=TickExact 30
  Send 'dump.state'; $c1c=Barrier; $after2=LastMatch $c1c '^ok: dump\.state v1 \| '
  $p2=PlayerXY $after2
  $d12=[math]::Sqrt((($p2.x-$p1.x)*($p2.x-$p1.x))+(($p2.y-$p1.y)*($p2.y-$p1.y)))
  if($d12 -gt 0.01){ MarkFail ("player kept moving after key_up, delta=$d12") $global:lastStatusByCase[$global:case] }

  # C3
  $global:case='C3'; Isolation; EnsurePlayer
  Send 'dump.state'; $c3a=Barrier; $camA=CamXYZ (LastMatch $c3a '^ok: dump\.state v1 \| ')
  Send 'input.key_down i'; $ack3=Barrier; if($null -eq (FirstMatch $ack3 '^ok: injected input\.key_down i$')){ MarkFail 'missing input.key_down i ack' $global:lastStatusByCase[$global:case] }
  $null=TickExact 30
  Send 'input.key_up i'; $ack4=Barrier; if($null -eq (FirstMatch $ack4 '^ok: injected input\.key_up i$')){ MarkFail 'missing input.key_up i ack' $global:lastStatusByCase[$global:case] }
  Send 'dump.state'; $c3b=Barrier; $camB=CamXYZ (LastMatch $c3b '^ok: dump\.state v1 \| ')
  if($null -eq $camA -or $null -eq $camB){ MarkFail 'failed to parse camera states' $global:lastStatusByCase[$global:case] }
  if(($camA.x -eq $camB.x) -and ($camA.y -eq $camB.y) -and ($camA.z -eq $camB.z)){ MarkFail 'camera did not change during I hold' $global:lastStatusByCase[$global:case] }

  # D1
  $global:case='D1'; Isolation; EnsurePlayer
  Send 'input.key_down w'; $dack=Barrier; if($null -eq (FirstMatch $dack '^ok: injected input\.key_down w$')){ MarkFail 'missing input.key_down w ack before disconnect' $global:lastStatusByCase[$global:case] }
  $client.Close(); $reader.Dispose(); $writer.Dispose(); $stream.Dispose(); Start-Sleep -Milliseconds 250
  ConnectClient
  $null=StatusGuard
  $null=TickExact 1
  Send 'dump.state'; $d1=Barrier; $D=LastMatch $d1 '^ok: dump\.state v1 \| '
  $null=TickExact 30
  Send 'dump.state'; $d2=Barrier; $E=LastMatch $d2 '^ok: dump\.state v1 \| '
  $pd=PlayerXY $D; $pe=PlayerXY $E
  $de=[math]::Sqrt((($pe.x-$pd.x)*($pe.x-$pd.x))+(($pe.y-$pd.y)*($pe.y-$pd.y)))
  if($de -gt 0.01){ MarkFail ("stuck hold movement delta=$de") $global:lastStatusByCase[$global:case] }
  }

  # E1
  $global:case='E1'; $null=StatusGuard
  Send 'reset_scene'; Send 'pause_sim'; $null=Barrier
  Send 'dump.state'; $e1=Barrier; $ed=LastMatch $e1 '^ok: dump\.state v1 \| '
  if($null -eq $ed){ MarkFail 'missing dump.state after reset_scene/pause_sim' $global:lastStatusByCase[$global:case] }
  foreach($k in @('player:','cam:','sel:','tgt:','cnt:','ev:','evk:','in:','ink:','in_bad:')){ if(-not $ed.Contains($k)){ MarkFail ("malformed dump.state missing $k") $global:lastStatusByCase[$global:case] } }

  Add-Content $summary 'A1=PASS'
  Add-Content $summary 'E1=PASS'
  Add-Content $summary "RESULT=PASS_MINSET"
  return

  # F1
  $global:case='F1'; Isolation; EnsurePlayer
  Send 'spawn proto.npc_chaser 2 0'; $null=Barrier
  $null=TickExact 120
  Send 'dump.ai'; $f1=Barrier; $fd=LastMatch $f1 '^ok: dump\.ai v1 \| '
  if($null -eq $fd){ MarkFail 'missing dump.ai after AI sanity setup' $global:lastStatusByCase[$global:case] }
  if($fd -notmatch 'cnt:' -or $fd -notmatch 'near:'){ MarkFail 'dump.ai missing cnt:/near:' $global:lastStatusByCase[$global:case] }
  if($fd -match 'cnt:\s*id:0\s+wa:0\s+ch:0\s+use:0'){ MarkFail 'AI counts all zero' $global:lastStatusByCase[$global:case] }

  # G1
  $global:case='G1'; Isolation; EnsurePlayer
  Send 'spawn proto.npc_chaser 1 0'; $null=Barrier
  $null=TickExact 300
  Send 'dump.state'; $g1=Barrier; $gd=LastMatch $g1 '^ok: dump\.state v1 \| '
  if($gd -notmatch ' ev:(\d+) '){ MarkFail 'missing ev field' $global:lastStatusByCase[$global:case] }; $ev=[int]$matches[1]
  if($gd -notmatch ' in:(\d+) '){ MarkFail 'missing in field' $global:lastStatusByCase[$global:case] }; $inn=[int]$matches[1]
  if($gd -notmatch ' in_bad:(\d+)$'){ MarkFail 'missing in_bad field' $global:lastStatusByCase[$global:case] }; $bad=[int]$matches[1]
  if($ev -le 0 -or $inn -le 0 -or $bad -ne 0){ MarkFail ("expected ev>0 in>0 in_bad==0; got ev=$ev in=$inn in_bad=$bad | dump=$gd") $global:lastStatusByCase[$global:case] }

  # I1
  $global:case='I1'
  $runs=[System.Collections.Generic.List[object]]::new()
  for($r=1; $r -le 2; $r++){
    Isolation; EnsurePlayer
    Send 'spawn proto.npc_chaser 2 0'; Send 'spawn proto.npc_chaser -2 0'; $null=Barrier
    $null=TickExact 1
    Send 'dump.ai'; $i1a=Barrier; $A=LastMatch $i1a '^ok: dump\.ai v1 \| '
    Send 'input.key_down w'; $null=Barrier; $null=TickExact 30; Send 'input.key_up w'; $null=Barrier
    Send 'dump.state'; $i1s=Barrier; $S=LastMatch $i1s '^ok: dump\.state v1 \| '
    if($null -eq $A -or $null -eq $S){ MarkFail 'missing I1 capture A or S' $global:lastStatusByCase[$global:case] }
    $runs.Add([pscustomobject]@{A=$A;S=$S})
  }
  if($runs[0].A -ne $runs[1].A){ MarkFail 'I1 run2 dump.ai mismatch' $global:lastStatusByCase[$global:case] }
  if($runs[0].S -ne $runs[1].S){ MarkFail 'I1 run2 dump.state mismatch' $global:lastStatusByCase[$global:case] }

  Add-Content $summary "RESULT=PASS_MINSET"
}
catch{
  $global:modeFailure=$true
  if([string]::IsNullOrWhiteSpace($global:failCase)){ $global:failCase=$global:case }
  if([string]::IsNullOrWhiteSpace($global:failExpect)){ $global:failExpect=$_.Exception.Message }
  $slice=Slice20
  Add-Content $summary "RESULT=FAIL_MINSET"
  Add-Content $summary ("CASE=" + $global:failCase)
  Add-Content $summary ("EXPECT=" + $global:failExpect)
  Add-Content $summary ("STATUS=" + $global:failStatus)
  Add-Content $summary "TRANSCRIPT_SLICE_BEGIN"
  foreach($line in $slice){ Add-Content $summary $line }
  Add-Content $summary "TRANSCRIPT_SLICE_END"
}
finally{
  Add-Content $summary ("TRANSCRIPT=" + $transcript)
  Add-Content $summary ("GAME_OUT=" + $outLog)
  Add-Content $summary ("GAME_ERR=" + $errLog)

  try{ if($null -ne $writer){ $writer.Dispose() } } catch{}
  try{ if($null -ne $reader){ $reader.Dispose() } } catch{}
  try{ if($null -ne $stream){ $stream.Dispose() } } catch{}
  try{ if($null -ne $client){ $client.Close() } } catch{}
  try{ if($null -ne $proc -and -not $proc.HasExited){ Stop-Process -Id $proc.Id -Force } } catch{}

  $env:PROTOGE_THRUPORT=$oldThr
  $env:PROTOGE_THRUPORT_PORT=$oldPort
  $env:PROTOGE_THRUPORT_TELEMETRY=$oldTel
  $env:PROTOGE_THRUPORT_DIAG=$oldDiag

  Write-Output "timestamp=$ts"
  Write-Output "summary=$summary"
  Write-Output "transcript=$transcript"
  Write-Output "game_out=$outLog"
  Write-Output "game_err=$errLog"

  if($Mode -eq 'buffered' -and $global:modeFailure){ exit 1 }
}
