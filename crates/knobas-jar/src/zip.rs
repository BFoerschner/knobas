//! A zip archive read from its central directory (APPNOTE 6.3.10), and one entry at a time.
//!
//! Only the end record and the central directory are read to list an archive: a 200 MB fat JAR is listed by reading
//! its last few hundred kilobytes. An entry's bytes are read only when it is opened, through its local header.
//! Everything an archive declares is checked before it is believed: counts, offsets and sizes against the archive's
//! length, a declared size against the cap before inflating, the inflated bytes against the declared size and CRC as
//! they come. The archive may sit at any offset inside a file (`base`): a nested JAR stored in a fat JAR is read in
//! place, and a JAR with a launch script before it (Spring Boot's "fully executable" jars) is read past the script.

use std::io::{Read, Seek, SeekFrom};

/// Most entries an archive may list.
pub const MAX_ENTRIES: usize = 500_000;
/// Most bytes a central directory may take.
pub const MAX_DIRECTORY: u64 = 256 * 1024 * 1024;
/// An entry inflating to more than this many times its stored size, and to more than [`BOMB_FLOOR`], is refused.
pub const MAX_RATIO: u64 = 1_000;
pub const BOMB_FLOOR: u64 = 16 * 1024 * 1024;

const EOCD: u32 = 0x0605_4b50;
const EOCD64_LOCATOR: u32 = 0x0706_4b50;
const EOCD64: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

/// Why an archive or an entry was not read.
#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    #[error("not a zip archive (no end of central directory)")]
    NotZip,
    #[error("the archive is broken: {0}")]
    Broken(String),
    #[error("the archive lists {0} entries; knobas reads at most {MAX_ENTRIES}")]
    TooMany(usize),
    #[error("{name} is {size} bytes unpacked; knobas reads an entry up to {cap}")]
    TooLarge { name: String, size: u64, cap: u64 },
    #[error("{0} unpacks to over {MAX_RATIO} times its size: refused as a zip bomb")]
    Bomb(String),
    #[error("{0} is encrypted")]
    Encrypted(String),
    #[error("{name} is packed with method {method}, which knobas does not read")]
    Method { name: String, method: u16 },
    #[error("{0}'s name leaves the archive (an absolute path or `..`): not opened")]
    UnsafeName(String),
    #[error("{0} does not match its checksum: the archive is corrupt")]
    Crc(String),
    #[error("reading the archive: {0}")]
    Io(#[from] std::io::Error),
}

/// One entry of the central directory.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub csize: u64,
    pub size: u64,
    /// The local header's offset, from the archive's start (the prefix already added).
    pub local: u64,
    /// The name leaves the archive: absolute, a drive, a `..` segment, a backslash or a control character.
    pub unsafe_name: bool,
}

impl Entry {
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
    #[must_use]
    pub fn encrypted(&self) -> bool {
        self.flags & 1 != 0
    }
    /// Packed in a way this reader unpacks: stored or deflated, not encrypted.
    #[must_use]
    pub fn readable(&self) -> bool {
        !self.encrypted() && matches!(self.method, 0 | 8)
    }
}

/// An archive's listing, and where in its file it sits.
#[derive(Debug, Clone)]
pub struct Directory {
    pub entries: Vec<Entry>,
    /// The archive's first byte in its file.
    pub base: u64,
    pub len: u64,
    /// Bytes before the first local header that the offsets do not count (a launch script).
    pub prefix: u64,
}

/// Whether a name would leave a folder it were unpacked into.
#[must_use]
pub fn unsafe_name(name: &str) -> bool {
    name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
        || name.as_bytes().get(1) == Some(&b':')
        || name.split('/').any(|part| part == "..")
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap_or([0; 8]))
}

fn read_at<R: Read + Seek + ?Sized>(r: &mut R, at: u64, buf: &mut [u8]) -> Result<(), ZipError> {
    r.seek(SeekFrom::Start(at))?;
    r.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ZipError::Broken("it ends early".into())
        } else {
            e.into()
        }
    })
}

fn broken(why: &str) -> ZipError {
    ZipError::Broken(why.to_owned())
}

/// Lists the archive of `len` bytes at `base` in `r`.
///
/// # Errors
///
/// [`ZipError::NotZip`] where no end record is found, [`ZipError::Broken`] where a record points outside the archive
/// or is not what it says, [`ZipError::TooMany`] past [`MAX_ENTRIES`].
#[allow(clippy::too_many_lines)]
pub fn read_directory<R: Read + Seek + ?Sized>(
    r: &mut R,
    base: u64,
    len: u64,
) -> Result<Directory, ZipError> {
    if len < 22 {
        return Err(ZipError::NotZip);
    }
    // The end record is in the last 22 + 65,535 bytes (its comment is at most that long).
    let tail_len = len.min(22 + 65_535);
    let mut tail = vec![0u8; usize::try_from(tail_len).map_err(|_| ZipError::NotZip)?];
    let tail_at = len - tail_len;
    read_at(r, base + tail_at, &mut tail)?;
    let found = (0..=tail.len() - 22).rev().find(|&i| {
        le32(&tail, i) == EOCD && {
            // The comment must reach exactly to the end: a false match inside the comment or data does not.
            let comment = usize::from(le16(&tail, i + 20));
            i + 22 + comment == tail.len()
        }
    });
    // A comment that does not reach the end (some writers pad) is taken as a second chance.
    let at = found
        .or_else(|| {
            (0..=tail.len() - 22)
                .rev()
                .find(|&i| le32(&tail, i) == EOCD)
        })
        .ok_or(ZipError::NotZip)?;
    let eocd_pos = tail_at + at as u64;
    let e = &tail[at..at + 22];
    let mut count = u64::from(le16(e, 10));
    let mut cd_size = u64::from(le32(e, 12));
    let mut cd_offset = u64::from(le32(e, 16));
    let mut cd_end = eocd_pos;

    if (count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_offset == 0xFFFF_FFFF) && eocd_pos >= 20 {
        let mut loc = [0u8; 20];
        read_at(r, base + eocd_pos - 20, &mut loc)?;
        if le32(&loc, 0) == EOCD64_LOCATOR {
            let z64_offset = le64(&loc, 8);
            // The zip64 end record sits just before its locator; its own offset may not count a prefix.
            let z64_pos = eocd_pos
                .checked_sub(20 + 56)
                .ok_or_else(|| broken("the zip64 end record is missing"))?;
            let mut z = [0u8; 56];
            read_at(r, base + z64_pos, &mut z)?;
            if le32(&z, 0) != EOCD64 {
                let mut z2 = [0u8; 56];
                if z64_offset + 56 > len {
                    return Err(broken("the zip64 end record is missing"));
                }
                read_at(r, base + z64_offset, &mut z2)?;
                if le32(&z2, 0) != EOCD64 {
                    return Err(broken("the zip64 end record is missing"));
                }
                z = z2;
            }
            count = le64(&z, 32);
            cd_size = le64(&z, 40);
            cd_offset = le64(&z, 48);
            cd_end = z64_pos;
        }
    }

    if count as usize > MAX_ENTRIES || count > (len / 46) + 1 {
        return Err(if count as usize > MAX_ENTRIES {
            ZipError::TooMany(count as usize)
        } else {
            broken("it lists more entries than it can hold")
        });
    }
    if cd_size > MAX_DIRECTORY || cd_size > cd_end {
        return Err(broken("the central directory is larger than the archive"));
    }
    // Where the directory really starts: just before the end record. Anything between that and the offset the
    // archive says is a prefix (a launch script), which every local offset is shifted by.
    let cd_start = cd_end - cd_size;
    let prefix = cd_start
        .checked_sub(cd_offset)
        .ok_or_else(|| broken("the central directory's offset is past it"))?;

    let mut cd = vec![0u8; usize::try_from(cd_size).map_err(|_| broken("too large"))?];
    read_at(r, base + cd_start, &mut cd)?;
    let mut entries = Vec::with_capacity(count as usize);
    let mut at = 0usize;
    while at + 46 <= cd.len() && entries.len() < MAX_ENTRIES {
        if le32(&cd, at) != CENTRAL {
            break;
        }
        let flags = le16(&cd, at + 8);
        let method = le16(&cd, at + 10);
        let crc = le32(&cd, at + 16);
        let mut csize = u64::from(le32(&cd, at + 20));
        let mut size = u64::from(le32(&cd, at + 24));
        let name_len = usize::from(le16(&cd, at + 28));
        let extra_len = usize::from(le16(&cd, at + 30));
        let comment_len = usize::from(le16(&cd, at + 32));
        let mut local = u64::from(le32(&cd, at + 42));
        let name_at = at + 46;
        let extra_at = name_at + name_len;
        let next = extra_at + extra_len + comment_len;
        if next > cd.len() {
            return Err(broken("an entry runs past the central directory"));
        }
        let raw = &cd[name_at..extra_at];
        let name = String::from_utf8_lossy(raw).into_owned();
        // Zip64's extra field carries the sizes and offset that did not fit, in this order (APPNOTE 4.5.3).
        let mut x = extra_at;
        while x + 4 <= extra_at + extra_len {
            let id = le16(&cd, x);
            let n = usize::from(le16(&cd, x + 2));
            let body = x + 4;
            if body + n > extra_at + extra_len {
                break;
            }
            if id == 1 {
                let mut p = body;
                for field in [&mut size, &mut csize, &mut local] {
                    if *field == 0xFFFF_FFFF && p + 8 <= body + n {
                        *field = le64(&cd, p);
                        p += 8;
                    }
                }
            }
            x = body + n;
        }
        let local = local
            .checked_add(prefix)
            .ok_or_else(|| broken("an entry's offset overflows"))?;
        if local >= cd_start || csize > len {
            return Err(broken(&format!("{name} points outside the archive")));
        }
        entries.push(Entry {
            unsafe_name: unsafe_name(&name),
            name,
            method,
            flags,
            crc,
            csize,
            size,
            local,
        });
        at = next;
    }
    if entries.len() as u64 != count && count != 0xFFFF {
        return Err(broken(&format!(
            "it says {count} entries and lists {}",
            entries.len()
        )));
    }
    Ok(Directory {
        entries,
        base,
        len,
        prefix,
    })
}

/// Where an entry's packed bytes start, from its local header.
///
/// # Errors
///
/// [`ZipError::Broken`] where no local header is at its offset or its data runs past the archive.
pub fn data_start<R: Read + Seek + ?Sized>(
    r: &mut R,
    dir: &Directory,
    entry: &Entry,
) -> Result<u64, ZipError> {
    let mut head = [0u8; 30];
    if entry.local + 30 > dir.len {
        return Err(broken(&format!(
            "{} points outside the archive",
            entry.name
        )));
    }
    read_at(r, dir.base + entry.local, &mut head)?;
    if le32(&head, 0) != LOCAL {
        return Err(broken(&format!("{} has no local header", entry.name)));
    }
    let start = entry.local + 30 + u64::from(le16(&head, 26)) + u64::from(le16(&head, 28));
    if start + entry.csize > dir.len {
        return Err(broken(&format!("{} runs past the archive", entry.name)));
    }
    Ok(start)
}

/// Refuses an entry knobas would not unpack: an unsafe name, encryption, an unknown method, a size past `cap`, a
/// ratio of a bomb.
///
/// # Errors
///
/// The [`ZipError`] that says why.
pub fn check(entry: &Entry, cap: u64) -> Result<(), ZipError> {
    if entry.unsafe_name {
        return Err(ZipError::UnsafeName(entry.name.clone()));
    }
    if entry.encrypted() {
        return Err(ZipError::Encrypted(entry.name.clone()));
    }
    if !matches!(entry.method, 0 | 8) {
        return Err(ZipError::Method {
            name: entry.name.clone(),
            method: entry.method,
        });
    }
    if entry.size > cap {
        return Err(ZipError::TooLarge {
            name: entry.name.clone(),
            size: entry.size,
            cap,
        });
    }
    if entry.size > BOMB_FLOOR && entry.size / entry.csize.max(1) > MAX_RATIO {
        return Err(ZipError::Bomb(entry.name.clone()));
    }
    if entry.method == 0 && entry.size != entry.csize {
        return Err(broken(&format!(
            "{} is stored, and its two sizes differ",
            entry.name
        )));
    }
    Ok(())
}

/// Copies an entry's unpacked bytes into `out`, at most `cap` of them, checking its size and CRC as they come.
///
/// # Errors
///
/// As [`check`], and [`ZipError::Crc`] or [`ZipError::Broken`] where the bytes are not what the directory says.
pub fn copy_entry<R: Read + Seek + ?Sized, W: std::io::Write>(
    r: &mut R,
    dir: &Directory,
    entry: &Entry,
    cap: u64,
    out: &mut W,
) -> Result<u64, ZipError> {
    check(entry, cap)?;
    let start = data_start(r, dir, entry)?;
    r.seek(SeekFrom::Start(dir.base + start))?;
    let packed = (&mut *r).take(entry.csize);
    let mut reader: Box<dyn Read + '_> = if entry.method == 8 {
        Box::new(flate2::read::DeflateDecoder::new(packed))
    } else {
        Box::new(packed)
    };
    let mut crc = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(e)
                if e.kind() == std::io::ErrorKind::InvalidInput
                    || e.kind() == std::io::ErrorKind::InvalidData =>
            {
                return Err(broken(&format!("{} does not inflate", entry.name)));
            }
            Err(e) => return Err(e.into()),
        };
        if n == 0 {
            break;
        }
        total += n as u64;
        // An entry that inflates past what the directory said is lying about its size: stop at once.
        if total > entry.size {
            return Err(ZipError::Bomb(entry.name.clone()));
        }
        crc.update(&buf[..n]);
        out.write_all(&buf[..n])?;
    }
    if total != entry.size {
        return Err(broken(&format!(
            "{} is shorter than the directory says",
            entry.name
        )));
    }
    if crc.finalize() != entry.crc {
        return Err(ZipError::Crc(entry.name.clone()));
    }
    Ok(total)
}

/// An entry's unpacked bytes, at most `cap`.
///
/// # Errors
///
/// As [`copy_entry`].
pub fn read_entry<R: Read + Seek + ?Sized>(
    r: &mut R,
    dir: &Directory,
    entry: &Entry,
    cap: u64,
) -> Result<Vec<u8>, ZipError> {
    check(entry, cap)?;
    let mut out = Vec::with_capacity(usize::try_from(entry.size.min(cap)).unwrap_or(0));
    copy_entry(r, dir, entry, cap, &mut out)?;
    Ok(out)
}

/// The first bytes of an entry, unpacked, without its CRC check: a class's version, a file's sniff.
///
/// # Errors
///
/// As [`check`], and [`ZipError::Broken`] where its bytes do not inflate.
pub fn read_head<R: Read + Seek + ?Sized>(
    r: &mut R,
    dir: &Directory,
    entry: &Entry,
    n: usize,
) -> Result<Vec<u8>, ZipError> {
    check(entry, u64::MAX)?;
    let start = data_start(r, dir, entry)?;
    r.seek(SeekFrom::Start(dir.base + start))?;
    let packed = (&mut *r).take(entry.csize);
    let mut reader: Box<dyn Read + '_> = if entry.method == 8 {
        Box::new(flate2::read::DeflateDecoder::new(packed))
    } else {
        Box::new(packed)
    };
    let mut out = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = reader
            .read(&mut out[got..])
            .map_err(|_| broken(&format!("{} does not inflate", entry.name)))?;
        if k == 0 {
            break;
        }
        got += k;
    }
    out.truncate(got);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_leave_the_archive_are_unsafe() {
        for bad in [
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a\\b",
            "C:/x",
            "a\u{0}b",
            "",
        ] {
            assert!(unsafe_name(bad), "{bad:?}");
        }
        for good in ["a/b.class", "META-INF/MANIFEST.MF", "x..y/z", "a/"] {
            assert!(!unsafe_name(good), "{good:?}");
        }
    }

    #[test]
    fn bytes_that_are_no_archive_are_not_zip() {
        let mut c = std::io::Cursor::new(b"hello, this is not a zip archive at all".to_vec());
        assert!(matches!(
            read_directory(&mut c, 0, 39),
            Err(ZipError::NotZip)
        ));
        let mut c = std::io::Cursor::new(b"short".to_vec());
        assert!(matches!(
            read_directory(&mut c, 0, 5),
            Err(ZipError::NotZip)
        ));
    }
}
