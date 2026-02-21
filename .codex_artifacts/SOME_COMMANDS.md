## Tried and true commands:
### Open game through thruport
$ErrorActionPreference='Stop';
$cwd='c:\Users\lukep\source\repos\Prototype Game Engine'; $port=46001;
$oldThr=$env:PROTOGE_THRUPORT; $oldPort=$env:PROTOGE_THRUPORT_PORT; $oldTel=$env:PROTOGE_THRUPORT_TELEMETRY;
$env:PROTOGE_THRUPORT='1'; $env:PROTOGE_THRUPORT_PORT="$port"; $env:PROTOGE_THRUPORT_TELEMETRY='1';
$proc=Start-Process -FilePath cargo -ArgumentList 'run -p game' -WorkingDirectory $cwd -PassThru -WindowStyle Hidden;
Start-Sleep -Seconds 6;

$client=New-Object System.Net.Sockets.TcpClient; $client.Connect('127.0.0.1',$port);
$stream=$client.GetStream(); $writer=New-Object System.IO.StreamWriter($stream); $writer.AutoFlush=$true; $reader=New-Object System.IO.StreamReader($stream);
function DrainAll(){ $lines=@(); while($stream.DataAvailable){ $lines += $reader.ReadLine() }; return $lines }
function Send([string]$cmd,[int]$wait=600){ $writer.WriteLine($cmd); Start-Sleep -Milliseconds $wait; return (DrainAll) }

# setup controllable player
$null=Send 'pause_sim' 400
$null=Send 'reset_scene' 500
$null=Send 'tick 1' 500
$null=Send 'spawn proto.player 0 0' 500
$null=Send 'tick 2' 500

# telemetry exactness + sync
$null=Send 'pause_sim' 400
$null=DrainAll
$writer.WriteLine('tick 10')
$all=@();
for($i=0;$i -lt 12;$i++){ Start-Sleep -Milliseconds 300; $all += DrainAll }
$frames=$all | Where-Object { $_ -like 'thruport.frame v1 *' }
$syncOut = Send 'sync' 400

# disconnect-reset check
$preDump = Send 'dump.state' 500 | Where-Object { $_ -like 'ok: dump.state v1 | *' } | Select-Object -Last 1
$null=Send 'input.key_down w' 300
$client.Close(); Start-Sleep -Milliseconds 400;

$client2=New-Object System.Net.Sockets.TcpClient; $client2.Connect('127.0.0.1',$port);
$stream2=$client2.GetStream(); $writer2=New-Object System.IO.StreamWriter($stream2); $writer2.AutoFlush=$true; $reader2=New-Object System.IO.StreamReader($stream2);
function Drain2(){ $lines=@(); while($stream2.DataAvailable){ $lines += $reader2.ReadLine() }; return $lines }
function Send2([string]$cmd,[int]$wait=600){ $writer2.WriteLine($cmd); Start-Sleep -Milliseconds $wait; return (Drain2) }
$null=Send2 'tick 1' 500
$postDump = Send2 'dump.state' 500 | Where-Object { $_ -like 'ok: dump.state v1 | *' } | Select-Object -Last 1
$null=Send2 'quit' 300
$client2.Close()
if(-not $proc.HasExited){ Stop-Process -Id $proc.Id -Force }
$env:PROTOGE_THRUPORT=$oldThr; $env:PROTOGE_THRUPORT_PORT=$oldPort; $env:PROTOGE_THRUPORT_TELEMETRY=$oldTel;

# summarize
"FRAME_LINES=$($frames.Count)"
if($frames.Count -gt 0){
  $ticks=@(); $qt=@();
  foreach($f in $frames){ if($f -match 'tick:(\d+)'){ $ticks += [int]$matches[1] }; if($f -match 'qtick:(\d+)'){ $qt += [int]$matches[1] } }
  $mono=$true; for($i=1;$i -lt $ticks.Count;$i++){ if($ticks[$i] -ne ($ticks[$i-1]+1)){ $mono=$false; break } }
  "FIRST_FRAME=$($frames[0])"
  "LAST_FRAME=$($frames[$frames.Count-1])"
  "TICK_MONOTONIC=$mono"
  "QTICK_SERIES=$([string]::Join(',', $qt))"
}
"SYNC_LINES=$([string]::Join(' || ', $syncOut))"
"PRE_DUMP=$preDump"
"POST_DUMP=$postDump"