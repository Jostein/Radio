#!/usr/bin/env bash
# Save the newest screen the paused worker uploaded, so the desktop it is
# showing can be looked at from here.
set -euo pipefail
cd "$(dirname "$0")/.."

API=https://ci.appveyor.com/api
AUTH="Authorization: Bearer $(tr -d '\r\n' < .appveyor-token)"

# The history listing carries no jobs, so the job id comes from the build.
read -r job version <<<"$(curl -sf -H "$AUTH" "$API/projects/Jostein/radio/history?recordsNumber=10" |
  python3 -c 'import json,sys
b=[x for x in json.load(sys.stdin)["builds"] if x["status"] == "running"]
print(b[0]["version"]) if b else print("")' |
  while read -r version; do
    [ -n "$version" ] || continue
    curl -sf -H "$AUTH" "$API/projects/Jostein/radio/build/$version" |
      python3 -c 'import json,sys
b=json.load(sys.stdin)["build"]
print(b["jobs"][0]["jobId"], b["version"]) if b["jobs"] else print("", "")'
  done)"

if [ -z "${job:-}" ]; then
  echo "No running build: there is no session to look at."
  exit 1
fi

name=$(curl -sf -H "$AUTH" "$API/buildjobs/$job/artifacts" |
  python3 -c 'import json,sys
a=[x for x in json.load(sys.stdin) if x["fileName"].startswith("live")]
a.sort(key=lambda x: x["fileName"])
print(a[-1]["fileName"] if a else "")')

if [ -z "$name" ]; then
  echo "Build $version has uploaded no screen yet."
  exit 1
fi

mkdir -p .rdp
# The artifact endpoint answers 302 and signs a storage URL; without -L the
# file arrives empty.
curl -sfL -H "$AUTH" "$API/buildjobs/$job/artifacts/$name" -o .rdp/vm-live.png
sips -s format jpeg -s formatOptions 88 .rdp/vm-live.png --out .rdp/vm-live.jpg >/dev/null
echo "build $version, $name -> .rdp/vm-live.jpg"
