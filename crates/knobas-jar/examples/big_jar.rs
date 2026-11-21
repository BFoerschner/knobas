//! The JAR view's backend at scale (ux19): a Spring Boot fat JAR of about 200 MB and 10,000 entries, made here, then
//! timed as the app reads it: the listing from memory (what `archive_open` answers from), an entry, a nested JAR read
//! in place and its listing, and a class outlined. Medians of five.
//!
//!   cargo run --release -p knobas-jar --example big_jar
use std::io::{Cursor, Write};
use std::time::Instant;

fn zip(items: &[(String, Vec<u8>, bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes, deflate) in items {
        let packed = if *deflate {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(bytes).unwrap();
            e.finish().unwrap()
        } else {
            bytes.clone()
        };
        let crc = crc32fast::hash(bytes);
        let method: u16 = if *deflate { 8 } else { 0 };
        let local = out.len() as u32;
        let n = name.as_bytes();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 8]);
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(n);
        out.extend_from_slice(&packed);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&[20, 0, 20, 0, 0, 8]);
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        central.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        central.extend_from_slice(&(n.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&local.to_le_bytes());
        central.extend_from_slice(n);
    }
    let at = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

/// Bytes that do not pack: a library JAR's bulk.
fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn median(mut f: impl FnMut()) -> f64 {
    let mut times: Vec<f64> = (0..5)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[2]
}

fn main() {
    let class = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/Shelf.class"
    ))
    .unwrap();
    let mut items: Vec<(String, Vec<u8>, bool)> = vec![(
        "META-INF/MANIFEST.MF".into(),
        b"Manifest-Version: 1.0\r\nStart-Class: com.x.App\r\n\r\n".to_vec(),
        true,
    )];
    for i in 0..9_650 {
        items.push((
            format!(
                "BOOT-INF/classes/com/tidewater/payout/p{}/Payout{i}.class",
                i % 60
            ),
            class.clone(),
            true,
        ));
    }
    // 150 libraries, stored as Spring Boot stores them, 1.3 MB each: the bulk of the 200 MB.
    for i in 0..150 {
        let lib = zip(&[
            (
                "META-INF/MANIFEST.MF".into(),
                b"Manifest-Version: 1.0\r\n\r\n".to_vec(),
                true,
            ),
            ("org/lib/Thing.class".into(), class.clone(), true),
            ("org/lib/blob.bin".into(), noise(1_300_000, i + 1), false),
        ]);
        items.push((format!("BOOT-INF/lib/lib{i}-1.{i}.0.jar"), lib, false));
    }
    let jar = zip(&items);
    println!(
        "archive: {:.1} MB, {} entries",
        jar.len() as f64 / 1e6,
        items.len()
    );

    let list = median(|| {
        let mut c = Cursor::new(&jar[..]);
        let dir = knobas_jar::zip::read_directory(&mut c, 0, jar.len() as u64).unwrap();
        let ix = knobas_jar::index(&mut c, &dir, "payout-0.0.1.jar");
        assert_eq!(ix.libs.len(), 150);
    });
    println!("listing (directory, manifest, release sample): {list:.1} ms");
    let index_json = {
        let mut c = Cursor::new(&jar[..]);
        let dir = knobas_jar::zip::read_directory(&mut c, 0, jar.len() as u64).unwrap();
        serde_json_len(&knobas_jar::index(&mut c, &dir, "payout-0.0.1.jar"))
    };
    println!(
        "listing as it crosses the bridge: {:.0} KB of JSON",
        index_json as f64 / 1000.0
    );

    let mut c = Cursor::new(&jar[..]);
    let dir = knobas_jar::zip::read_directory(&mut c, 0, jar.len() as u64).unwrap();
    let entry = knobas_jar::entry(
        &dir,
        "BOOT-INF/classes/com/tidewater/payout/p7/Payout4567.class",
    )
    .unwrap()
    .clone();
    let read = median(|| {
        let bytes = knobas_jar::zip::read_entry(&mut c, &dir, &entry, 1 << 26).unwrap();
        knobas_jar::class::outline(&bytes).unwrap();
    });
    println!("a class read and outlined: {read:.2} ms");
    let lib = knobas_jar::entry(&dir, "BOOT-INF/lib/lib77-1.77.0.jar")
        .unwrap()
        .clone();
    let nested = median(|| {
        let knobas_jar::Inner::InPlace { base, len } =
            knobas_jar::inner(&mut c, &dir, &lib).unwrap()
        else {
            panic!()
        };
        let d = knobas_jar::zip::read_directory(&mut c, base, len).unwrap();
        knobas_jar::index(&mut c, &d, "lib77-1.77.0.jar");
    });
    println!("a nested JAR listed in place: {nested:.2} ms");
    let spool = std::env::temp_dir().join(format!("knobas-big-jar-{}.jar", std::process::id()));
    let write = median(|| std::fs::write(&spool, &jar).unwrap());
    let _ = std::fs::remove_file(&spool);
    println!("the archive spooled to disk behind the answer: {write:.0} ms");
}

fn serde_json_len(ix: &knobas_jar::ArchiveIndex) -> usize {
    // The names dominate; the rest is three arrays of numbers.
    ix.names.len()
        + ix.sizes
            .iter()
            .chain(&ix.packed_sizes)
            .map(|n| n.to_string().len() + 1)
            .sum::<usize>()
        + ix.flags.len() * 2
}
