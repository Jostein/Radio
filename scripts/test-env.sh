#!/usr/bin/env bash
# Bring up a Windows desktop to test the tray app, starting from nothing.
#
# Reuses a paused AppVeyor worker when one is alive; otherwise it starts a build
# on the prototype branch and waits for the Remote Desktop pause. The build
# launches radio.exe before it pauses, so arriving in the session means the tray
# icon is already in the notification area: left-click it, click a station,
# listen. Sound is set to "play on this computer" in the .rdp file, so what the
# app plays comes out of the Mac.
#
#   scripts/test-env.sh          # reuse a live worker, else start one
#   scripts/test-env.sh --new    # always start a fresh build
#
# The pause expires on its own (RADIO_RDP_MINUTES, 20 by default), so a session
# nobody uses cannot hold the account's only worker for long.
set -euo pipefail
cd "$(dirname "$0")/.."

API=https://ci.appveyor.com/api
PROJECT=Jostein/radio
BRANCH=flyout-prototype
AUTH="Authorization: Bearer $(tr -d '\r\n' < .appveyor-token)"

# A session window is named "<build> - <server>"; the device list is just
# "Windows App". Counting the former tells a real session from the list.
session_windows() {
  osascript -e 'tell application "System Events" to tell process "Windows App" to return (name of every window as string)' 2>/dev/null |
    tr ',' '\n' | grep -c ' - ' || true
}

# A client that has lost a session silently ignores the .rdp file and shows its
# device list instead, so it gets quit and asked once more. Without this the
# script reports a session that never appeared.
ensure_session() {
  local rdp wait=0
  rdp=$(ls -t .rdp/*.rdp | head -1)

  while [ "$wait" -lt 20 ]; do
    [ "$(session_windows)" -gt 0 ] && return 0
    sleep 4
    wait=$((wait + 4))
  done

  echo "No session window yet; restarting Windows App."
  osascript -e 'quit app "Windows App"' >/dev/null 2>&1 || true
  sleep 3
  open -a "Windows App" "$rdp"

  wait=0
  while [ "$wait" -lt 30 ]; do
    [ "$(session_windows)" -gt 0 ] && return 0
    sleep 5
    wait=$((wait + 5))
  done

  echo "The session window still did not appear. Open $rdp by hand."
  return 1
}

if [ "${1:-}" != "--new" ]; then
  if scripts/rdp.sh; then
    ensure_session
    exit 0
  fi
  echo "No worker to attach to; starting a build of $BRANCH."
fi

# A push, or an earlier run of this script, may already have a build waiting
# for the worker. Starting another would only queue behind it, so wait for the
# one that is next in line instead.
read -r version build <<<"$(curl -sf -H "$AUTH" "$API/projects/$PROJECT/history?recordsNumber=20" |
  python3 -c 'import json,sys
b=[x for x in json.load(sys.stdin)["builds"] if x["status"] in ("queued", "starting")]
print(min(b, key=lambda x: int(x["version"]))["version"], min(b, key=lambda x: int(x["version"]))["buildId"]) if b else print("", "")')"

if [ -n "${version:-}" ]; then
  echo "Build $version is already waiting for the worker."
else
  read -r version build <<<"$(curl -sf -X POST -H "$AUTH" -H 'Content-Type: application/json' \
    -d "{\"accountName\":\"Jostein\",\"projectSlug\":\"radio\",\"branch\":\"$BRANCH\"}" \
    "$API/account/Jostein/builds" | python3 -c 'import json,sys
r=json.load(sys.stdin)
b=r.get("build", r)
print(b["version"], b["buildId"])')"

  if [ -z "${version:-}" ]; then
    echo "AppVeyor did not accept the build request."
    exit 1
  fi
  echo "Build $version queued: https://ci.appveyor.com/project/Jostein/radio/builds/$build"
fi

# Waiting is mostly queue: the account has one worker, so this build starts only
# after every earlier paused one has expired. Status is printed when it changes,
# and the pause itself is detected in the job log, where the credentials are.
status=""
deadline=$((SECONDS + 50 * 60))
while [ "$SECONDS" -lt "$deadline" ]; do
  state=$(curl -sf -H "$AUTH" "$API/projects/$PROJECT/build/$version" |
    python3 -c 'import json,sys; b=json.load(sys.stdin)["build"]; print(b["status"], b["jobs"][0]["jobId"])')
  job=${state##* }
  state=${state%% *}

  case "$state" in
    failed | cancelled)
      echo "Build $version ended as $state."
      exit 1
      ;;
  esac

  if [ "$state" != "$status" ]; then
    echo "Build $version is $state ($((SECONDS / 60)) min into the wait)."
    status=$state
  fi

  if [ "$state" = "running" ]; then
    if curl -sf -H "$AUTH" "$API/buildjobs/$job/log" | grep -q 'Remote Desktop connection details'; then
      break
    fi
  fi

  sleep 15
done

if [ "$SECONDS" -ge "$deadline" ]; then
  echo "Gave up after 50 minutes. The worker is still building or the queue is long."
  exit 1
fi

if ! scripts/rdp.sh; then
  echo "The worker paused but its credentials could not be read; check the build log."
  exit 1
fi
ensure_session
