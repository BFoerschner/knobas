#!/bin/sh
# Fetch the two Data Center timebomb licence keys knobas' real Jira and
# Confluence containers need, from Atlassian's public page, and hand them to
# the shell.
#
#   eval "$(./fetch-timebomb-keys.sh)"     # exports both keys into this shell
#   ./fetch-timebomb-keys.sh --write       # also writes testenv/.env.licences
#
# WHY A SCRIPT: the keys are public strings on one page, need no Atlassian
# account, and are the only free Data Center licences left since self-service
# trials ended 2026-03-30. They are 10-user keys that expire THREE HOURS after
# being applied to an instance -- the same key starts a fresh container again,
# so fetching once per run costs nothing and never goes stale. Typing them by
# hand was #273's human step; this replaces it.
#
# HOW THE PAGE IS READ: the page ships its own markdown inside a JSON string,
# so the code block under each "10 user <product> Data Center license, expires
# in 3 hours" heading is what is extracted, whitespace stripped (the key is
# wrapped over several lines). A key is accepted only if it is base64 and
# decodes to the 10-user, 3-hour Data Center licence for that product.
# If Atlassian reshapes the page, this fails loudly with the heading it could
# not find, rather than exporting an empty variable for the wizard to reject
# forty minutes later.
#
# `.env.licences` is covered by the tracked `.gitignore` (`.env.*`). Nothing
# here is secret, but the seed scripts refuse `.env` for keys on principle, and
# a public key today is not a promise about the next one.
set -eu
cd "$(dirname "$0")"

PAGE=https://developer.atlassian.com/platform/marketplace/timebomb-licenses-for-testing-server-apps/
OUT=.env.licences

die() { echo "fetch-timebomb-keys: $*" >&2; exit 1; }

command -v python3 >/dev/null || die "python3 is needed to read the page"

html=$(curl -sfL -A 'knobas-testenv (fetch-timebomb-keys.sh)' "$PAGE") \
  || die "could not fetch $PAGE"

# One line per product: NAME KEY. python3 does the extraction and the decode
# check; the shell only formats.
# shellcheck disable=SC2016  # the single quotes hold a Python program; its $ are Python's
pairs=$(printf '%s' "$html" | python3 -c '
import re, sys, base64, zlib, struct
s = sys.stdin.read()
wanted = {
  "JIRA_LICENSE_KEY": ("10 user Jira Software Data Center license, expires in 3 hours", "jira"),
  "CONFLUENCE_LICENSE_KEY": ("10 user Confluence Data Center license, expires in 3 hours", "conf"),
}
def decode(key):
    # An Atlassian key is base64(4-byte big-endian payload length || payload)
    # followed by a signature; the payload is a short marker then zlib-
    # compressed Java properties naming the product, edition and expiry. The
    # length prefix is what finds the payload boundary -- the signature
    # follows it inside the same base64 run, so nothing else delimits it.
    n = struct.unpack(">I", base64.b64decode(key[:8] + "==")[:4])[0]
    nchars = -(-(4 + n) // 3) * 4
    payload = base64.b64decode(key[:nchars])[4:4 + n]
    return zlib.decompress(payload[payload.find(b"\x78"):]).decode("utf-8", "replace")
for var, (heading, product) in wanted.items():
    m = re.search(re.escape(heading) + r".{0,40}?```\s*bash\\n(.*?)```", s, re.S)
    if not m:
        sys.exit(f"heading not found on the page: {heading!r}")
    key = "".join(m.group(1).replace("\\n", "\n").replace("\\\"", "\"").split())
    if not re.fullmatch(r"[A-Za-z0-9+/=]+", key):
        sys.exit(f"{var}: extracted text is not base64")
    try:
        props = decode(key)
    except Exception as e:
        sys.exit(f"{var}: key does not decode as an Atlassian licence: {e}")
    want = (f"{product}.DataCenter=true", "LicenseExpiryDate=P3H", "NumberOfUsers=10")
    missing = [w for w in want if w not in props]
    if missing:
        sys.exit(f"{var}: decoded licence lacks {missing} -- not the 10-user 3-hour Data Center key")
    print(var, key)
') || exit 1

write=0; [ "${1:-}" = "--write" ] && write=1
[ $write -eq 1 ] && : > "$OUT"
printf '%s\n' "$pairs" | while read -r var key; do
  line="export $var='$key'"
  echo "$line"
  [ $write -eq 1 ] && echo "$line" >> "$OUT"
done
[ $write -eq 1 ] && echo "fetch-timebomb-keys: wrote $OUT" >&2
exit 0
