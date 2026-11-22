//! **The Pleasant adapter against the real free server** (ADR-0056, ADR-0013:
//! the real instance is the witness). Pleasant 9.5.4 in the disposable
//! Windows VM on the WSL box, seeded with `testenv/pleasant`'s `plan(20)`:
//! three users, twenty entries, the comment-required `Payments/Prod`, the
//! request-only `Payments/Prod/PSP` (`testenv/pleasant/README.md`).
//!
//! What it proves, each against the server's own answer:
//! - a sync is one sign-in and one tree, and a second within the token's 12
//!   hours is the tree alone: the audit promise, since only the sign-in
//!   writes a line (`S/pleasant-server.md` §6.1);
//! - the mirror holds the twenty, their folders and codes, and no key;
//! - one password read; the comment-required one refused without a comment
//!   and read with one; the request-only one refused in the server's words;
//! - an attachment read live;
//! - the writes: a create refused at the free edition's cap, an edit and a
//!   change of password each read back, **and each put back**, so the server
//!   stays as seeded.
//!
//! The audit lines the run writes are read by the run's harness from the
//! admin's Logged Events (the grid `testenv/pleasant`'s `audit.mjs` reads),
//! before and after; this file counts the requests that can write them.
//!
//! `#[ignore]`d: it needs the VM up and its users' credentials.
//!
//! ```text
//! export KNOBAS_PLEASANT_URL=https://127.0.0.1:24843 \
//!   KNOBAS_PLEASANT_READER_USER=… KNOBAS_PLEASANT_READER_PASSWORD=… \
//!   KNOBAS_PLEASANT_WRITER_USER=… KNOBAS_PLEASANT_WRITER_PASSWORD=…
//! cargo test -p knobas-source-pleasant --test live_pleasant -- --ignored --nocapture --test-threads=1
//! ```
//!
//! The VM's own certificate names no subjectAltName, so knobas cannot verify
//! it by name; the run reaches it through a TLS front with a loopback leaf
//! from the testenv's constrained root, trusted through `SSL_CERT_FILE`, and
//! the front pins the VM's certificate. Nothing turns verification off.

use std::collections::BTreeMap;

use knobas_source::contract::KeySink;
use knobas_source::instance::SourceInstance;
use knobas_source::passwords::{
    EntryRecord, SecretAnswer, SecretAsk, SecretRef, SecretText, Write,
};
use knobas_source::{AuthMethod, Source};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: see this file's header"))
}

fn source(user: &str, password: &str) -> Box<dyn Source> {
    knobas_source_pleasant::build(SourceInstance {
        id: "pl".into(),
        kind: knobas_source_pleasant::ADAPTER_KIND.into(),
        display_name: "Team passwords".into(),
        base_url: env("KNOBAS_PLEASANT_URL"),
        auth: Some(AuthMethod::UserPassword),
        secret: Some(password.into()),
        account: None,
        config: serde_json::json!({ "username": user }),
    })
    .unwrap()
}

/// Every entry by title: its key and record.
fn entries(sink: &KeySink) -> BTreeMap<String, (String, EntryRecord)> {
    sink.items
        .iter()
        .filter(|i| i.kind == knobas_source::passwords::ENTRY_KIND)
        .map(|i| {
            (
                i.title.clone(),
                (
                    i.entity.to_string(),
                    serde_json::from_value(i.payload.clone()).unwrap(),
                ),
            )
        })
        .collect()
}

async fn read(
    s: &dyn Source,
    entity: &str,
    folder: Option<&String>,
    comment: Option<&str>,
) -> SecretAnswer {
    s.read_secret(&SecretAsk {
        entity: entity.to_owned(),
        which: SecretRef::Password,
        comment: comment.map(str::to_owned),
        folder: folder.cloned(),
    })
    .await
    .unwrap()
}

fn value(answer: SecretAnswer) -> String {
    match answer {
        SecretAnswer::Value(v) => v.expose().to_owned(),
        other => panic!("no value: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs the Pleasant VM and its users: see the header"]
async fn the_reader_syncs_reads_and_is_refused_as_the_real_server_says() {
    let reader = source(
        &env("KNOBAS_PLEASANT_READER_USER"),
        &env("KNOBAS_PLEASANT_READER_PASSWORD"),
    );
    let mut sink = KeySink::default();
    let (cursor, sent) = knobas_http::counted::counted(reader.sync(None, &mut sink)).await;
    cursor.unwrap();
    assert_eq!(sent, 2, "a first sync: one sign-in and one tree");
    let all = entries(&sink);
    println!("synced {} entries, {} keys", all.len(), sink.keys.len());
    assert_eq!(all.len(), 20, "{:?}", all.keys().collect::<Vec<_>>());
    for item in &sink.items {
        assert!(
            !item.payload.to_string().contains("otpauth://"),
            "{}",
            item.title
        );
    }
    let coded = all.values().filter(|(_, r)| r.otp.is_some()).count();
    assert_eq!(sink.keys.len(), coded);
    assert!(coded >= 3, "{coded}");
    // The request-only folder's entries carry no key and no read.
    let psp = &all["PSP dashboard, finance sign-in"].1;
    assert!(psp.otp.is_none() && !psp.can_read);

    // A second sync within the token's life: the tree alone, no sign-in.
    let mut again = KeySink::default();
    let (_, sent) = knobas_http::counted::counted(reader.sync(None, &mut again)).await;
    assert_eq!(sent, 1, "a second sync reuses the token");

    // One password; the comment-required one, refused then read with one.
    let (teamcity, rec) = &all["TeamCity service account"];
    let (answer, sent) =
        knobas_http::counted::counted(read(reader.as_ref(), teamcity, rec.folder.as_ref(), None))
            .await;
    assert!(matches!(answer, SecretAnswer::Value(_)));
    assert_eq!(sent, 1, "a read is one request");
    let (payout, rec) = &all["payout-service database user"];
    assert!(rec.comment.read);
    assert!(matches!(
        read(reader.as_ref(), payout, rec.folder.as_ref(), None).await,
        SecretAnswer::CommentRequired { .. }
    ));
    assert!(matches!(
        read(
            reader.as_ref(),
            payout,
            rec.folder.as_ref(),
            Some("knobas live suite")
        )
        .await,
        SecretAnswer::Value(_)
    ));
    let (live, rec) = &all["PSP live secret key"];
    match read(reader.as_ref(), live, rec.folder.as_ref(), None).await {
        SecretAnswer::Refused { reason } => {
            assert!(
                reason.contains("does not have permission to view the password"),
                "{reason}"
            );
        }
        other => panic!("{other:?}"),
    }

    // An attachment, read live.
    let (with_file, rec) = all
        .iter()
        .find_map(|(_, (id, r))| match &r.attachments {
            knobas_source::passwords::Attachments::Listed(files) => Some((id, files[0].clone())),
            _ => None,
        })
        .map(|(id, f)| (id.clone(), f))
        .expect("an entry with an attachment");
    let read = reader
        .attachment_bytes(&with_file, &rec.id, 1 << 20)
        .await
        .unwrap();
    match read {
        Some(knobas_source::view::FileBytes::Read(bytes)) => {
            assert_eq!(Some(bytes.len() as u64), rec.size, "{}", rec.name);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs the Pleasant VM and its users: see the header"]
async fn the_writer_is_refused_at_the_cap_and_an_edit_and_a_new_password_read_back_and_are_put_back()
 {
    let writer = source(
        &env("KNOBAS_PLEASANT_WRITER_USER"),
        &env("KNOBAS_PLEASANT_WRITER_PASSWORD"),
    );
    let mut sink = KeySink::default();
    writer.sync(None, &mut sink).await.unwrap();
    let all = entries(&sink);
    let (wifi, rec) = all["Guest Wi-Fi"].clone();
    let folder = rec.folder.clone();

    // The free edition holds twenty entries: a twenty-first is refused, in
    // the server's words, and nothing is made.
    let refused = writer
        .passwords_write(Write::CreateEntry {
            source: "pl".into(),
            folder: folder.clone(),
            title: "knobas live suite".into(),
            username: None,
            address: None,
            notes: None,
            expires: None,
            password: SecretText::new("x"),
            otp_key: None,
            comment: None,
        })
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("20 entry limit"), "{refused}");

    // An edit, read back, and put back.
    let was = rec.username.clone();
    let edited = writer
        .passwords_write(Write::EditEntry {
            entity: wifi.clone(),
            title: None,
            username: Some(Some("knobas-live-guest".into())),
            address: None,
            notes: None,
            expires: None,
            folder: None,
            otp_key: None,
            comment: None,
            listed_in: folder.clone(),
        })
        .await
        .unwrap();
    let back: EntryRecord =
        serde_json::from_value(edited.item.expect("read back").payload).unwrap();
    assert_eq!(back.username.as_deref(), Some("knobas-live-guest"));
    writer
        .passwords_write(Write::EditEntry {
            entity: wifi.clone(),
            title: None,
            username: Some(was.clone()),
            address: None,
            notes: None,
            expires: None,
            folder: None,
            otp_key: None,
            comment: None,
            listed_in: folder.clone(),
        })
        .await
        .unwrap();

    // A new password, read back, and the seeded one put back.
    let seeded = value(read(writer.as_ref(), &wifi, folder.as_ref(), None).await);
    let new = format!("knobas-live-{}", std::process::id());
    writer
        .passwords_write(Write::SetPassword {
            entity: wifi.clone(),
            password: SecretText::new(new.clone()),
            comment: None,
            listed_in: folder.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        value(read(writer.as_ref(), &wifi, folder.as_ref(), None).await),
        new
    );
    writer
        .passwords_write(Write::SetPassword {
            entity: wifi.clone(),
            password: SecretText::new(seeded.clone()),
            comment: None,
            listed_in: folder.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        value(read(writer.as_ref(), &wifi, folder.as_ref(), None).await),
        seeded
    );
    let mut after = KeySink::default();
    writer.sync(None, &mut after).await.unwrap();
    assert_eq!(
        entries(&after)["Guest Wi-Fi"].1.username,
        was,
        "the username was not put back"
    );
}
