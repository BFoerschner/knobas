"""TeamCity at the worst a real team has: project `Extremes` (*Extreme shapes*), one configuration, one build with
5,000 artifacts (3,000 in one folder, 2,000 over 40 folders), 20,000 tests (400 failed) and a log with a 1 MB line.

Not named `Knobas...`: the seeded live suite deletes projects of that name as its own leftovers.
"""

from __future__ import annotations

import re
import subprocess
import time

from common import Http, HttpError, die, say

P = "teamcity"
PROJECT = "Extremes"
CONFIG = "Extremes_Everything"
# The configuration that builds the extreme repository, and the branches it builds.
REPO_CONFIG = "Extremes_BuildsEverything"
REPO_BRANCHES = ["main"] + [f"many/b-{b:04d}" for b in (5000, 4999, 4998, 4500, 4000, 3000, 2000, 1500, 1000, 500, 2, 1)]
MAX_CONFIGS = 100
MAIN_CONFIG = "/data/teamcity_server/datadir/config/main-config.xml"
# `%%` is a literal percent sign: TeamCity reads a word between two single ones as a parameter.
STEP = r"""#!/bin/sh
# testenv/seed-extremes.sh: the extremes' one build.
mkdir -p out/flat
i=1
while [ "$i" -le 3000 ]; do echo "artifact $i" > "out/flat/file-$(printf '%%04d' "$i").txt"; i=$((i + 1)); done
i=1
while [ "$i" -le 2000 ]; do
  d="out/nested/d-$(printf '%%02d' $((i %% 40)))"; mkdir -p "$d"
  echo "artifact $i" > "$d/file-$(printf '%%04d' "$i").txt"; i=$((i + 1))
done
# The server publishes at most 1,000 files at once (its global limit), so in batches.
for g in 0 1 2; do echo "##teamcity[publishArtifacts 'out/flat/file-$g*.txt => flat']"; done
echo "##teamcity[publishArtifacts 'out/flat/file-3*.txt => flat']"
for g in 0 1 2 3; do echo "##teamcity[publishArtifacts 'out/nested/d-$g*/** => nested']"; done
i=0
while [ "$i" -lt 20000 ]; do
  t="com.tidewater.extremes.Suite$((i / 100)).case$i"
  echo "##teamcity[testStarted name='$t']"
  [ $((i %% 50)) -eq 7 ] && echo "##teamcity[testFailed name='$t' message='expected 200 but was 500' details='at Suite.case$i']"
  echo "##teamcity[testFinished name='$t' duration='$(( (i * 37) %% 900 ))']"
  i=$((i + 1))
done
echo "a 1 MB line follows"
head -c 1048576 /dev/zero | tr '\0' 'x'; echo
echo "done"
"""


class TeamCity:
    def __init__(self, state):
        t = state.get("teamcity") or die(P, "seed-state.json has no teamcity block: just live-up teamcity first")
        self.h = Http(t["url"], headers={"Authorization": f"Bearer {t['token']}"})
        self.gitea_token = (state.get("gitea") or {}).get("token", "")

    def run(self):
        try:
            self.h.get(f"/app/rest/projects/id:{PROJECT}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/app/rest/projects", {"id": PROJECT, "name": "Extreme shapes", "parentProject": {"id": "_Root"}})
            say(P, f"created project {PROJECT}")
        try:
            self.h.get(f"/app/rest/buildTypes/id:{CONFIG}")
        except HttpError as e:
            if e.status != 404:
                raise
            n = self.h.get("/app/rest/buildTypes?locator=count:1000&fields=count")["count"]
            if n >= MAX_CONFIGS:
                die(P, f"the server holds {n} configurations; the licence stops at {MAX_CONFIGS}")
            self.h.post("/app/rest/buildTypes", {"id": CONFIG, "name": "Everything", "project": {"id": PROJECT}})
            self.h.post(f"/app/rest/buildTypes/id:{CONFIG}/steps", {
                "name": "extremes", "type": "simpleRunner", "properties": {"property": [
                    {"name": "use.custom.script", "value": "true"}, {"name": "script.content", "value": STEP}]}})
            say(P, f"created configuration {CONFIG}")
        step = self.h.get(f"/app/rest/buildTypes/id:{CONFIG}/steps")["step"][0]["id"]
        path = f"/app/rest/buildTypes/id:{CONFIG}/steps/{step}/parameters/script.content"
        if self.h.request("GET", path, headers={"Accept": "text/plain"})[1] != STEP:
            self.h.put(path, raw=STEP.encode(), headers={"Content-Type": "text/plain", "Accept": "text/plain"})
            say(P, "the step changed: updated it")
        got = self.h.get(f"/app/rest/builds?locator=buildType:(id:{CONFIG}),state:finished,count:1&fields=build(id,status)")
        if got.get("build"):
            bid = got["build"][0]["id"]
            n = self.artifacts(bid)
            if n >= 5000:
                say(P, f"build {bid} is there, {n} artifacts")
                return
            say(P, f"build {bid} has {n} artifacts, not 5,000: deleting it and building again")
            self.h.request("DELETE", f"/app/rest/builds/id:{bid}", ok=(204, 404))
        # The server takes at most `maxArtifactsNumber` files a build (1,000 as shipped): raised for this one build,
        # put back as it was found the moment the build ends. Artifacts published stay.
        found = self.limit()
        self.limit(6000)
        try:
            res = self.h.post("/app/rest/buildQueue", {"buildType": {"id": CONFIG},
                                                       "comment": {"text": "testenv/seed-extremes.sh"}})
            bid = res["id"]
            say(P, f"queued build {bid}")
            for _ in range(360):
                b = self.h.get(f"/app/rest/builds/id:{bid}?fields=state,status,statusText")
                if b["state"] == "finished":
                    break
                time.sleep(5)
            else:
                die(P, f"build {bid} not finished after 30 minutes")
        finally:
            self.limit(found)
        say(P, f"build {bid}: {b['status']} ({b.get('statusText', '')}), {self.artifacts(bid)} artifacts")

    def limit(self, to=None):
        """The server's artifacts-a-build limit, read, or set and waited for (it re-reads its config file)."""
        def sh(cmd):
            p = subprocess.run(["docker", "exec", "knobas-teamcity", "sh", "-c", cmd], capture_output=True)
            if p.returncode:
                die(P, f"docker exec: {p.stderr.decode()[-500:]}")
            return p.stdout.decode()
        now = int(re.search(r'maxArtifactsNumber="(\d+)"', sh(f"cat {MAIN_CONFIG}")).group(1))
        if to is None or to == now:
            return now
        sh(f"sed -i 's/maxArtifactsNumber=\"{now}\"/maxArtifactsNumber=\"{to}\"/' {MAIN_CONFIG}")
        say(P, f"artifacts a build: {now} -> {to}")
        time.sleep(20)
        return to

    def artifacts(self, bid, path=""):
        """How many files the build published, walking its folders."""
        got = self.h.get(f"/app/rest/builds/id:{bid}/artifacts/children/{path}?fields=file(name,children)")
        n = 0
        for f in got.get("file", []):
            if "children" in f:
                n += self.artifacts(bid, f"{path}/{f['name']}" if path else f["name"])
            else:
                n += 1
        return n

    def repo(self):
        """A configuration that builds the extreme Gitea repository (its own Git root, nothing checked out) on `main`
        and on a dozen of its branches, a few of them failing: what a repository's Builds tab draws a rail of
        thousands of branches beside (the extremes pass, 2026-11-13)."""
        self.ensure_project()
        root = f"{PROJECT}_Git"
        try:
            self.h.get(f"/app/rest/vcs-roots/id:{root}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/app/rest/vcs-roots", {
                "id": root, "name": "knobas-extremes/everything", "vcsName": "jetbrains.git", "project": {"id": PROJECT},
                "properties": {"property": [
                    {"name": "url", "value": "http://gitea:3000/knobas-extremes/everything.git"},
                    {"name": "branch", "value": "refs/heads/main"},
                    {"name": "teamcity:branchSpec", "value": "+:refs/heads/*"},
                    {"name": "authMethod", "value": "PASSWORD"},
                    {"name": "username", "value": "knobas"},
                    {"name": "secure:password", "value": self.gitea_token}]}})
            say(P, "created the Git root of knobas-extremes/everything")
        try:
            self.h.get(f"/app/rest/buildTypes/id:{REPO_CONFIG}")
        except HttpError as e:
            if e.status != 404:
                raise
            n = self.h.get("/app/rest/buildTypes?locator=count:1000&fields=count")["count"]
            if n >= MAX_CONFIGS:
                die(P, f"the server holds {n} configurations; the licence stops at {MAX_CONFIGS}")
            self.h.post("/app/rest/buildTypes", {"id": REPO_CONFIG, "name": "Builds everything", "project": {"id": PROJECT}})
            self.h.post(f"/app/rest/buildTypes/id:{REPO_CONFIG}/vcs-root-entries", {"vcs-root": {"id": root}})
            self.h.put(f"/app/rest/buildTypes/id:{REPO_CONFIG}/settings/checkoutMode", raw=b"MANUAL",
                       headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
            self.h.post(f"/app/rest/buildTypes/id:{REPO_CONFIG}/steps", {
                "name": "check", "type": "simpleRunner", "properties": {"property": [
                    {"name": "use.custom.script", "value": "true"},
                    {"name": "script.content", "value": 'echo "checking %teamcity.build.branch%"; exit "${XTRM_EXIT:-0}"'}]}})
            say(P, f"created configuration {REPO_CONFIG}")
        have = self.h.get(f"/app/rest/builds?locator=buildType:(id:{REPO_CONFIG}),defaultFilter:false,branch:default:any,count:100&fields=build(id)")
        if len(have.get("build", [])) >= len(REPO_BRANCHES):
            say(P, f"{REPO_CONFIG} has its builds")
            return
        ids = []
        for k, branch in enumerate(REPO_BRANCHES):
            body = {"buildType": {"id": REPO_CONFIG}, "comment": {"text": "testenv/seed-extremes.sh"},
                    "properties": {"property": [{"name": "env.XTRM_EXIT", "value": "1" if k % 4 == 3 else "0"}]}}
            if branch != "main":
                body["branchName"] = f"refs/heads/{branch}"
            ids.append(self.h.post("/app/rest/buildQueue", body)["id"])
        for _ in range(360):
            got = self.h.get(f"/app/rest/builds?locator=buildType:(id:{REPO_CONFIG}),defaultFilter:false,branch:default:any,"
                             f"state:finished,count:100&fields=build(id)")
            if len(got.get("build", [])) >= len(ids):
                say(P, f"{REPO_CONFIG}: {len(ids)} builds finished")
                return
            time.sleep(5)
        die(P, f"{REPO_CONFIG}'s builds not finished after 30 minutes")

    def ensure_project(self):
        try:
            self.h.get(f"/app/rest/projects/id:{PROJECT}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/app/rest/projects", {"id": PROJECT, "name": "Extreme shapes", "parentProject": {"id": "_Root"}})

    def remove(self):
        try:
            self.h.get(f"/app/rest/projects/id:{PROJECT}")
        except HttpError as e:
            if e.status == 404:
                say(P, f"no project {PROJECT}")
                return
            raise
        for b in self.h.get(f"/app/rest/builds?locator=affectedProject:(id:{PROJECT}),defaultFilter:false,count:100&fields=build(id)").get("build", []):
            self.h.request("DELETE", f"/app/rest/builds/id:{b['id']}", ok=(204, 404))
        self.h.request("DELETE", f"/app/rest/projects/id:{PROJECT}", ok=(204, 404))
        say(P, f"deleted project {PROJECT} and its builds")


def seed(state, workers, parts=None):
    t = TeamCity(state)
    if parts is None or "artifacts" in parts:
        t.run()
    if parts is None or "repo" in parts:
        t.repo()


def remove(state):
    TeamCity(state).remove()
