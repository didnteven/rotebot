#!/bin/zsh
# usage: run_track.sh <task-file>   (runs from the macroscope repo root)
cd /Users/todd/StudioProjects/macroscope
t=$1; name=$(basename $t .md); kind=${name##*.}; id=${name%.*}
log=plan/logs/$id.$kind.log
prompt="You are one of several planning agents working in parallel on a new desktop app. First read plan/BRIEF.md fully. Then do the track below. Write your output file yourself (markdown), make it substantive and specific (roughly 2,000-5,000 words, ASCII diagrams welcome), and do not edit any other agent's files.

$(cat $t)"
start=$(date +%s)
if [[ $kind == codex ]]; then
  codex --search exec -C . -s workspace-write --skip-git-repo-check -c model_reasoning_effort=high \
    -o plan/logs/$id.codex.last.md "$prompt" > $log 2>&1
else
  claude -p "$prompt" --model claude-opus-5-5 --effort high --permission-mode acceptEdits \
    --allowedTools "Read,Write,Edit,Glob,Grep,WebSearch,WebFetch,Bash,Skill" > $log 2>&1
fi
echo "$id $kind exit=$? secs=$(( $(date +%s)-start ))" >> plan/logs/_status.txt
