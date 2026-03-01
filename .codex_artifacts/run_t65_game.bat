@echo off
cd /d "c:\Users\lukep\source\repos\Prototype Game Engine"
set PROTOGE_THRUPORT=1
set PROTOGE_THRUPORT_PORT=46003
set PROTOGE_THRUPORT_TELEMETRY=0
cargo run -p game 1> .codex_artifacts\t65_game_stdout.log 2> .codex_artifacts\t65_game_stderr.log
