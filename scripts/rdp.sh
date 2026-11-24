#!/usr/bin/env bash
# Open the running AppVeyor worker in the Mac RDP client with host, port and
# user already filled in, and the one-time password on the clipboard.
set -euo pipefail
cd "$(dirname "$0")/.."

API=https://ci.appveyor.com/api
PROJECT=Jostein/radio
AUTH="Authorization: Bearer $(tr -d '\r\n' < .appveyor-token)"

read -r build version age <<<"$(curl -sf -H "$AUTH" \
  "$API/projects/$PROJECT/history?recordsNumber=10" |
  python3 -c 'import json,sys,re,datetime
b=[x for x in json.load(sys.stdin).get("builds",[]) if x["status"]=="running"]
if not b:
    print("", "", ""); raise SystemExit
x=b[0]
c=re.sub(r"\.(\d+)", lambda m: "."+(m.group(1)+"000000")[:6], x["created"])
t=datetime.datetime.fromisoformat(c.replace("Z","+00:00"))
n=datetime.datetime.now(datetime.timezone.utc)
print(x["buildId"], x["version"], int((n-t).total_seconds()//60))')"

if [ -z "${build:-}" ]; then
  echo "No running AppVeyor build: the worker is gone. Ask me to start a new one."
  exit 1
fi

# A job is killed at 60 minutes, but AppVeyor keeps reporting it as running.
if [ "${age:-0}" -ge 55 ]; then
  echo "Build $version is ${age} min old and its worker has expired; nothing to connect to."
  echo "Ask me for a new build (it takes ~14 min to reach the pause)."
  exit 1
fi

job=$(curl -sf -H "$AUTH" "$API/projects/$PROJECT/build/$version" |
  python3 -c 'import json,sys; print(json.load(sys.stdin)["build"]["jobs"][0]["jobId"])')

details=$(curl -sf -H "$AUTH" "$API/buildjobs/$job/log" | tr -d '\r' |
  awk '/Remote Desktop connection details/{f=1} f')

field() { printf '%s\n' "$details" | awk -v k="$1" 'index($0,k){sub(/^.*: /,""); print; exit}'; }
server=$(field 'Server:')
user=$(field 'Username:')
pass=$(field 'Password:')

if [ -z "$server" ] || [ -z "$pass" ]; then
  echo "Build $version has not reached the RDP pause yet; try again in a minute."
  exit 1
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
