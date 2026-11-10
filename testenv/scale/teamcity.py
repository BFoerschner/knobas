"""TeamCity at Björn's scale, as far as this server's licence lets it: a
`Scale` project tree of 40 projects beside the Tidewater ones, 85 build
configurations (the free Professional licence stops at 100 in all, and the
fixture, the chain and the live suite's own need the rest), and ~3,800 builds
with mixed outcomes, test results and artifacts.

Each leaf project builds one of the scale Gitea repositories through its own
Git root, without checking anything out (`checkoutMode=MANUAL`): a build
still carries the branch and revision it ran on, and the agent spends no time
cloning. A build's outcome, tests and artifact come from parameters given at
queue time to one shell step, so a build takes about as long as the agent
takes to pick it up. Extra agents (`--agents N`, the licence allows three) are
started beside `knobas-teamcity-agent` for the run and removed after it.

Builds are queued in batches and waited for; the count already there is read
per configuration, so a rerun queues only what is missing. Their dates are
the server's own: TeamCity takes none over REST (see the README).
"""

from __future__ import annotations

import json
import subprocess
import time

from common import (COMPONENTS, Http, HttpError, chunks, die, parallel, q, rng, say, sentence, summary,
                    weighted, zipf_pick)

P = "teamcity"
ROOT = "Scale"
GROUPS = [
    ("Platform", ["Gateway", "Mesh", "Deploy", "Terraform"]),
    ("Payments", ["Ledger", "Payouts", "Refunds", "Cards"]),
    ("Commerce", ["Checkout", "Cart", "Pricing", "Catalog"]),
    ("Data", ["Warehouse", "Streams", "Reporting", "Features"]),
    ("Mobile", ["Ios", "Android", "Sync", "Push"]),
    ("Web", ["Storefront", "Admin", "Cms", "DesignSystem"]),
    ("Security", ["Scanning", "Secrets", "Audit"]),
    ("Tools", ["Ci", "Bots", "Docs", "Release"]),
]
CONFIG_NAMES = ["Build", "Unit tests", "Integration tests", "Deploy to staging", "Lint", "Nightly",
                "Deploy to production", "Security scan", "Benchmarks", "E2E tests"]
CONFIGS = 85
MAX_CONFIGS = 100
# A literal percent sign in the step is doubled: TeamCity reads a word between
# two of them as a parameter, and one it cannot resolve becomes an implicit
# requirement no agent meets (every build waited, "no compatible agents").
STEP = r"""#!/bin/sh
# testenv/seed-scale.sh: one build of the scale estate. What it does is the
# queue's parameters: tests, failures, an artifact, an exit code.
echo "##teamcity[progressMessage 'running the scale step']"
n=${SCALE_TESTS:-0}; f=${SCALE_FAILED:-0}; i=0
while [ "$i" -lt "$n" ]; do
  t="com.tidewater.${SCALE_SUITE:-core}.Test$((i / 10)).case$i"
  echo "##teamcity[testStarted name='$t']"
  [ "$i" -lt "$f" ] && echo "##teamcity[testFailed name='$t' message='expected 200 but was 500']"
  echo "##teamcity[testFinished name='$t' duration='$(( (i * 37) %% 900 ))']"
  i=$((i + 1))
done
if [ "${SCALE_ARTIFACT:-0}" = 1 ]; then
  echo "build %build.number%" > report.txt
  echo "##teamcity[publishArtifacts 'report.txt']"
fi
[ -n "${SCALE_LOG:-}" ] && echo "$SCALE_LOG"
exit "${SCALE_EXIT:-0}"
"""


def projects():
    """[(id, name, parent id)], leaves with the repository each builds."""
    out = [(ROOT, "Scale test", "_Root", None)]
    # The repositories the Gitea scale seed makes at its default size.
    import gitea
    repos = [(r.org, r.name) for r in gitea.plan(10000)]
    k = 0
    for g, leaves in GROUPS:
        gid = f"{ROOT}_{g}"
        out.append((gid, g, ROOT, None))
        for leaf in leaves:
            org, repo = repos[(k * 7) % len(repos)]
            out.append((f"{gid}_{leaf}", leaf, gid, (org, repo)))
            k += 1
    return out


def configs():
    leaves = [p for p in projects() if p[3]]
    r = rng(P, "configs")
    per = {p[0]: 2 for p in leaves}
    extra = CONFIGS - 2 * len(leaves)
    for p in r.sample(leaves, len(leaves))[:extra]:
        per[p[0]] += 1
    for p in r.sample(leaves, len(leaves))[:max(0, CONFIGS - sum(per.values()))]:
        per[p[0]] += 1
    out = []
    for pid, _, _, repo in leaves:
        for name in CONFIG_NAMES[:per[pid]]:
            out.append((f"{pid}_{name.replace(' ', '')}", name, pid, repo))
    return out


def plan(total):
    """Builds per configuration (a zipf share of `total`) and each build's
    branch slot, outcome and tests, from its own generator."""
    cfgs = configs()
    r = rng(P, "shares")
    weights = [r.lognormvariate(0, 1.0) * (3 if c[1] in ("Build", "Unit tests") else 1) for c in cfgs]
    s = sum(weights)
    n = [max(3, int(total * w / s)) for w in weights]
    n[0] += total - sum(n)
    out = []
    for (cid, name, pid, repo), m in zip(cfgs, n):
        builds = []
        for j in range(m):
            br = rng(P, cid, j)
            b = {"main": br.random() < 0.65 or name.startswith("Deploy") or name == "Nightly"}
            x = br.random()
            tests = 0 if name in ("Lint", "Deploy to staging", "Deploy to production") else int(br.lognormvariate(3, 1.2))
            tests = min(tests, 400)
            if x < 0.78:
                b.update(exit=0, failed=0)
            elif x < 0.88:
                b.update(exit=1, failed=0, log=f"error: {summary(br).lower()}")
            elif x < 0.96 and tests:
                b.update(exit=0, failed=br.randint(1, max(1, min(tests, 12))))
            elif x < 0.985:
                b.update(exit=0, failed=0, cancel=True)
            else:
                b.update(exit=137, failed=0, log="Killed: out of memory")
            b["tests"] = tests
            b["artifact"] = br.random() < 0.3
            b["suite"] = name.split()[0].lower()
            b["slot"] = br.randrange(1000)
            builds.append(b)
        out.append(((cid, name, pid, repo), builds))
    return out


def describe(total):
    pl = plan(total)
    nb = sum(len(b) for _, b in pl)
    print(f"projects: {len(projects())}, build configurations: {len(pl)} (licence: {MAX_CONFIGS} in all)")
    print(f"builds: {nb}, most on one configuration: {max(len(b) for _, b in pl)}")
    print(f"failed: {sum(1 for _, bs in pl for b in bs if b['exit'] or b['failed'])}, "
          f"cancelled: {sum(1 for _, bs in pl for b in bs if b.get('cancel'))}")
    print(f"test results: {sum(b['tests'] for _, bs in pl for b in bs)}")


class TeamCity:
    def __init__(self, state, total, workers, agents):
        t = state.get("teamcity") or die(P, "seed-state.json has no teamcity block: just live-up teamcity first")
        self.url, self.token = t["url"], t["token"]
        self.h = Http(self.url, headers={"Authorization": f"Bearer {self.token}"})
        self.gitea_token = (state.get("gitea") or {}).get("token") or die(P, "no gitea token in seed-state.json")
        self.total, self.workers, self.agents = total, workers, agents
        self.made = {"projects": 0, "vcs roots": 0, "configurations": 0, "builds": 0}

    def run(self):
        self.h.get("/app/rest/server")
        have = self.h.get("/app/rest/buildTypes?locator=count:1000&fields=count,buildType(id)")
        mine = {b["id"] for b in have.get("buildType", []) if b["id"].startswith(ROOT + "_")}
        others = have["count"] - len(mine)
        if others + CONFIGS > MAX_CONFIGS:
            die(P, f"{others} build configurations outside {ROOT} and {CONFIGS} here is over the licence's {MAX_CONFIGS}: "
                   f"clear the strays first (just teamcity-live-seeded's leftover clearing does)")
        for pid, name, parent, repo in projects():
            self.project(pid, name, parent, repo)
        for cid, name, pid, repo in configs():
            self.config(cid, name, pid)
        if not self.marked("builds"):
            started = self.start_agents()
            try:
                self.builds()
            finally:
                self.stop_agents(started)
            self.mark("builds")
        if not self.marked("dates"):
            self.dates()
            self.mark("dates")
        self.report()

    # A parameter on the Scale project per finished phase, valued with the
    # count it was finished for: what a rerun reads instead of the builds.
    def marked(self, phase):
        try:
            v = self.h.get(f"/app/rest/projects/id:{ROOT}/parameters/knobas.scale.seed.{phase}")
            return str(v.get("value")) == str(self.total)
        except HttpError as e:
            if e.status == 404:
                return False
            raise

    def mark(self, phase):
        self.h.put(f"/app/rest/projects/id:{ROOT}/parameters/knobas.scale.seed.{phase}",
                   {"name": f"knobas.scale.seed.{phase}", "value": str(self.total)})

    # -- structure --------------------------------------------------------------------
    def project(self, pid, name, parent, repo):
        try:
            self.h.get(f"/app/rest/projects/id:{pid}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/app/rest/projects", {"id": pid, "name": name, "parentProject": {"id": parent}})
            self.h.put(f"/app/rest/projects/id:{pid}/description",
                       raw=b"Scale test data made by testenv/seed-scale.sh. Not fixture content.",
                       headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
            self.made["projects"] += 1
        if repo is None:
            return
        rid = f"{pid}_Git"
        try:
            self.h.get(f"/app/rest/vcs-roots/id:{rid}")
        except HttpError as e:
            if e.status != 404:
                raise
            org, name_ = repo
            self.h.post("/app/rest/vcs-roots", {
                "id": rid, "name": f"{org}/{name_}", "vcsName": "jetbrains.git", "project": {"id": pid},
                "properties": {"property": [
                    {"name": "url", "value": f"http://gitea:3000/{org}/{name_}.git"},
                    {"name": "branch", "value": "refs/heads/main"},
                    {"name": "teamcity:branchSpec", "value": "+:refs/heads/*"},
                    {"name": "authMethod", "value": "PASSWORD"},
                    {"name": "username", "value": "knobas"},
                    {"name": "secure:password", "value": self.gitea_token}]}})
            self.made["vcs roots"] += 1

    def config(self, cid, name, pid):
        try:
            self.h.get(f"/app/rest/buildTypes/id:{cid}")
        except HttpError as e:
            if e.status != 404:
                raise
            self.h.post("/app/rest/buildTypes", {"id": cid, "name": name, "project": {"id": pid}})
            self.made["configurations"] += 1
        entries = self.h.get(f"/app/rest/buildTypes/id:{cid}/vcs-root-entries")
        if not entries.get("vcs-root-entry"):
            self.h.post(f"/app/rest/buildTypes/id:{cid}/vcs-root-entries", {"vcs-root": {"id": f"{pid}_Git"}})
        self.h.put(f"/app/rest/buildTypes/id:{cid}/settings/checkoutMode", raw=b"MANUAL",
                   headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
        steps = self.h.get(f"/app/rest/buildTypes/id:{cid}/steps")
        if steps.get("step"):
            sid = steps["step"][0]["id"]
            have = self.h.request("GET", f"/app/rest/buildTypes/id:{cid}/steps/{sid}/parameters/script.content",
                                  headers={"Accept": "text/plain"})[1]
            if have != STEP:
                self.h.put(f"/app/rest/buildTypes/id:{cid}/steps/{sid}/parameters/script.content", raw=STEP.encode(),
                           headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
        else:
            self.h.post(f"/app/rest/buildTypes/id:{cid}/steps", {
                "name": name, "type": "simpleRunner", "properties": {"property": [
                    {"name": "use.custom.script", "value": "true"}, {"name": "script.content", "value": STEP}]}})

    # -- agents -----------------------------------------------------------------------
    def scale_agents(self):
        got = self.h.get("/app/rest/agents?locator=defaultFilter:false&fields=agent(id,name,connected,authorized)")
        return [x for x in got.get("agent", []) if x["name"].startswith("scale-agent-")]

    def forget_agents(self):
        # An agent record outlives its container, and an authorized one holds
        # one of the licence's three slots; a new container under a taken name
        # registers as `<name>-1` instead.
        for x in self.scale_agents():
            if x["authorized"]:
                self.h.put(f"/app/rest/agents/id:{x['id']}/authorized", raw=b"false",
                           headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
            try:
                self.h.request("DELETE", f"/app/rest/agents/id:{x['id']}", ok=(204, 404))
            except HttpError as e:
                # Its container is gone but the server has not noticed yet;
                # unauthorized, it holds no slot, and a later pass removes it.
                if e.status != 500:
                    raise

    def start_agents(self):
        started = []
        if self.agents <= 1:
            return started
        image = subprocess.run(["docker", "inspect", "--format", "{{.Config.Image}}", "knobas-teamcity-agent"],
                               capture_output=True, text=True, check=True).stdout.strip()
        net = subprocess.run(["docker", "inspect", "--format", "{{range $k, $v := .NetworkSettings.Networks}}{{$k}}{{end}}",
                              "knobas-teamcity-agent"], capture_output=True, text=True, check=True).stdout.strip()
        running = set(subprocess.run(["docker", "ps", "--format", "{{.Names}}"], capture_output=True, text=True,
                                     check=True).stdout.split())
        before = {x["id"] for x in self.scale_agents()}
        for k in range(2, self.agents + 1):
            name = f"knobas-scale-agent-{k}"
            started.append(name)
            if name in running:
                continue     # a run that was killed left it: keep it, it is registered
            subprocess.run(["docker", "rm", "-f", name], capture_output=True)
            subprocess.run(["docker", "run", "-d", "--name", name, "--network", net, "--memory", "1g",
                            "-e", "SERVER_URL=http://teamcity:8111", "-e", f"AGENT_NAME=scale-agent-{k}", image],
                           check=True, capture_output=True)
        fresh = len(started) - len([n for n in started if n in running])
        say(P, f"agents {', '.join(started)} ({fresh} started now); waiting for them to register")
        deadline = time.time() + 900
        while True:
            every = self.scale_agents()
            # A record from before this start that still reads connected is a
            # container just removed; only a new record, or one whose
            # container was kept, is an agent.
            live = [x for x in every if x["connected"] and (x["id"] not in before or fresh == 0)]
            stale = [x for x in every if x not in live and x["authorized"]]
            for x in stale:
                self.h.put(f"/app/rest/agents/id:{x['id']}/authorized", raw=b"false",
                           headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
            for x in live:
                if not x["authorized"]:
                    self.h.put(f"/app/rest/agents/id:{x['id']}/authorized", raw=b"true",
                               headers={"Content-Type": "text/plain", "Accept": "text/plain"}, ok=(200,))
            if len(live) >= len(started):
                return started
            if time.time() > deadline:
                die(P, f"only {len(live)} of {len(started)} extra agents registered within 15 min")
            time.sleep(10)

    def stop_agents(self, started):
        for name in started:
            subprocess.run(["docker", "rm", "-f", name], capture_output=True)
        deadline = time.time() + 300
        self.forget_agents()
        while self.scale_agents() and time.time() < deadline:
            time.sleep(15)
            self.forget_agents()
        if started:
            say(P, f"removed {', '.join(started)} and their agent records")

    # -- builds -------------------------------------------------------------------------
    def branches(self, repo):
        org, name = repo
        g = Http("http://127.0.0.1:3000", headers={"Authorization": f"token {self.gitea_token}"})
        out, page = [], 1
        while True:
            got = g.get(f"/api/v1/repos/{org}/{name}/branches?limit=50&page={page}")
            if not got:
                return sorted(out)
            out += [b["name"] for b in got if b["name"] != "main"]
            page += 1

    def builds(self):
        pl = plan(self.total)
        branches = {}
        jobs = []
        for (cid, name, pid, repo), bs in pl:
            have = self.h.get(f"/app/rest/builds?locator=buildType:(id:{cid}),defaultFilter:false,state:any,canceled:any,"
                              f"count:10000&fields=count")["count"]
            if have >= len(bs):
                continue
            if repo not in branches:
                branches[repo] = self.branches(repo)
            for j, b in enumerate(bs[have:], have):
                jobs.append((cid, j, b, branches[repo]))
        if not jobs:
            return
        say(P, f"queueing {len(jobs)} builds, one configuration's builds in order")
        # Round-robin over the configurations, so every configuration's
        # history grows through the run instead of one after another.
        by_cfg = {}
        for job in jobs:
            by_cfg.setdefault(job[0], []).append(job)
        order = []
        while by_cfg:
            for cid in list(by_cfg):
                order.append(by_cfg[cid].pop(0))
                if not by_cfg[cid]:
                    del by_cfg[cid]
        t0 = time.time()
        done = 0
        for batch in chunks(order, 60):
            ids = parallel(P, "queue", batch, self.queue, self.workers, 10 ** 9)
            self.wait(ids)
            done += len(batch)
            self.made["builds"] += len(batch)
            rate = done / max(time.time() - t0, 1)
            say(P, f"builds {done}/{len(order)} ({rate * 60:.0f}/min, ~{(len(order) - done) / max(rate, 0.01) / 60:.0f} min left)")

    def queue(self, job):
        cid, j, b, branches = job
        props = [{"name": "env.SCALE_EXIT", "value": str(b["exit"])},
                 {"name": "env.SCALE_TESTS", "value": str(b["tests"])},
                 {"name": "env.SCALE_FAILED", "value": str(b["failed"])},
                 {"name": "env.SCALE_ARTIFACT", "value": "1" if b["artifact"] else "0"},
                 {"name": "env.SCALE_SUITE", "value": b["suite"]}]
        if b.get("log"):
            props.append({"name": "env.SCALE_LOG", "value": b["log"]})
        body = {"buildType": {"id": cid}, "properties": {"property": props},
                "comment": {"text": f"testenv/seed-scale.sh: scale build {j + 1}"}}
        if not b["main"] and branches:
            body["branchName"] = branches[b["slot"] % len(branches)]
        res = self.h.post("/app/rest/buildQueue", body)
        if b.get("cancel"):
            self.h.post(f"/app/rest/buildQueue/id:{res['id']}", {"comment": "superseded", "readdIntoQueue": False},
                        ok=(200, 404))
        return res["id"]

    def wait(self, ids):
        deadline = time.time() + 1800
        pending = set(ids)
        while pending:
            if time.time() > deadline:
                die(P, f"{len(pending)} builds still queued or running after 30 min, e.g. {sorted(pending)[:5]}")
            for c in chunks(sorted(pending), 50):
                items = ",".join(f"item:(id:{i})" for i in c)
                got = self.h.get(f"/app/rest/builds?locator={items},defaultFilter:false&fields=build(id,state)")
                for bld in got.get("build", []):
                    if bld["state"] == "finished":
                        pending.discard(bld["id"])
            if pending:
                time.sleep(3)

    def dates(self):
        """Spread each configuration's builds over the two years, in order,
        keeping each build's own duration: TeamCity takes no dates over REST,
        so they are written into its internal HSQLDB with the server stopped."""
        import datetime as dt
        from common import END, START
        got = self.h.get(f"/app/rest/builds?locator=affectedProject:(id:{ROOT}),defaultFilter:false,canceled:any,"
                         f"count:100000&fields=build(id,buildTypeId,queuedDate,startDate,finishDate)")["build"]

        def ms(v):
            return int(dt.datetime.strptime(v, "%Y%m%dT%H%M%S%z").timestamp() * 1000) if v else None

        by_cfg = {}
        for b in got:
            by_cfg.setdefault(b["buildTypeId"], []).append(b)
        sql = []
        span = (END - START).total_seconds() * 1000
        for cid, bs in by_cfg.items():
            bs.sort(key=lambda b: b["id"])
            r = rng(P, cid, "dates")
            for j, b in enumerate(bs):
                anchor = ms(b.get("startDate")) or ms(b.get("queuedDate"))
                if anchor is None:
                    continue
                # Later builds later, busier lately, office hours mostly.
                frac = ((j + r.random()) / len(bs)) ** 0.85
                t = START + dt.timedelta(milliseconds=span * frac)
                t = t.replace(hour=r.randint(6, 20), minute=r.randint(0, 59), second=r.randint(0, 59))
                d = int(min(t, END).timestamp() * 1000) - anchor
                def sh(col):   # -1 or NULL is "never": a build cancelled in the queue never started
                    return f"{col} = CASE WHEN {col} > 0 THEN {col} + {d} ELSE {col} END"
                cols = ["BUILD_START_TIME_SERVER", "BUILD_START_TIME_AGENT", "BUILD_FINISH_TIME_SERVER",
                        "QUEUED_TIME", "REMOVE_FROM_QUEUE_TIME"]
                for table in ("HISTORY", "LIGHT_HISTORY"):
                    sql.append(f"UPDATE {table} SET {', '.join(sh(c) for c in cols)} WHERE BUILD_ID = {b['id']}")
                sql.append(f"UPDATE BUILD_STATE SET {sh('QUEUED_TIME')}, {sh('REMOVE_FROM_QUEUE_TIME')} WHERE BUILD_ID = {b['id']}")
        image = subprocess.run(["docker", "inspect", "--format", "{{.Config.Image}}", "knobas-teamcity"],
                               capture_output=True, text=True, check=True).stdout.strip()
        volume = subprocess.run(["docker", "inspect", "--format",
                                 '{{range .Mounts}}{{if eq .Destination "/data/teamcity_server/datadir"}}{{.Name}}{{end}}{{end}}',
                                 "knobas-teamcity"], capture_output=True, text=True, check=True).stdout.strip()
        say(P, f"stopping TeamCity to rewrite {len(got)} builds' dates in its internal database")
        subprocess.run(["docker", "stop", "-t", "180", "knobas-teamcity"], check=True, capture_output=True)
        import io
        import os
        import tarfile
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tar:
            tar.add(os.path.join(os.path.dirname(os.path.abspath(__file__)), "Sql.java"), arcname="Sql.java")
            data = ("\n".join(sql) + "\n").encode()
            info = tarfile.TarInfo("dates.sql")
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        try:
            p = subprocess.run(["docker", "run", "--rm", "-i", "--entrypoint", "sh", "-v", f"{volume}:/data/teamcity_server/datadir",
                                image, "-c", "mkdir -p /tmp/s && tar -x -C /tmp/s && java -cp "
                                "/opt/teamcity/webapps/ROOT/WEB-INF/lib/hsqldb-jdk8.jar /tmp/s/Sql.java "
                                "jdbc:hsqldb:file:/data/teamcity_server/datadir/system/buildserver < /tmp/s/dates.sql"],
                               input=buf.getvalue(), capture_output=True)
            if p.returncode:
                die(P, f"the date rewrite failed: {p.stderr.decode()[-1500:]}")
            say(P, p.stderr.decode().strip().splitlines()[-1])
        finally:
            subprocess.run(["docker", "start", "knobas-teamcity"], check=True, capture_output=True)
        deadline = time.time() + 900
        while True:
            try:
                self.h.request("GET", "/app/rest/server", tries=1)
                break
            except Exception:
                if time.time() > deadline:
                    die(P, "TeamCity did not answer within 15 min of its restart")
                time.sleep(10)
        first = min(got, key=lambda b: b["id"])
        now = self.h.get(f"/app/rest/builds/id:{first['id']}?fields=startDate,queuedDate")
        say(P, f"build {first['id']} now reads started {now.get('startDate') or now.get('queuedDate')}")

    def report(self):
        n = self.h.get(f"/app/rest/builds?locator=affectedProject:(id:{ROOT}),defaultFilter:false,canceled:any,"
                       f"count:100000&fields=count")["count"]
        cfg = self.h.get(f"/app/rest/buildTypes?locator=affectedProject:(id:{ROOT})&fields=count")["count"]
        pr = self.h.get(f"/app/rest/projects?locator=affectedProject:(id:{ROOT})&fields=count")["count"]
        say(P, f"done: {pr + 1} projects, {cfg} build configurations, {n} builds under {ROOT}; this run made "
               + ", ".join(f"{v} {k}" for k, v in self.made.items()))


def main(state, count, workers, agents=3):
    TeamCity(state, count, workers, agents).run()
