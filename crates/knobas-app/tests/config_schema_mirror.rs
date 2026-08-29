//! `app/src/lib/sources/fixtures.ts` against the adapters' real `config_schema`.
//!
//! The Add-source form is *generated* from an adapter's `config_schema`, which
//! reaches the frontend over IPC as `unknown`. So `app/` cannot import the
//! schemas and transcribes them instead -- and a transcription drifts. It has
//! already cost one user-facing bug: the fixture spelled `username` as
//! `{type: "string"}` while every adapter spells an optional string
//! `{type: ["string", "null"]}`, so the form fell through to a JSON textarea
//! and typing a username was a parse error unless you knew to quote it (#82,
//! fixed in #110). No test could see it, because the fixture *was* the tests'
//! idea of reality. Two more of the same kind were found afterwards (#124).
//!
//! This file is the side of that comparison that can be made to work, and it
//! is here rather than in `app/` because of what the drift is made of:
//!
//! * The adapter's schema is a real `serde_json::Value`, so
//!   `"maximum": MAX_BUILDS_PER_CONFIG` -- a Rust `const`, and the property
//!   that drifted furthest -- arrives already **resolved to `10000`**. A check
//!   inside `app/` could only have scraped that as text, i.e. could not have
//!   compared the one value it most needed to.
//! * Because both sides are values, the check tells **drift from a legitimate
//!   change**: change an adapter and update the fixture to match and it stays
//!   green; change one without the other and it goes red. A test that pins the
//!   number instead fires either way, which is a value pin doing a drift
//!   detector's job by accident.
//!
//! The idiom is `tests/sources_mirror.rs`'s: `include_str!` the TypeScript,
//! compare it against a real value. The difference is that the mirror there is
//! a set of `interface` declarations, where a key set is the whole content;
//! here the mirror is data, so the comparison is over values.
//!
//! **The rule, taken from the fixture's own header:** *a property that is here
//! is verbatim; a property may be absent, and each absence is named.* The
//! licence to be absent is spent at exactly one level, the `properties` map.
//! Everything else present in the fixture -- including a top-level `required`
//! the adapter never declared -- is compared.
//!
//! **Nothing here skips.** A literal this file cannot parse, a fixture that is
//! no longer `as const`, a fixture that has been renamed away, a fixture with
//! no adapter behind it: each is a panic naming the text. A drift detector
//! that quietly declines to check is the failure this file exists to remove.
//!
//! **What it cannot see, and what does:** absence, and key *order*. A property
//! dropped from the fixture is licensed here by construction, and a
//! `serde_json::Map` has no order to compare -- but the form is generated in
//! the schema's own order, so a reordered fixture draws a different form.
//! `app/src/lib/sources/fixtures.test.ts` pins both: the exact key list per
//! fixture, in order, against the header's named absence lists. It also names
//! this file as the home for the half it cannot do.

/// A TypeScript `export const X = { … } as const;` literal, as JSON.
///
/// A recursive-descent parser over the object literal, not a regex and not a
/// text scrape. The distinction is the whole point of this module: the
/// property that drifted furthest was TeamCity's `"maximum":
/// MAX_BUILDS_PER_CONFIG`, a Rust `const` and not a literal, so a check that
/// compared *text* could not see the one value it most needed to compare. Here
/// the Rust side is a real `serde_json::Value` -- the constant is already
/// resolved by the compiler -- and the TypeScript side is parsed into another
/// one, so the comparison is between values.
///
/// **Every failure is a panic.** Nothing in here returns an `Option` a caller
/// could quietly treat as "nothing to check": an unparseable literal, a
/// missing `as const`, a bare identifier, a duplicate key all stop the test
/// with a message naming the offending text. A drift detector that declines to
/// check is the failure this file exists to remove.
mod literal {
    use serde_json::{Map, Value};

    /// The value of `export const <name> = <literal> as const;` in `source`.
    ///
    /// # Panics
    ///
    /// If the declaration is missing, declared more than once, not followed by
    /// `as const`, or contains anything this parser cannot turn into a value.
    pub fn as_const(source: &str, name: &str) -> Value {
        let needle = format!("export const {name} =");
        let mut hits = source.match_indices(&needle);
        let (byte_start, _) = hits.next().unwrap_or_else(|| {
            panic!(
                "app/src/lib/sources/fixtures.ts declares no `{needle} …`; a fixture that was \
                 renamed or removed is a fixture nothing compares against any more"
            )
        });
        assert!(
            hits.next().is_none(),
            "app/src/lib/sources/fixtures.ts declares `{needle} …` more than once, so which one \
             is under test is undecidable"
        );

        let chars: Vec<char> = source.chars().collect();
        let start = source[..byte_start].chars().count() + needle.chars().count();
        let mut parser = Parser {
            chars: &chars,
            pos: start,
            what: name,
        };
        let value = parser.value();
        parser.keyword("as");
        parser.keyword("const");
        value
    }

    /// Every `export const <NAME>` in `source`, in declaration order.
    ///
    /// So the test can assert it compares *all* of them: a fixture added here
    /// with no adapter behind it would otherwise be a schema the form is
    /// developed against and nothing checks.
    pub fn exported_const_names(source: &str) -> Vec<String> {
        source
            .match_indices("export const ")
            .map(|(at, needle)| {
                source[at + needle.len()..]
                    .chars()
                    .take_while(|c| is_ident_continue(*c))
                    .collect()
            })
            .collect()
    }

    fn is_ident_start(c: char) -> bool {
        c.is_ascii_alphabetic() || c == '_' || c == '$'
    }

    fn is_ident_continue(c: char) -> bool {
        is_ident_start(c) || c.is_ascii_digit()
    }

    struct Parser<'a> {
        chars: &'a [char],
        pos: usize,
        what: &'a str,
    }

    impl Parser<'_> {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.pos).copied()
        }

        /// The text around the cursor, for a panic message that can be acted on.
        fn here(&self) -> String {
            let from = self.pos.saturating_sub(40);
            let to = (self.pos + 40).min(self.chars.len());
            format!(
                "{}⟪here⟫{}",
                self.chars[from..self.pos].iter().collect::<String>(),
                self.chars[self.pos..to].iter().collect::<String>()
            )
        }

        fn fail(&self, why: &str) -> ! {
            panic!(
                "app/src/lib/sources/fixtures.ts: {} in `{}`: {}",
                why,
                self.what,
                self.here()
            )
        }

        /// Whitespace, `// line` and `/* block */` comments.
        ///
        /// The fixture carries comments *inside* its object literals -- the two
        /// that explain TeamCity's `maximum` and its plain-`"string"` username
        /// -- so skipping them is not a nicety.
        fn trivia(&mut self) {
            loop {
                while self.peek().is_some_and(char::is_whitespace) {
                    self.pos += 1;
                }
                if self.peek() == Some('/') && self.chars.get(self.pos + 1) == Some(&'/') {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.pos += 1;
                    }
                    continue;
                }
                if self.peek() == Some('/') && self.chars.get(self.pos + 1) == Some(&'*') {
                    self.pos += 2;
                    loop {
                        match self.peek() {
                            None => self.fail("an unterminated /* comment"),
                            Some('*') if self.chars.get(self.pos + 1) == Some(&'/') => {
                                self.pos += 2;
                                break;
                            }
                            Some(_) => self.pos += 1,
                        }
                    }
                    continue;
                }
                return;
            }
        }

        fn expect(&mut self, c: char) {
            self.trivia();
            if self.peek() != Some(c) {
                self.fail(&format!("expected `{c}`"));
            }
            self.pos += 1;
        }

        /// A bare word that must be exactly `word` -- how `as const` is checked.
        fn keyword(&mut self, word: &str) {
            self.trivia();
            if !self.peek().is_some_and(is_ident_start) {
                self.fail(&format!("expected `{word}`"));
            }
            let got = self.identifier();
            if got != word {
                self.fail(&format!("expected `{word}`, found `{got}`"));
            }
        }

        fn identifier(&mut self) -> String {
            if !self.peek().is_some_and(is_ident_start) {
                self.fail("expected an identifier");
            }
            let from = self.pos;
            while self.peek().is_some_and(is_ident_continue) {
                self.pos += 1;
            }
            self.chars[from..self.pos].iter().collect()
        }

        fn value(&mut self) -> Value {
            self.trivia();
            match self.peek() {
                Some('{') => self.object(),
                Some('[') => self.array(),
                Some('"' | '\'') => Value::String(self.string()),
                Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
                Some('`') => self.fail(
                    "a template literal. Only literals a JSON value can hold belong in this \
                     fixture -- spell the string out",
                ),
                Some(c) if is_ident_start(c) => {
                    let word = self.identifier();
                    match word.as_str() {
                        "true" => Value::Bool(true),
                        "false" => Value::Bool(false),
                        "null" => Value::Null,
                        other => self.fail(&format!(
                            "`{other}` is not a literal. This fixture is compared value-for-value \
                             against the adapter, so every value here must be spelled out rather \
                             than referenced"
                        )),
                    }
                }
                Some(_) => self.fail("not the start of a value"),
                None => self.fail("the declaration ends before its value"),
            }
        }

        fn object(&mut self) -> Value {
            self.expect('{');
            let mut map = Map::new();
            loop {
                self.trivia();
                if self.peek() == Some('}') {
                    self.pos += 1;
                    return Value::Object(map);
                }
                let key = match self.peek() {
                    Some('"' | '\'') => self.string(),
                    Some(c) if is_ident_start(c) => self.identifier(),
                    _ => self.fail("expected a property name or `}`"),
                };
                self.expect(':');
                let value = self.value();
                if map.insert(key.clone(), value).is_some() {
                    self.fail(&format!("`{key}` is declared twice"));
                }
                self.trivia();
                match self.peek() {
                    Some(',') => self.pos += 1,
                    Some('}') => {}
                    _ => self.fail("expected `,` or `}` after a property"),
                }
            }
        }

        fn array(&mut self) -> Value {
            self.expect('[');
            let mut items = Vec::new();
            loop {
                self.trivia();
                if self.peek() == Some(']') {
                    self.pos += 1;
                    return Value::Array(items);
                }
                items.push(self.value());
                self.trivia();
                match self.peek() {
                    Some(',') => self.pos += 1,
                    Some(']') => {}
                    _ => self.fail("expected `,` or `]` after an element"),
                }
            }
        }

        fn string(&mut self) -> String {
            let quote = match self.peek() {
                Some(q @ ('"' | '\'')) => q,
                _ => self.fail("expected a string"),
            };
            self.pos += 1;
            let mut out = String::new();
            loop {
                match self.peek() {
                    None | Some('\n') => self.fail("an unterminated string"),
                    Some(c) if c == quote => {
                        self.pos += 1;
                        return out;
                    }
                    Some('\\') => {
                        self.pos += 1;
                        let escaped = match self.peek() {
                            None => self.fail("an unterminated escape"),
                            Some(c) => c,
                        };
                        self.pos += 1;
                        match escaped {
                            '"' | '\'' | '\\' | '/' => out.push(escaped),
                            'n' => out.push('\n'),
                            'r' => out.push('\r'),
                            't' => out.push('\t'),
                            'b' => out.push('\u{8}'),
                            'f' => out.push('\u{c}'),
                            'u' => {
                                let hex: String = self.chars
                                    [self.pos..(self.pos + 4).min(self.chars.len())]
                                    .iter()
                                    .collect();
                                let code = u32::from_str_radix(&hex, 16)
                                    .unwrap_or_else(|_| self.fail("a malformed \\u escape"));
                                self.pos += 4;
                                out.push(char::from_u32(code).unwrap_or_else(|| {
                                    self.fail("a \\u escape that is not a character")
                                }));
                            }
                            other => self
                                .fail(&format!("`\\{other}` is not an escape this parser knows")),
                        }
                    }
                    Some(c) => {
                        out.push(c);
                        self.pos += 1;
                    }
                }
            }
        }

        /// A number, `_` separators included -- `maximum: 10_000` is how the
        /// fixture spells the adapter's `MAX_BUILDS_PER_CONFIG = 10_000`, and
        /// reading it as `10` would be a check that agrees with the wrong thing.
        fn number(&mut self) -> Value {
            let from = self.pos;
            if self.peek() == Some('-') {
                self.pos += 1;
            }
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_digit() || c == '_' || c == '.' || c == 'e' || c == 'E')
            {
                if matches!(self.peek(), Some('e' | 'E'))
                    && matches!(self.chars.get(self.pos + 1), Some('+' | '-'))
                {
                    self.pos += 1;
                }
                self.pos += 1;
            }
            if self.peek().is_some_and(is_ident_start) {
                self.fail("a number followed by a word");
            }
            let text: String = self.chars[from..self.pos]
                .iter()
                .filter(|c| **c != '_')
                .collect();
            if let Ok(int) = text.parse::<i64>() {
                return Value::Number(int.into());
            }
            let float: f64 = text
                .parse()
                .unwrap_or_else(|_| self.fail(&format!("`{text}` is not a number")));
            Value::Number(
                serde_json::Number::from_f64(float)
                    .unwrap_or_else(|| self.fail(&format!("`{text}` is not a finite number"))),
            )
        }
    }
}

#[test]
fn an_object_literal_with_unquoted_keys_parses() {
    let value = literal::as_const(
        r#"export const X = { type: "object", n: 1 } as const;"#,
        "X",
    );
    assert_eq!(value, serde_json::json!({ "type": "object", "n": 1 }));
}

// -- the rule the comparison applies -------------------------------------------

/// Where the fixture disagrees with the adapter's real `config_schema`.
///
/// **The rule the fixture's own header states**, encoded: *a property that is
/// here is verbatim; a property may be absent, and each absence is named.* So
/// the licence to be absent is spent at exactly one level -- the `properties`
/// map -- and nowhere else, in **both** directions: a top-level key either
/// side has and the other does not is a disagreement, and only a *property*
/// may go missing. Inside a property the fixture is compared whole:
/// `type`, `title`, `description`, `default`, bounds, `items`. A looser rule
/// that compared only the keys a reader thought to list is what shipped #82,
/// because `username`'s `type` was never the key anybody thought to list.
///
/// Empty means the two agree. Every entry names a JSON path and prints both
/// sides, so the failure says which half to change rather than only that they
/// differ.
fn disagreements(fixture: &serde_json::Value, adapter: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    let (Some(fixture), Some(adapter)) = (fixture.as_object(), adapter.as_object()) else {
        found.push(format!(
            "the schema is not an object on both sides:\n  fixture: {fixture}\n  adapter: {adapter}"
        ));
        return found;
    };
    for (key, ours) in fixture {
        let Some(theirs) = adapter.get(key) else {
            found.push(format!(
                "`{key}` is declared by the fixture and the adapter's schema does not have it at \
                 all -- the fixture is stricter than the source, which is a form that rejects \
                 what the adapter accepts\n  fixture: {ours}"
            ));
            continue;
        };
        if key != "properties" {
            if ours != theirs {
                found.push(disagreement(key, ours, theirs));
            }
            continue;
        }
        // The one level the absence licence is spent at.
        let (Some(ours), Some(theirs)) = (ours.as_object(), theirs.as_object()) else {
            found.push("`properties` is not an object on both sides".to_owned());
            continue;
        };
        for (name, ours) in ours {
            match theirs.get(name) {
                None => found.push(format!(
                    "the fixture declares a property `{name}` the adapter's schema does not \
                     have\n  fixture: {ours}"
                )),
                Some(theirs) if ours != theirs => {
                    found.push(disagreement(&format!("properties.{name}"), ours, theirs));
                }
                Some(_) => {}
            }
        }
    }
    // The same rule read the other way. The licence to be absent is spent
    // inside `properties`, so a *top-level* key the adapter declares and the
    // fixture drops is not licensed: a fixture without the adapter's
    // `additionalProperties: false` is a form model that believes an unknown
    // key is accepted where the source rejects it. That is #82's direction
    // mirrored -- the fixture looser than the source rather than stricter --
    // and it is exactly as invisible, because the tests agree with the
    // fixture.
    for (key, theirs) in adapter {
        if !fixture.contains_key(key) {
            found.push(format!(
                "`{key}` is in the adapter's schema and the fixture does not have it at all -- \
                 the licence to be absent is spent inside `properties`, so a top-level key \
                 dropped here is a form looser than the source\n  adapter: {theirs}"
            ));
        }
    }
    found
}

fn disagreement(path: &str, ours: &serde_json::Value, theirs: &serde_json::Value) -> String {
    format!(
        "`{path}` disagrees\n  fixture: {}\n  adapter: {}",
        serde_json::to_string_pretty(ours).expect("a parsed value re-serializes"),
        serde_json::to_string_pretty(theirs).expect("a real schema re-serializes"),
    )
}

#[test]
fn a_property_the_fixture_leaves_out_is_licensed_and_one_it_spells_differently_is_not() {
    let adapter = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "username": { "type": ["string", "null"], "title": "Username" },
            "page_size": { "type": "integer", "maximum": 1000 }
        }
    });
    // `page_size` absent: the fixture header names its absences, and the check
    // is over what is there.
    let faithful = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": { "username": { "type": ["string", "null"], "title": "Username" } }
    });
    assert_eq!(disagreements(&faithful, &adapter), Vec::<String>::new());

    // The #82 drift: an optional string read as a plain one.
    let drifted = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": { "username": { "type": "string", "title": "Username" } }
    });
    let found = disagreements(&drifted, &adapter);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("properties.username"), "{found:#?}");
}

/// A *property* may be absent; a top-level key may not.
///
/// The licence the fixture header grants is over the `properties` map, and it
/// is spent there. `additionalProperties: false` dropped from a fixture would
/// leave the form model believing an unknown key is accepted where the adapter
/// rejects it -- #82's direction mirrored, and exactly as invisible, because
/// every frontend test would go on agreeing with the fixture. This is the
/// witness that the licence stops at the one level the doc comment says it
/// does.
#[test]
fn a_top_level_key_the_fixture_drops_is_not_licensed() {
    let adapter = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    });
    let dropped = serde_json::json!({ "type": "object", "properties": {} });
    let found = disagreements(&dropped, &adapter);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("additionalProperties"), "{found:#?}");
}

// -- the check itself ----------------------------------------------------------

const FIXTURES: &str = include_str!("../../../app/src/lib/sources/fixtures.ts");

/// Each fixture const and the adapter kind it transcribes.
///
/// The adapter side is looked up in `Registry::builtin()` rather than named
/// here, so this table carries only the one fact it has to: which fixture is
/// about which kind. A kind that is not compiled in fails the lookup.
const MIRRORED: &[(&str, &str)] = &[
    ("JIRA_SCHEMA", "jira"),
    ("GITEA_SCHEMA", "gitea"),
    ("TEAMCITY_SCHEMA", "teamcity"),
    ("EMPTY_SCHEMA", "mock"),
];

fn adapter_schema(kind: &str) -> serde_json::Value {
    use knobas_app::sources::Registry;
    use knobas_sync::scheduler::AdapterRegistry;
    Registry::builtin()
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == kind)
        .unwrap_or_else(|| panic!("no adapter of kind `{kind}` is compiled in"))
        .config_schema
}

/// The transcription in `fixtures.ts` says what the adapters say.
///
/// This is the check #124 asked for and #133 could not build from inside
/// `app/`: the adapter's schema here is the **value** the adapter really
/// returns, so `"maximum": MAX_BUILDS_PER_CONFIG` arrives as `10000` with the
/// constant already resolved, and the comparison is between two
/// `serde_json::Value`s rather than between two pieces of text.
///
/// It therefore tells drift from a legitimate change, which a pinned number
/// cannot: change an adapter's schema and update the fixture to match and this
/// stays green; change one without the other and it goes red.
#[test]
fn every_fixture_schema_says_what_its_adapter_says() {
    let mut failures = Vec::new();
    for (name, kind) in MIRRORED {
        let found = disagreements(&literal::as_const(FIXTURES, name), &adapter_schema(kind));
        if !found.is_empty() {
            failures.push(format!("{name} ({kind}):\n{}", found.join("\n")));
        }
    }
    assert!(
        failures.is_empty(),
        "app/src/lib/sources/fixtures.ts has drifted from the adapters it transcribes. The \
         fixture is the Add-source form's whole idea of what a config schema looks like, so a \
         disagreement here is a control rendered wrong in the window and right in every test \
         (#82, #110, #124).\n\n{}",
        failures.join("\n\n")
    );
}

/// Every fixture in the file is compared -- none is merely exported.
///
/// A fixture added with no entry in [`MIRRORED`] would be a schema the form is
/// developed and tested against that nothing pins, which is the state this
/// whole file exists to end.
#[test]
fn every_exported_fixture_is_compared_against_an_adapter() {
    let mut exported = literal::exported_const_names(FIXTURES);
    exported.sort();
    let mut compared: Vec<String> = MIRRORED
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    compared.sort();
    assert_eq!(
        exported, compared,
        "app/src/lib/sources/fixtures.ts exports a schema this test does not compare against any \
         adapter"
    );
}

// -- the parser refuses rather than skipping ------------------------------------

/// The property that drifted furthest, in the shape a text scrape hits it in.
///
/// `"maximum": MAX_BUILDS_PER_CONFIG` is a `const` on the adapter's side, and
/// the temptation for a checker that meets a name it cannot resolve is to move
/// on. Moving on is how a drift detector reports success over the one property
/// it could not read, so this is a panic.
#[test]
#[should_panic(expected = "is not a literal")]
fn a_name_the_fixture_cannot_resolve_stops_the_test() {
    literal::as_const(
        "export const X = { maximum: MAX_BUILDS_PER_CONFIG } as const;",
        "X",
    );
}

/// A fixture that stops being `as const` is a fixture whose type widens --
/// and, here, one whose end this parser never found.
#[test]
#[should_panic(expected = "expected `as`")]
fn a_literal_that_is_not_as_const_stops_the_test() {
    literal::as_const(r#"export const X = { type: "object" };"#, "X");
}

/// A fixture that was renamed or deleted must not read as "nothing to compare".
#[test]
#[should_panic(expected = "declares no")]
fn a_fixture_that_is_gone_stops_the_test() {
    literal::as_const(r#"export const Y = { type: "object" } as const;"#, "X");
}
