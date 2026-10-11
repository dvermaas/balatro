@echo off
rem Launch Balatro with BalatroBot fast mode (10x game speed, uncapped FPS), with shaders and audio on.
rem Steam must be running. Close any running Balatro first.
set BALATROBOT_FAST=1
set BALATROBOT_AUDIO=1
set GAME=C:\Program Files (x86)\Steam\steamapps\common\Balatro
start "" /D "%GAME%" "%GAME%\Balatro.exe"
