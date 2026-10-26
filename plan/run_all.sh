#!/bin/zsh
cd /Users/todd/StudioProjects/macroscope
: > plan/logs/_status.txt
for t in plan/tasks/*.md; do ./plan/run_track.sh $t & sleep 2; done
wait
echo ALL_DONE >> plan/logs/_status.txt
