/* The fixture: one TeamCity (mockups/shared/dataset.md), 13:20 was the last look at Builds, now is 14:32 on Fri 22 Aug.
   Three kinds of name on purpose: TeamCity's own (`Payout_Build`), names that repeat across folders (*Build*, *Test*),
   and names too long for any cell. */
const SOURCE = "Tidewater TeamCity";
const YOU = "Mara Lindqvist";
const DAYS = ["Today", "Yesterday", "Wed 20 Aug", "Tue 19 Aug"];
const OLD_DAYS = ["Mon 18 Aug", "Fri 15 Aug", "Thu 14 Aug"];
const CLOCK = { min: 14 * 60 + 32, sec: 0 };

const TREE = {
  "Payments/Payouts": ["Payout_Build", "Payout_IntegrationTests", "Payout_Lint", "Payout_Docker"],
  "Payments/Payouts/Deploy": ["Payout_Deploy_Staging", "Payout_Deploy_Prod"],
  "Payments/Payouts/Dashboard": ["Build", "Test"],
  "Payments/Ledger": ["Ledger_Build", "Ledger_Tests", "Ledger_Migrations_Check"],
  "Payments/PSP client": ["Build", "Test", "Release"],
  "Logistics/Booking/API": ["Build", "Test"],
  "Logistics/Booking/Web": ["Build", "Test", "End-to-end against staging (Chromium, Firefox, WebKit)"],
  "Logistics/Customs": ["CustomsGateway_Build", "CustomsGateway_Contract", "EdiParser_Fuzz"],
  "Logistics/Tracking": ["TrackingEvents_Build", "CarrierAdapters_Contract_Verification_Nightly", "EtaModel_Train"],
  "Platform/Infrastructure": ["Terraform_Plan", "Terraform_Apply_Stage", "Helm_Lint"],
  "Platform/Services/Auth gateway": ["Build", "Test"],
  "Platform/Services/Notification hub": ["Build", "Test"],
  "Data": ["Dbt_Run_Nightly", "Dbt_Test", "WarehouseSync_Build"],
};
const PINS = ["Payments/Payouts/Payout_IntegrationTests", "Payments/Payouts/Payout_Build", "Payments/Payouts/Deploy/Payout_Deploy_Staging"];

const PIPELINES = Object.entries(TREE).flatMap(([path, names]) =>
  names.map((name) => ({ id: `${path}/${name}`, name, path: path.split("/"), pinned: PINS.includes(`${path}/${name}`) })));
const PIPE = new Map(PIPELINES.map((p) => [p.id, p]));

const PAY = "feature/PAY-231-sepa-retry";
const QB = "feature/quote-banner";
const VCS = "VCS trigger";
const CRON = "Schedule";
const hm = (text) => { const [h, m] = text.split(":").map(Number); return h * 60 + m; };

/* One run: [pipeline, number, state, day, started, seconds, branch, started by, tests failed/passed, mark].
   state: ok err warn run q muted. mark: what counts it in a bubble (docs: README, *Bubbles*):
   "b" running and not looked at, "g" passed while you were away, "r" failed and unread, "" read. */
const RUNS = [
  ["Logistics/Booking/Web/Build", 2101, "run", 0, "14:29", null, QB, "Jonas Becker", null, "b"],
  ["Platform/Infrastructure/Terraform_Plan", null, "q", 0, "14:27", null, "main", VCS, null, ""],
  ["Payments/Payouts/Payout_Build", 1188, "run", 0, "14:28", null, PAY, YOU, null, "b"],
  ["Logistics/Booking/Web/End-to-end against staging (Chromium, Firefox, WebKit)", 2093, "err", 0, "13:58", 700, QB, "Jonas Becker", [1, 212], "r"],
  ["Payments/PSP client/Build", 233, "ok", 0, "13:41", 125, "main", VCS, null, "g"],
  ["Payments/PSP client/Test", 518, "ok", 0, "13:40", 228, "main", VCS, [0, 164], "g"],
  ["Logistics/Booking/API/Build", 871, "ok", 0, "13:12", 151, "main", VCS, null, ""],
  ["Logistics/Booking/API/Test", 1203, "ok", 0, "13:10", 362, "main", VCS, [0, 388], ""],
  ["Logistics/Tracking/EtaModel_Train", 52, "run", 0, "13:00", null, "main", CRON, null, ""],
  ["Payments/Payouts/Deploy/Payout_Deploy_Staging", 318, "ok", 0, "12:04", 72, PAY, YOU, null, ""],
  ["Payments/Payouts/Payout_Docker", 611, "ok", 0, "11:52", 200, PAY, YOU, null, ""],
  ["Payments/Payouts/Payout_Lint", 904, "ok", 0, "11:44", 48, PAY, YOU, null, ""],
  ["Payments/Payouts/Payout_IntegrationTests", 1187, "err", 0, "10:05", 252, PAY, YOU, [3, 41], "r"],
  ["Platform/Services/Auth gateway/Test", 140, "ok", 0, "09:41", 270, "main", VCS, [0, 96], ""],
  ["Platform/Services/Auth gateway/Build", 77, "ok", 0, "09:40", 118, "main", VCS, null, ""],
  ["Platform/Services/Notification hub/Test", 129, "err", 0, "09:13", 301, "main", VCS, [2, 88], ""],
  ["Platform/Services/Notification hub/Build", 64, "ok", 0, "09:12", 96, "main", VCS, null, ""],
  ["Logistics/Customs/EdiParser_Fuzz", 88, "err", 0, "06:00", 1800, "main", CRON, null, "r"],
  ["Data/Dbt_Test", 640, "err", 0, "05:30", 410, "main", CRON, [4, 310], ""],
  ["Data/Dbt_Run_Nightly", 1411, "ok", 0, "05:00", 1320, "main", CRON, null, ""],
  ["Logistics/Tracking/CarrierAdapters_Contract_Verification_Nightly", 209, "warn", 0, "02:00", 2460, "main", CRON, [0, 57], ""],
  ["Payments/Payouts/Payout_IntegrationTests", 1186, "err", 1, "17:44", 249, PAY, YOU, [3, 41], "r"],
  ["Payments/Payouts/Payout_Build", 1187, "ok", 1, "17:40", 244, PAY, YOU, null, ""],
  ["Payments/Ledger/Ledger_Tests", 2210, "ok", 1, "16:22", 515, "main", VCS, [0, 512], ""],
  ["Payments/Ledger/Ledger_Migrations_Check", 301, "ok", 1, "16:21", 64, "main", VCS, null, ""],
  ["Payments/Ledger/Ledger_Build", 932, "ok", 1, "16:20", 190, "main", VCS, null, ""],
  ["Payments/Payouts/Dashboard/Test", 612, "ok", 1, "14:35", 340, "main", VCS, [0, 74], ""],
  ["Payments/Payouts/Dashboard/Build", 377, "ok", 1, "14:32", 133, "main", VCS, null, ""],
  ["Logistics/Booking/Web/Test", 3051, "ok", 1, "11:16", 480, QB, "Jonas Becker", [0, 1204], ""],
  ["Logistics/Booking/Web/Build", 2100, "ok", 1, "11:15", 288, QB, "Jonas Becker", null, ""],
  ["Logistics/Customs/CustomsGateway_Contract", 198, "ok", 1, "10:04", 222, "main", VCS, [0, 31], ""],
  ["Logistics/Customs/CustomsGateway_Build", 455, "ok", 1, "10:02", 175, "main", VCS, null, ""],
  ["Platform/Infrastructure/Terraform_Apply_Stage", 88, "muted", 1, "09:30", 35, "main", "Tomasz Wójcik", null, ""],
  ["Platform/Infrastructure/Helm_Lint", 95, "ok", 1, "09:10", 41, "main", VCS, null, ""],
  ["Data/WarehouseSync_Build", 230, "ok", 1, "08:40", 205, "main", VCS, null, ""],
  ["Logistics/Tracking/TrackingEvents_Build", 1410, "ok", 1, "08:12", 164, "main", VCS, null, ""],
  ["Payments/PSP client/Release", 41, "ok", 2, "15:31", 95, "main", "Priya Nair", null, ""],
  ["Payments/Payouts/Deploy/Payout_Deploy_Prod", 204, "ok", 2, "15:02", 88, "main", "Priya Nair", null, ""],
  ["Logistics/Customs/EdiParser_Fuzz", 87, "ok", 2, "06:00", 1795, "main", CRON, null, ""],
  ["Data/Dbt_Run_Nightly", 1410, "ok", 2, "05:00", 1297, "main", CRON, null, ""],
  ["Platform/Infrastructure/Terraform_Plan", 411, "ok", 3, "16:48", 58, "main", VCS, null, ""],
  ["Logistics/Tracking/EtaModel_Train", 51, "ok", 3, "13:00", 6120, "main", CRON, null, ""],
].map(([pipeline, no, state, day, at, secs, branch, by, tests, mark], i) =>
  ({ id: `r${i}`, pipeline, no, state, day, at: hm(at), secs, branch, by, tests, mark }));

/* What *Step* plays, in order: the live changes a sync or a refresh brings while the page is open. */
const EVENTS = [
  { say: "Terraform_Plan left the queue and runs: one more blue on its row, its folders, All pipelines and the top bar", run: (r) => r.pipeline.endsWith("Terraform_Plan") && r.state === "q", to: { state: "run", no: 412, mark: "b" } },
  { say: "Booking › Web › Build #2101 passed in front of you: its blue goes and no green comes", run: (r) => r.no === 2101 && r.state === "run", to: { state: "ok", secs: "now", mark: "" } },
  { say: "Payout_Build #1188 failed: its blue turns red, in its own column", run: (r) => r.no === 1188 && r.state === "run", to: { state: "err", secs: "now", mark: "r", tests: [2, 118] } },
  { say: "A run started on Ledger_Build", add: { pipeline: "Payments/Ledger/Ledger_Build", no: 933, state: "run", day: 0, secs: null, branch: "main", by: VCS, tests: null, mark: "b" } },
];

/* A small fixed-seed generator: the same older history on every load. */
function seeded(seed) { let s = seed >>> 0; return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296); }

/* A pipeline's older runs, which only its own page reads. */
function olderRuns(pipeline, newest) {
  const random = seeded(pipeline.id.length * 31 + pipeline.name.length);
  let no = (newest ?? 40) - 1;
  return OLD_DAYS.flatMap((label, d) => Array.from({ length: 3 }, (_, i) => ({
    id: `old-${d}-${i}`, pipeline: pipeline.id, no: no--, state: random() < 0.12 ? "err" : "ok", dayLabel: label,
    at: hm("17:20") - i * 190 - Math.floor(random() * 40), secs: 60 + Math.floor(random() * 260), branch: random() < 0.6 ? "main" : PAY,
    by: random() < 0.6 ? VCS : YOU, tests: null, mark: "",
  })));
}
