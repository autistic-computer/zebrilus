#!/usr/bin/env bash
set -euo pipefail

cd /home/runner/actions-runner
./config.sh --unattended --ephemeral --replace --disableupdate \
    --url "$RUNNER_URL" --token "$RUNNER_TOKEN" \
    --name "$RUNNER_NAME" --labels "$RUNNER_LABELS" --work /home/runner/_work
unset RUNNER_TOKEN
exec ./run.sh
