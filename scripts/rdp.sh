#!/usr/bin/env bash
# Open the running AppVeyor worker in the Mac RDP client with host, port and
# user already filled in, and the one-time password on the clipboard.
set -euo pipefail
cd "$(dirname "$0")/.."

API=https://ci.appveyor.com/api
PROJECT=Jostein/radio
# Must match RADIO_RDP_MINUTES in appveyor.yml, which expires the pause on its
# own; attaching to a worker after that means connecting into a build that has
# already carried on and been reclaimed.
PAUSE_MINUTES=${RADIO_RDP_MINUTES:-20}
AUTH="Authorization: Bearer $(tr -d '\r\n' < .appveyor-token)"

read -r build version <<<"$(curl -sf -H "$AUTH" \
  "$API/projects/$PROJECT/history?recordsNumber=10" |
  python3 -c 'import json,sys
b=[x for x in json.load(sys.stdin).get("builds",[]) if x["status"]=="running"]
print(b[0]["buildId"], b[0]["version"]) if b else print("", "")')"

if [ -z "${build:-}" ]; then
  echo "No running AppVeyor build: the worker is gone. Ask me to start a new one."
  exit 1
fi

# The job's own start, not the build's creation: a build can sit in the queue
# for an hour before a worker takes it, and that hour says nothing about
# whether the desktop is still there.
read -r job age <<<"$(curl -sf -H "$AUTH" "$API/projects/$PROJECT/build/$version" |
  python3 -c 'import json,sys,re,datetime
b=json.load(sys.stdin)["build"]
j=b["jobs"][0]
stamp=(j.get("started") or b.get("created")).split("+")[0]
c=re.sub(r"\.(\d+)", lambda m: "."+(m.group(1)+"000000")[:6], stamp)
t=datetime.datetime.fromisoformat(c)
n=datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)
print(j["jobId"], int((n-t).total_seconds()//60))')"

# A job is killed at 60 minutes, but AppVeyor keeps reporting it as running.
if [ "${age:-0}" -ge 55 ]; then
  echo "Build $version has been running ${age} min; its worker expires at 60 and is gone."
  echo "Ask me for a new build (it takes ~14 min to reach the pause)."
  exit 1
fi

log=$(curl -sf -H "$AUTH" "$API/buildjobs/$job/log" | tr -d '\r')
details=$(printf '%s\n' "$log" | awk '/Remote Desktop connection details/{f=1} f')

field() { printf '%s\n' "$details" | awk -v k="$1" 'index($0,k){sub(/^.*: /,""); print; exit}'; }
server=$(field 'Server:')
user=$(field 'Username:')
pass=$(field 'Password:')

if [ -z "$server" ] || [ -z "$pass" ]; then
  echo "Build $version has not reached the RDP pause yet; try again in a minute."
  exit 1
fi

# The log stamps the pause with the job's own clock (hours since the job
# started), so the pause age is the job age minus that stamp. A worker whose
# pause has expired is still reported as running for a while; connecting to it
# just hangs.
paused=$(printf '%s\n' "$log" | grep -o '^\[[0-9][0-9]:[0-9][0-9]:[0-9][0-9]\] Build paused' |
  tail -1 | cut -d']' -f1 | tr -d '[' || true)
if [ -n "$paused" ]; then
  paused_min=$(printf '%s\n' "$paused" | awk -F: '{print $1 * 60 + $2}')
  pause_age=$((age - paused_min))
  if [ "$pause_age" -ge $((PAUSE_MINUTES - 2)) ]; then
    echo "Build $version paused ${pause_age} min ago; the pause expires after $PAUSE_MINUTES and the desktop has gone."
    echo "Ask me to start a new one."
    exit 1
  fi
fi

mkdir -p .rdp
rdp=".rdp/radio-$version.rdp"
cat >"$rdp" <<EOF
full address:s:$server
username:s:$user
prompt for credentials:i:0
authentication level:i:0
screen mode id:i:2
use multimon:i:0
redirectclipboard:i:1
audiomode:i:0
EOF

printf '%s' "$pass" | pbcopy
for app in "Windows App" "Microsoft Remote Desktop"; do
  if [ -d "/Applications/$app.app" ]; then
    open -a "$app" "$rdp"
    break
  fi
done

echo "Build $version → $server as $user"
echo "Password (also on your clipboard): $pass"
