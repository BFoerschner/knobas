/**
 * The JAR view's fixture (ux19): two Java archives and the `archive_*` reads the view makes of them, in the shapes the
 * backend answers (`knobas_jar`): a plain library JAR, and a Spring Boot fat JAR of 10,000 entries with 150 libraries
 * inside, the scale the view is timed at. The view's tests read it, and `?fake-jar` (`shell/dev/fake/jar.ts`) serves
 * it; nothing else imports it, so it is in no production build.
 *
 * `java` `"nojava"` answers that no Java is found (no *Decompile*), `"oldjava"` a Java 11 (*Decompile* disabled, its
 * reason beside it). A class decompiles to a short made-up source.
 */

import type { ArchiveIndex, ClassOutline, Lib, Member } from "../ipc/entity";
import type { JavaRuntime } from "../ipc/app";

export const JAR_FILES: Record<string, number> = {
  "build/libs/ledger-core-2.3.1.jar": 184_312,
  "build/libs/payout-service-0.0.1-SNAPSHOT.jar": 61_204_118,
};

const LIBS: [string, string][] = [
  ["spring-boot", "3.2.0"], ["spring-boot-autoconfigure", "3.2.0"], ["spring-core", "6.1.1"], ["spring-context", "6.1.1"],
  ["spring-beans", "6.1.1"], ["spring-web", "6.1.1"], ["spring-webmvc", "6.1.1"], ["spring-jdbc", "6.1.1"],
  ["jackson-databind", "2.15.3"], ["jackson-core", "2.15.3"], ["jackson-annotations", "2.15.3"], ["jackson-datatype-jsr310", "2.15.3"],
  ["tomcat-embed-core", "10.1.16"], ["tomcat-embed-el", "10.1.16"], ["tomcat-embed-websocket", "10.1.16"], ["snakeyaml", "2.2"],
  ["logback-classic", "1.4.11"], ["logback-core", "1.4.11"], ["slf4j-api", "2.0.9"], ["micrometer-core", "1.12.0"],
  ["micrometer-commons", "1.12.0"], ["HdrHistogram", "2.1.12"], ["postgresql", "42.6.0"], ["HikariCP", "5.0.1"],
  ["guava", "32.1.3-jre"], ["commons-lang3", "3.13.0"], ["jakarta.annotation-api", "2.1.1"], ["jakarta.validation-api", "3.0.2"],
  ["hibernate-validator", "8.0.1.Final"], ["flyway-core", "9.22.3"],
];

function libs(n: number): Lib[] {
  const out: Lib[] = [];
  for (let i = 0; i < n; i += 1) {
    const [name, version] = LIBS[i % LIBS.length]!;
    const tag = i < LIBS.length ? name : `${name}-ext${Math.floor(i / LIBS.length)}`;
    out.push({ entry: `BOOT-INF/lib/${tag}-${version}.jar`, name: tag, version, size: 40_000 + ((i * 7919) % 900_000) });
  }
  return out;
}

function columns(entries: [string, number][]): Pick<ArchiveIndex, "names" | "sizes" | "packed_sizes" | "flags" | "total" | "packed"> {
  entries.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  const sizes = entries.map(([, s]) => s);
  const packed = entries.map(([n, s]) => (n.endsWith(".jar") ? s : Math.round(s * 0.45)));
  return {
    names: entries.map(([n]) => n).join("\n"),
    sizes,
    packed_sizes: packed,
    flags: entries.map(([n]) => (n.endsWith(".jar") ? 8 : 0)),
    total: sizes.reduce((a, b) => a + b, 0),
    packed: packed.reduce((a, b) => a + b, 0),
  };
}

const PKGS = ["api", "api.dto", "batch", "config", "domain", "domain.events", "ledger", "ledger.rules", "payout", "payout.export", "repo", "web", "web.admin", "util"];

function plain(handle: string): ArchiveIndex {
  const entries: [string, number][] = [
    ["META-INF/MANIFEST.MF", 412],
    ["META-INF/maven/com.tidewater/ledger-core/pom.properties", 61],
    ["META-INF/maven/com.tidewater/ledger-core/pom.xml", 2_104],
    ["com/tidewater/ledger/Ledger.class", 4_211],
    ["com/tidewater/ledger/Ledger$Entry.class", 1_802],
    ["com/tidewater/ledger/Posting.class", 2_331],
    ["com/tidewater/ledger/rules/Rule.class", 611],
    ["com/tidewater/ledger/rules/Rounding.class", 1_402],
    ["ledger-defaults.properties", 318],
    ["db/migration/V1__ledger.sql", 1_240],
    ["logo.png", 4_812],
  ];
  return {
    handle,
    kind: "jar",
    size: JAR_FILES["build/libs/ledger-core-2.3.1.jar"]!,
    ...columns(entries),
    manifest: [
      ["Manifest-Version", "1.0"],
      ["Created-By", "Maven JAR Plugin 3.3.0"],
      ["Build-Jdk-Spec", "17"],
      ["Built-By", "mara"],
      ["Implementation-Title", "ledger-core"],
      ["Implementation-Version", "2.3.1"],
      ["Automatic-Module-Name", "com.tidewater.ledger"],
    ],
    maven: [{ group: "com.tidewater", artifact: "ledger-core", version: "2.3.1", name: "Ledger core", description: "Double-entry postings for payouts.", packaging: "jar", path: "META-INF/maven/com.tidewater/ledger-core/" }],
    release: "Java 17",
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
  };
}

function fat(handle: string): ArchiveIndex {
  const lib = libs(150);
  const entries: [string, number][] = [
    ["META-INF/MANIFEST.MF", 702],
    ["META-INF/maven/com.tidewater/payout-service/pom.properties", 66],
    ["META-INF/maven/com.tidewater/payout-service/pom.xml", 4_918],
    ["BOOT-INF/classpath.idx", 9_340],
    ["BOOT-INF/layers.idx", 212],
    ["BOOT-INF/classes/application.yml", 1_118],
    ["BOOT-INF/classes/application-prod.yml", 402],
    ["BOOT-INF/classes/db/migration/V1__init.sql", 2_880],
    ["BOOT-INF/classes/db/migration/V2__payouts.sql", 1_906],
    ["BOOT-INF/classes/static/README.md", 640],
  ];
  for (let i = 0; entries.length < 10_000 - lib.length - 40; i += 1) {
    const pkg = PKGS[i % PKGS.length]!.replace(/\./g, "/");
    const name = `Payout${["Service", "Controller", "Record", "Mapper", "Rule", "Event", "Config"][i % 7]}${i}`;
    entries.push([`BOOT-INF/classes/com/tidewater/payout/${pkg}/${name}.class`, 900 + ((i * 131) % 6_000)]);
    if (i % 9 === 0) entries.push([`BOOT-INF/classes/com/tidewater/payout/${pkg}/${name}$Builder.class`, 700 + ((i * 17) % 1_500)]);
  }
  for (let i = 0; i < 40; i += 1) entries.push([`org/springframework/boot/loader/launch/Loader${i}.class`, 1_200 + i * 13]);
  for (const l of lib) entries.push([l.entry, l.size]);
  return {
    handle,
    kind: "boot-jar",
    size: JAR_FILES["build/libs/payout-service-0.0.1-SNAPSHOT.jar"]!,
    ...columns(entries),
    manifest: [
      ["Manifest-Version", "1.0"],
      ["Created-By", "Maven JAR Plugin 3.3.0"],
      ["Build-Jdk-Spec", "21"],
      ["Implementation-Title", "payout-service"],
      ["Implementation-Version", "0.0.1-SNAPSHOT"],
      ["Main-Class", "org.springframework.boot.loader.launch.JarLauncher"],
      ["Start-Class", "com.tidewater.payout.PayoutApplication"],
      ["Spring-Boot-Version", "3.2.0"],
      ["Spring-Boot-Classes", "BOOT-INF/classes/"],
      ["Spring-Boot-Lib", "BOOT-INF/lib/"],
      ["Spring-Boot-Classpath-Index", "BOOT-INF/classpath.idx"],
      ["Spring-Boot-Layers-Index", "BOOT-INF/layers.idx"],
    ],
    maven: [{ group: "com.tidewater", artifact: "payout-service", version: "0.0.1-SNAPSHOT", name: "Payout service", description: null, packaging: "jar", path: "META-INF/maven/com.tidewater/payout-service/" }],
    release: "Java 21",
    releases: [],
    libs: lib,
    prefix: 0,
    warnings: [],
  };
}

function nestedOf(handle: string, entry: string): ArchiveIndex {
  const stem = entry.slice(entry.lastIndexOf("/") + 1).replace(/\.jar$/, "");
  const dash = stem.search(/-\d/);
  const name = dash > 0 ? stem.slice(0, dash) : stem;
  const version = dash > 0 ? stem.slice(dash + 1) : "";
  const pkg = `org/${name.replace(/[^a-z]/gi, "").toLowerCase()}`;
  const entries: [string, number][] = [["META-INF/MANIFEST.MF", 380]];
  for (let i = 0; i < 180; i += 1) entries.push([`${pkg}/${["core", "io", "util", "spi"][i % 4]}/Part${i}.class`, 1_000 + i * 7]);
  return {
    handle,
    kind: "jar",
    size: 300_000,
    ...columns(entries),
    manifest: [["Manifest-Version", "1.0"], ["Implementation-Title", name], ["Implementation-Version", version]],
    maven: [],
    release: "Java 17",
    releases: [],
    libs: [],
    prefix: 0,
    warnings: [],
  };
}

function member(over: Partial<Member>): Member {
  return { annotations: [], modifiers: "public", type_params: "", ty: "void", name: "x", params: "", throws: [], value: null, deprecated: false, constant: false, ...over };
}

function outline(entry: string): ClassOutline {
  const binary = entry.replace(/^BOOT-INF\/classes\//, "").replace(/\.class$/, "").replace(/\//g, ".");
  const simple = binary.slice(binary.lastIndexOf(".") + 1);
  return {
    major: 65,
    minor: 0,
    release: "Java 21",
    kind: "class",
    modifiers: "public",
    name: binary,
    type_params: "",
    extends: null,
    implements: ["java.io.Serializable"],
    permits: [],
    annotations: ["@org.springframework.stereotype.Service"],
    components: [],
    fields: [
      member({ modifiers: "private static final", ty: "long", name: "serialVersionUID", params: null, value: "1L" }),
      member({ modifiers: "private final", ty: "com.tidewater.ledger.Ledger", name: "ledger", params: null }),
    ],
    methods: [
      member({ ty: "", name: simple.replace(/^.*\$/, ""), params: "com.tidewater.ledger.Ledger ledger" }),
      member({ ty: "java.util.List<com.tidewater.ledger.Posting>", name: "postings", params: "String account, java.time.LocalDate from, java.time.LocalDate to" }),
      member({ annotations: ["@org.springframework.transaction.annotation.Transactional"], ty: "void", name: "settle", params: "long payoutId", throws: ["java.io.IOException"] }),
    ],
    synthetic: 1,
    source_file: `${simple.replace(/\$.*$/, "")}.java`,
    deprecated: false,
    module: [],
  };
}

const TEXT: Record<string, string> = {
  "application.yml": "server:\n  port: 8080\nspring:\n  datasource:\n    url: jdbc:postgresql://db:5432/payout\n  flyway:\n    enabled: true\n",
  "application-prod.yml": "logging:\n  level:\n    root: WARN\n",
  "V1__init.sql": "create table payout (\n  id bigint primary key,\n  amount numeric(18, 2) not null\n);\n",
  "V2__payouts.sql": "alter table payout add column settled_at timestamptz;\n",
  "README.md": "# Static files\n\nServed under `/static`.\n",
  "ledger-defaults.properties": "ledger.currency=EUR\nledger.rounding=HALF_EVEN\n",
  "V1__ledger.sql": "create table posting (id bigint primary key);\n",
  "MANIFEST.MF": "Manifest-Version: 1.0\nCreated-By: Maven JAR Plugin 3.3.0\n",
};

/** A 1 x 1 PNG. */
const PNG = Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0));

function runtime(asked: string | null): JavaRuntime {
  if (asked === "nojava") return { java: null, decompiler: "Vineflower 1.12.0", needs: 17, can_decompile: false, reason: null, set_path: null };
  if (asked === "oldjava")
    return { java: { path: "/usr/bin/java", version: "11.0.21", release: 11 }, decompiler: "Vineflower 1.12.0", needs: 17, can_decompile: false, reason: "Java 11 found; Decompile needs Java 17", set_path: null };
  return { java: { path: "/usr/bin/java", version: "21.0.4", release: 21 }, decompiler: "Vineflower 1.12.0", needs: 17, can_decompile: true, reason: null, set_path: null };
}

/** The archive reads, answered from this fixture, by command name as the bridge takes them. */
export interface JarAnswers {
  archive_open: (args: Record<string, unknown>) => ArchiveIndex;
  archive_nested: (args: Record<string, unknown>) => ArchiveIndex;
  archive_entry: (args: Record<string, unknown>) => Uint8Array;
  archive_class: (args: Record<string, unknown>) => ClassOutline;
  archive_decompile: (args: Record<string, unknown>) => string;
  archive_download: (args: Record<string, unknown>) => string;
  java_runtime: (args: Record<string, unknown>) => JavaRuntime;
  set_java_path: (args: Record<string, unknown>) => JavaRuntime;
}

export function jarAnswers(java: string | null = null): JarAnswers {
  let next = 0;
  /** Each handle's archive: the path it was opened from, or the nested entry. */
  const opened = new Map<string, string>();
  return {
    archive_open: (args) => {
      const path = String(args["path"]);
      if (JAR_FILES[path] === undefined) throw { code: "not_found", message: `gitea: no file ${path}`, source_id: null };
      next += 1;
      const handle = `a${next}`;
      opened.set(handle, path);
      return path.includes("payout-service") ? fat(handle) : plain(handle);
    },
    archive_nested: (args) => {
      const handle = `${String(args["handle"])}/${(next += 1)}`;
      opened.set(handle, String(args["entry"]));
      return nestedOf(handle, String(args["entry"]));
    },
    archive_entry: (args) => {
      const entry = String(args["entry"]);
      const base = entry.slice(entry.lastIndexOf("/") + 1);
      if (base.endsWith(".png")) return PNG;
      const text = TEXT[base];
      if (text !== undefined) return new TextEncoder().encode(text);
      return Uint8Array.of(0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65, 0);
    },
    archive_class: (args) => outline(String(args["entry"])),
    archive_decompile: (args) => {
      const o = outline(String(args["entry"]).replace(/\$[^/]*\.class$/, ".class"));
      const simple = o.name.slice(o.name.lastIndexOf(".") + 1);
      const pkg = o.name.slice(0, o.name.lastIndexOf("."));
      return `package ${pkg};\n\nimport com.tidewater.ledger.Ledger;\nimport com.tidewater.ledger.Posting;\nimport java.io.IOException;\nimport java.time.LocalDate;\nimport java.util.List;\nimport org.springframework.stereotype.Service;\n\n@Service\npublic class ${simple} implements java.io.Serializable {\n   private static final long serialVersionUID = 1L;\n   private final Ledger ledger;\n\n   public ${simple}(Ledger ledger) {\n      this.ledger = ledger;\n   }\n\n   public List<Posting> postings(String account, LocalDate from, LocalDate to) {\n      return this.ledger.between(account, from, to);\n   }\n\n   public void settle(long payoutId) throws IOException {\n      this.ledger.post(payoutId);\n   }\n}\n`;
    },
    archive_download: (args) => {
      const handle = String(args["handle"]);
      const entry = args["entry"] === null || args["entry"] === undefined ? null : String(args["entry"]);
      const from = entry ?? opened.get(handle) ?? "archive.jar";
      return `/Users/mara/Downloads/${from.slice(from.lastIndexOf("/") + 1)}`;
    },
    java_runtime: () => runtime(java),
    set_java_path: () => runtime(java),
  };
}
