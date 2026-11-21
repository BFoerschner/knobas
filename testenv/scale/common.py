"""What every product's scale seed shares: a fixed clock, seeded randomness,
prose that reads like an estate's, and an HTTP client that keeps its
connections and retries what a busy server drops.

Stdlib only: the seed runs on a bare python3, as `seed-scale.sh` calls it.
"""

from __future__ import annotations

import base64
import datetime as dt
import hashlib
import http.client
import json
import random
import sys
import threading
import time
import urllib.parse
from concurrent.futures import ThreadPoolExecutor, as_completed

# The estate's seed. Every random choice below is drawn from a generator keyed
# on this, the product and the thing being made, so a rerun (or a rerun with a
# smaller --count) makes the same thing again, in any order.
SEED = 20261110

# The clock the estate is dated by: two years ending on a fixed day, not
# "now", so the dates are the same on every run.
END = dt.datetime(2026, 11, 1, 17, 0, tzinfo=dt.timezone.utc)
START = END - dt.timedelta(days=730)
SPAN_S = (END - START).total_seconds()


def rng(*parts) -> random.Random:
    """A generator of its own for one thing: `rng("jira", "SCLPAY", 17)`."""
    key = ":".join(str(p) for p in (SEED, *parts))
    return random.Random(int.from_bytes(hashlib.sha256(key.encode()).digest()[:8], "big"))


def say(product: str, msg: str) -> None:
    print(f"seed-scale {product}: {msg}", flush=True)


def die(product: str, msg: str) -> None:
    print(f"seed-scale {product}: {msg}", file=sys.stderr, flush=True)
    sys.exit(1)


def when(i: int, n: int, r: random.Random, jitter_days: float = 6.0) -> dt.datetime:
    """The date of item `i` of `n` made over the two years: in order, so a
    later key is a later item, with a few days' jitter, clamped to the span."""
    base = START + dt.timedelta(seconds=SPAN_S * (i + 0.5) / max(n, 1))
    t = base + dt.timedelta(days=r.uniform(-jitter_days, jitter_days))
    # Office hours, mostly: 07:00 to 19:00 UTC.
    t = t.replace(hour=r.randint(7, 18), minute=r.randint(0, 59), second=r.randint(0, 59))
    return min(max(t, START), END - dt.timedelta(minutes=5))


def later(t: dt.datetime, r: random.Random, median_days: float, sigma: float = 1.0) -> dt.datetime:
    """A time after `t`, log-normally distributed around `median_days`, never past END."""
    import math

    d = r.lognormvariate(math.log(max(median_days, 0.01)), sigma)
    return min(t + dt.timedelta(days=d), END - dt.timedelta(minutes=1))


def iso(t: dt.datetime) -> str:
    return t.strftime("%Y-%m-%dT%H:%M:%S.000+0000")


def weighted(r: random.Random, table):
    """One key of a [(key, weight), ...] table."""
    total = sum(w for _, w in table)
    x = r.uniform(0, total)
    for k, w in table:
        x -= w
        if x <= 0:
            return k
    return table[-1][0]


def zipf_pick(r: random.Random, items, s: float = 1.1):
    """An item, the first ones far more often than the last: a few epics,
    parents or repositories hold most of the children, as in a real estate."""
    n = len(items)
    weights = [1.0 / (k + 1) ** s for k in range(n)]
    return r.choices(items, weights)[0]


# -- prose ---------------------------------------------------------------------

COMPONENTS = [
    "payout scheduler", "ledger writer", "settlement batch", "refund service", "card gateway",
    "fraud scoring", "invoice renderer", "webhook relay", "auth proxy", "session store",
    "search indexer", "notification fan-out", "pricing engine", "checkout flow", "cart service",
    "mobile sync", "push gateway", "report exporter", "audit log", "feature flags",
    "rate limiter", "billing cron", "SEPA connector", "FX quotes", "KYC checks",
    "data warehouse loader", "event bus", "schema registry", "metrics pipeline", "log shipper",
    "deploy pipeline", "secrets rotation", "DNS automation", "load balancer", "Kafka consumer",
    "Postgres primary", "read replica", "Redis cache", "object storage", "CDN edge",
    "admin console", "partner API", "onboarding wizard", "dashboard widgets", "CSV import",
]
VERBS = [
    "Fix", "Add", "Remove", "Refactor", "Investigate", "Document", "Migrate", "Speed up",
    "Harden", "Retry", "Validate", "Paginate", "Cache", "Instrument", "Split", "Deprecate",
    "Upgrade", "Backfill", "Throttle", "Rename", "Expose", "Audit", "Reconcile", "Simplify",
]
PROBLEMS = [
    "times out under load", "double-counts partial refunds", "drops events after a restart",
    "leaks connections", "ignores the retry budget", "returns 500 on empty input",
    "logs secrets at debug level", "shows stale totals", "fails on leap days",
    "rounds half-cents the wrong way", "blocks the event loop", "is missing an index",
    "spams the on-call channel", "rejects valid IBANs", "loses the cursor on failover",
    "breaks on non-ASCII names", "OOMs on large exports", "races with the nightly job",
    "renders wrong in dark mode", "has no alerting", "retries forever", "skips the last page",
]
OBJECTS = [
    "idempotency keys", "the retry queue", "structured logs", "the health endpoint",
    "p99 latency dashboards", "the batch window", "a feature flag", "the admin export",
    "webhook signatures", "the migration script", "the runbook", "connection pooling",
    "the CSV parser", "error budgets", "trace sampling", "the config loader",
    "TLS certificates", "the backoff policy", "the cache key", "pagination tokens",
]
WORDS = (
    "the a service request queue batch retry ledger payout customer partner window "
    "latency error budget deploy rollback cache index query migration schema event "
    "consumer producer timeout threshold alert dashboard metric trace span log "
    "config flag release branch review test fixture staging production region "
    "replica primary failover backlog incident postmortem owner team sprint "
    "estimate scope risk dependency contract API endpoint payload response "
    "validation edge case regression benchmark throughput capacity quota "
    "is was will should must can may needs runs fails passes reads writes "
    "sends drops keeps moves checks blocks waits retries handles returns "
    "after before during when while because so but and or with without "
    "every some most each one two three new old slow fast large small "
    "manual automatic nightly hourly weekly critical minor known open closed"
).split()


def sentence(r: random.Random, lo: int = 6, hi: int = 18) -> str:
    n = r.randint(lo, hi)
    words = [r.choice(WORDS) for _ in range(n)]
    if r.random() < 0.4:
        words.insert(r.randrange(len(words)), r.choice(COMPONENTS))
    s = " ".join(words)
    return s[0].upper() + s[1:] + r.choice([".", ".", ".", "?", "!"])


def paragraph(r: random.Random, lo: int = 2, hi: int = 7) -> str:
    return " ".join(sentence(r) for _ in range(r.randint(lo, hi)))


def summary(r: random.Random, kind: str = "") -> str:
    comp = r.choice(COMPONENTS)
    shape = r.random()
    if kind == "Bug" or (not kind and shape < 0.3):
        s = f"{comp.capitalize()} {r.choice(PROBLEMS)}"
    elif kind == "Epic":
        s = r.choice([
            f"{comp.capitalize()} v{r.randint(2, 4)}", f"Q{r.randint(1, 4)} {comp} reliability",
            f"Move {comp} to the new platform", f"{comp.capitalize()} self-service",
            f"Retire the legacy {comp}", f"{comp.capitalize()} for EU customers",
        ])
    elif shape < 0.7:
        s = f"{r.choice(VERBS)} {r.choice(OBJECTS)} in the {comp}"
    else:
        s = f"{r.choice(VERBS)} {comp} {r.choice(OBJECTS)}"
    if r.random() < 0.08:  # the odd very long title
        s += " (" + sentence(r, 8, 20).rstrip(".!?") + ")"
    return s


def length_class(r: random.Random) -> int:
    """How many paragraphs a body gets: mostly short, a long tail."""
    return min(int(r.lognormvariate(0.9, 0.9)), 60)


# -- HTTP ------------------------------------------------------------------------


class HttpError(Exception):
    def __init__(self, method, path, status, body):
        super().__init__(f"{method} {path}: HTTP {status}: {body[:600]}")
        self.status = status
        self.body = body


class Http:
    """One base URL, basic or token auth, one keep-alive connection per thread,
    retries for the statuses a server under bulk load answers transiently."""

    RETRY = {429, 500, 502, 503, 504}

    def __init__(self, base: str, *, user=None, password=None, headers=None, timeout=120):
        u = urllib.parse.urlsplit(base.rstrip("/"))
        self.scheme, self.host, self.port = u.scheme, u.hostname, u.port
        self.prefix = u.path
        self.timeout = timeout
        self.headers = dict(headers or {})
        if user is not None:
            self.headers["Authorization"] = basic(user, password)
        self.local = threading.local()

    def _conn(self):
        c = getattr(self.local, "conn", None)
        if c is None:
            cls = http.client.HTTPSConnection if self.scheme == "https" else http.client.HTTPConnection
            c = cls(self.host, self.port, timeout=self.timeout)
            self.local.conn = c
        return c

    def _drop(self):
        c = getattr(self.local, "conn", None)
        if c is not None:
            try:
                c.close()
            except Exception:
                pass
        self.local.conn = None

    def request(self, method, path, body=None, *, headers=None, ok=(200, 201, 204), raw=None,
                ctype="application/json", tries=6):
        h = dict(self.headers)
        h["Accept"] = "application/json"
        if headers:
            h.update(headers)
        data = None
        if raw is not None:
            data = raw
        elif body is not None:
            data = json.dumps(body).encode()
            h["Content-Type"] = ctype
        last = None
        for attempt in range(tries):
            try:
                c = self._conn()
                c.request(method, self.prefix + path, body=data, headers=h)
                resp = c.getresponse()
                text = resp.read().decode("utf-8", "replace")
                status = resp.status
                if resp.getheader("Connection", "").lower() == "close":
                    self._drop()
            except (http.client.HTTPException, OSError) as e:
                self._drop()
                last = e
                time.sleep(min(2 ** attempt, 30))
                continue
            if status in self.RETRY and attempt < tries - 1:
                last = HttpError(method, path, status, text)
                time.sleep(min(2 ** attempt, 30))
                continue
            if ok is not None and status not in ok:
                raise HttpError(method, path, status, text)
            if not text:
                return status, None
            try:
                return status, json.loads(text)
            except ValueError:
                return status, text
        raise RuntimeError(f"{method} {path}: gave up after {tries} tries: {last}")

    def get(self, path, **kw):
        return self.request("GET", path, **kw)[1]

    def post(self, path, body=None, **kw):
        return self.request("POST", path, body, **kw)[1]

    def put(self, path, body=None, **kw):
        return self.request("PUT", path, body, **kw)[1]


def basic(user, password) -> str:
    return "Basic " + base64.b64encode(f"{user}:{password}".encode()).decode()


def q(s: str) -> str:
    return urllib.parse.quote(s, safe="")


def parallel(product: str, what: str, jobs, fn, workers: int = 12, every: int = 500):
    """Run fn(job) for every job on `workers` threads, printing progress every
    `every` and stopping at the first failure, which it re-raises."""
    jobs = list(jobs)
    if not jobs:
        return []
    out = [None] * len(jobs)
    t0 = time.time()
    done = 0
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(fn, j): k for k, j in enumerate(jobs)}
        try:
            for f in as_completed(futs):
                out[futs[f]] = f.result()
                done += 1
                if done % every == 0 or done == len(jobs):
                    rate = done / max(time.time() - t0, 0.001)
                    say(product, f"{what}: {done}/{len(jobs)} ({rate:.0f}/s)")
        except BaseException:
            for f in futs:
                f.cancel()
            raise
    return out


def chunks(seq, n):
    seq = list(seq)
    for k in range(0, len(seq), n):
        yield seq[k:k + n]
