//! A class file read as `javap` would outline it, without Java (JVMS chapter 4).
//!
//! Reads the constant pool, the class's own flags, names, fields and methods and the attributes that change what a
//! reader sees: `Signature` (generics), `Exceptions`, `ConstantValue`, `RuntimeVisibleAnnotations` and
//! `RuntimeInvisibleAnnotations`, `MethodParameters` (names where the compiler kept them), `Record`,
//! `PermittedSubclasses`, `InnerClasses` (a nested class's own flags), `SourceFile`, `Deprecated` and `Module`.
//! Bytecode is skipped, never interpreted. Every count and index is checked against the bytes, so a broken or hostile
//! class file is an error, never a panic or a long loop.

use serde::Serialize;

/// The outline of one class, interface, enum, record, annotation or module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ClassOutline {
    /// The class file's major version.
    pub major: u16,
    pub minor: u16,
    /// The Java release the version is (`Java 17`, `Java 1.4`), with `preview` where the minor says so.
    pub release: String,
    /// `class`, `interface`, `enum`, `record`, `@interface` or `module`.
    pub kind: String,
    /// The modifiers before the kind (`public abstract sealed`).
    pub modifiers: String,
    /// The binary name, dotted (`com.example.Foo$Bar`).
    pub name: String,
    /// Type parameters as written (`<T extends Comparable<T>>`), or empty.
    pub type_params: String,
    /// The superclass, `null` for `Object`, an interface, an enum's `Enum` and a record's `Record`.
    pub extends: Option<String>,
    /// The interfaces implemented (an interface's: extended).
    pub implements: Vec<String>,
    /// What a `sealed` class permits.
    pub permits: Vec<String>,
    pub annotations: Vec<String>,
    /// A record's components (`String name`).
    pub components: Vec<String>,
    pub fields: Vec<Member>,
    pub methods: Vec<Member>,
    /// Members the compiler made (bridges, lambdas' bodies, `access$000`), not listed.
    pub synthetic: u32,
    pub source_file: Option<String>,
    pub deprecated: bool,
    /// A module's `requires`, `exports` and `opens`, one line each.
    pub module: Vec<String>,
}

/// One field or method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Member {
    pub annotations: Vec<String>,
    /// `public static final`, in the order the language writes them.
    pub modifiers: String,
    /// A method's type parameters (`<T>`), or empty.
    pub type_params: String,
    /// The field's type, the method's return type, empty for a constructor.
    pub ty: String,
    pub name: String,
    /// A method's parameters as written (`String id, int limit`); `null` for a field.
    pub params: Option<String>,
    /// What a method declares it throws.
    pub throws: Vec<String>,
    /// A constant's value as source would spell it (`42`, `"x"`).
    pub value: Option<String>,
    pub deprecated: bool,
    /// An enum's constant.
    pub constant: bool,
}

/// Why a class file was not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClassError {
    #[error("not a class file (no CAFEBABE)")]
    NotClass,
    #[error("the class file ends early")]
    Truncated,
    #[error("the class file is broken: {0}")]
    Broken(&'static str),
}

type R<T> = Result<T, ClassError>;

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(ClassError::Truncated)?;
        let out = self.bytes.get(self.at..end).ok_or(ClassError::Truncated)?;
        self.at = end;
        Ok(out)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> R<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> R<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[derive(Debug, Clone)]
enum Const {
    Unused,
    Utf8(String),
    Int(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class(u16),
    Str(u16),
    Module(u16),
    Package(u16),
    Other,
}

struct Pool(Vec<Const>);

impl Pool {
    fn utf8(&self, index: u16) -> R<&str> {
        match self.0.get(usize::from(index)) {
            Some(Const::Utf8(s)) => Ok(s),
            _ => Err(ClassError::Broken("a name points at no text")),
        }
    }
    /// A `CONSTANT_Class`'s name, dotted.
    fn class(&self, index: u16) -> R<String> {
        match self.0.get(usize::from(index)) {
            Some(Const::Class(name)) => Ok(self.utf8(*name)?.replace('/', ".")),
            _ => Err(ClassError::Broken("a class points at no class")),
        }
    }
    fn named(&self, index: u16) -> R<String> {
        match self.0.get(usize::from(index)) {
            Some(Const::Module(name) | Const::Package(name)) => {
                Ok(self.utf8(*name)?.replace('/', "."))
            }
            _ => Err(ClassError::Broken("a module points at no name")),
        }
    }
}

/// Java's modified UTF-8: NUL as two bytes and characters past the BMP as surrogate pairs, each three bytes.
fn mutf8(bytes: &[u8]) -> String {
    if let Ok(plain) = std::str::from_utf8(bytes) {
        return plain.to_owned();
    }
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b & 0x80 == 0 {
            units.push(u16::from(b));
            i += 1;
        } else if b & 0xE0 == 0xC0 && i + 1 < bytes.len() {
            units.push((u16::from(b & 0x1F) << 6) | u16::from(bytes[i + 1] & 0x3F));
            i += 2;
        } else if b & 0xF0 == 0xE0 && i + 2 < bytes.len() {
            units.push(
                (u16::from(b & 0x0F) << 12)
                    | (u16::from(bytes[i + 1] & 0x3F) << 6)
                    | u16::from(bytes[i + 2] & 0x3F),
            );
            i += 3;
        } else {
            units.push(0xFFFD);
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

fn read_pool(c: &mut Cursor<'_>) -> R<Pool> {
    let count = c.u16()?;
    let mut pool = Vec::with_capacity(usize::from(count));
    pool.push(Const::Unused);
    let mut index = 1u16;
    while index < count {
        let tag = c.u8()?;
        let entry = match tag {
            1 => {
                let len = c.u16()?;
                Const::Utf8(mutf8(c.take(usize::from(len))?))
            }
            3 => Const::Int(c.u32()?.cast_signed()),
            4 => Const::Float(f32::from_bits(c.u32()?)),
            5 | 6 => {
                let hi = u64::from(c.u32()?);
                let lo = u64::from(c.u32()?);
                let bits = (hi << 32) | lo;
                if tag == 5 {
                    Const::Long(bits.cast_signed())
                } else {
                    Const::Double(f64::from_bits(bits))
                }
            }
            7 => Const::Class(c.u16()?),
            8 => Const::Str(c.u16()?),
            19 => Const::Module(c.u16()?),
            20 => Const::Package(c.u16()?),
            16 => {
                c.u16()?;
                Const::Other
            }
            9 | 10 | 11 | 12 | 17 | 18 => {
                c.u32()?;
                Const::Other
            }
            15 => {
                c.take(3)?;
                Const::Other
            }
            _ => return Err(ClassError::Broken("an unknown constant")),
        };
        let wide = matches!(entry, Const::Long(_) | Const::Double(_));
        pool.push(entry);
        index += 1;
        if wide {
            // A long or a double takes two slots (JVMS 4.4.5).
            pool.push(Const::Unused);
            index += 1;
        }
    }
    Ok(Pool(pool))
}

/// Strips `java.lang.` from a top-level `java.lang` type, as source does; `$` of a nested class is a dot.
fn simple(name: &str) -> String {
    let name = name
        .strip_prefix("java.lang.")
        .filter(|rest| !rest.contains('.'))
        .unwrap_or(name);
    name.replace('$', ".")
}

/// Signatures and descriptors (JVMS 4.3, 4.7.9.1) as Java source writes them.
struct Sig<'a> {
    s: &'a [u8],
    at: usize,
}

impl Sig<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }
    fn next(&mut self) -> R<u8> {
        let b = self
            .peek()
            .ok_or(ClassError::Broken("a signature ends early"))?;
        self.at += 1;
        Ok(b)
    }
    fn until(&mut self, stops: &[u8]) -> R<&str> {
        let start = self.at;
        while let Some(b) = self.peek() {
            if stops.contains(&b) {
                break;
            }
            self.at += 1;
        }
        std::str::from_utf8(&self.s[start..self.at])
            .map_err(|_| ClassError::Broken("a signature is not text"))
    }

    fn ty(&mut self, depth: u8) -> R<String> {
        if depth > 32 {
            return Err(ClassError::Broken("a signature nests too deep"));
        }
        Ok(match self.next()? {
            b'B' => "byte".into(),
            b'C' => "char".into(),
            b'D' => "double".into(),
            b'F' => "float".into(),
            b'I' => "int".into(),
            b'J' => "long".into(),
            b'S' => "short".into(),
            b'Z' => "boolean".into(),
            b'V' => "void".into(),
            b'[' => format!("{}[]", self.ty(depth + 1)?),
            b'T' => {
                let name = self.until(b";")?.to_owned();
                self.next()?;
                name
            }
            b'L' => self.class_ty(depth)?,
            _ => return Err(ClassError::Broken("a signature names no type")),
        })
    }

    /// After the `L`: `pkg/Outer<args>.Inner<args>;`.
    fn class_ty(&mut self, depth: u8) -> R<String> {
        let mut out = simple(&self.until(b"<;.")?.replace('/', "."));
        loop {
            match self.next()? {
                b';' => return Ok(out),
                b'<' => {
                    let mut args = Vec::new();
                    while self.peek() != Some(b'>') {
                        args.push(match self.peek() {
                            Some(b'*') => {
                                self.next()?;
                                "?".to_owned()
                            }
                            Some(b'+') => {
                                self.next()?;
                                format!("? extends {}", self.ty(depth + 1)?)
                            }
                            Some(b'-') => {
                                self.next()?;
                                format!("? super {}", self.ty(depth + 1)?)
                            }
                            _ => self.ty(depth + 1)?,
                        });
                    }
                    self.next()?;
                    out = format!("{out}<{}>", args.join(", "));
                }
                b'.' => {
                    let inner = self.until(b"<;.")?;
                    out = format!("{out}.{inner}");
                }
                _ => return Err(ClassError::Broken("a class type is broken")),
            }
        }
    }

    /// `<T:Ljava/lang/Object;U::Ljava/lang/Comparable<TU;>;>`, where the next byte is `<`.
    fn type_params(&mut self) -> R<String> {
        if self.peek() != Some(b'<') {
            return Ok(String::new());
        }
        self.next()?;
        let mut params = Vec::new();
        while self.peek() != Some(b'>') {
            let name = self.until(b":")?.to_owned();
            let mut bounds = Vec::new();
            while self.peek() == Some(b':') {
                self.next()?;
                if matches!(self.peek(), Some(b'L' | b'T' | b'[')) {
                    bounds.push(self.ty(1)?);
                }
            }
            bounds.retain(|b| b != "Object");
            params.push(if bounds.is_empty() {
                name
            } else {
                format!("{name} extends {}", bounds.join(" & "))
            });
            if self.peek().is_none() {
                return Err(ClassError::Broken("type parameters end early"));
            }
        }
        self.next()?;
        Ok(format!("<{}>", params.join(", ")))
    }
}

/// A method's descriptor or signature: type parameters, parameter types, the return type, what it throws.
struct MethodSig {
    type_params: String,
    params: Vec<String>,
    ret: String,
    throws: Vec<String>,
}

fn method_sig(text: &str) -> R<MethodSig> {
    let mut s = Sig {
        s: text.as_bytes(),
        at: 0,
    };
    let type_params = s.type_params()?;
    if s.next()? != b'(' {
        return Err(ClassError::Broken(
            "a method's descriptor has no parameters",
        ));
    }
    let mut params = Vec::new();
    while s.peek() != Some(b')') {
        params.push(s.ty(0)?);
    }
    s.next()?;
    let ret = s.ty(0)?;
    let mut throws = Vec::new();
    while s.peek() == Some(b'^') {
        s.next()?;
        throws.push(s.ty(0)?);
    }
    Ok(MethodSig {
        type_params,
        params,
        ret,
        throws,
    })
}

fn field_ty(text: &str) -> R<String> {
    Sig {
        s: text.as_bytes(),
        at: 0,
    }
    .ty(0)
}

/// The Java release a class file version is.
#[must_use]
pub fn release(major: u16, minor: u16) -> String {
    let base = match major {
        45 => "Java 1.1".to_owned(),
        46 => "Java 1.2".to_owned(),
        47 => "Java 1.3".to_owned(),
        48 => "Java 1.4".to_owned(),
        49.. => format!("Java {}", major - 44),
        _ => format!("class file {major}"),
    };
    if major >= 56 && minor == 0xFFFF {
        format!("{base} (preview)")
    } else {
        base
    }
}

const PUBLIC: u16 = 0x0001;
const PRIVATE: u16 = 0x0002;
const PROTECTED: u16 = 0x0004;
const STATIC: u16 = 0x0008;
const FINAL: u16 = 0x0010;
const SYNCHRONIZED: u16 = 0x0020;
const VOLATILE: u16 = 0x0040;
const BRIDGE: u16 = 0x0040;
const TRANSIENT: u16 = 0x0080;
const VARARGS: u16 = 0x0080;
const NATIVE: u16 = 0x0100;
const INTERFACE: u16 = 0x0200;
const ABSTRACT: u16 = 0x0400;
const STRICT: u16 = 0x0800;
const SYNTHETIC: u16 = 0x1000;
const ANNOTATION: u16 = 0x2000;
const ENUM: u16 = 0x4000;
const MODULE: u16 = 0x8000;

fn visibility(flags: u16, out: &mut Vec<&'static str>) {
    if flags & PUBLIC != 0 {
        out.push("public");
    } else if flags & PROTECTED != 0 {
        out.push("protected");
    } else if flags & PRIVATE != 0 {
        out.push("private");
    }
}

#[derive(Default)]
struct Attrs {
    signature: Option<String>,
    annotations: Vec<String>,
    deprecated: bool,
    exceptions: Vec<String>,
    constant: Option<String>,
    param_names: Vec<Option<String>>,
    record: Vec<String>,
    permits: Vec<String>,
    /// From `InnerClasses`: this class's own flags where it is nested.
    inner_flags: Option<u16>,
    source_file: Option<String>,
    module: Vec<String>,
    module_name: Option<String>,
}

/// What an attribute list is read for, so a field's attributes are not read as a class's.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Class,
    Field,
    Method,
    Component,
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn constant(pool: &Pool, index: u16, ty: &str) -> R<String> {
    Ok(match pool.0.get(usize::from(index)) {
        Some(Const::Int(v)) => match ty {
            "boolean" => (if *v == 0 { "false" } else { "true" }).to_owned(),
            "char" => char::from_u32(v.cast_unsigned())
                .map_or_else(|| v.to_string(), |c| format!("'{}'", c.escape_default())),
            _ => v.to_string(),
        },
        Some(Const::Long(v)) => format!("{v}L"),
        Some(Const::Float(v)) => format!("{v}f"),
        Some(Const::Double(v)) => format!("{v}"),
        Some(Const::Str(s)) => {
            let text = pool.utf8(*s)?;
            // A long constant is cut: the outline is a line.
            if text.chars().count() > 200 {
                format!("{}…", quote(&text.chars().take(200).collect::<String>()))
            } else {
                quote(text)
            }
        }
        _ => return Err(ClassError::Broken("a constant points at nothing")),
    })
}

fn element(c: &mut Cursor<'_>, pool: &Pool, depth: u8) -> R<String> {
    if depth > 16 {
        return Err(ClassError::Broken("an annotation nests too deep"));
    }
    let tag = c.u8()?;
    Ok(match tag {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => {
            let ty = match tag {
                b'Z' => "boolean",
                b'C' => "char",
                _ => "",
            };
            let index = c.u16()?;
            constant(pool, index, ty)?
        }
        b's' => quote(pool.utf8(c.u16()?)?),
        b'e' => {
            let ty = field_ty(pool.utf8(c.u16()?)?)?;
            let name = pool.utf8(c.u16()?)?;
            format!("{ty}.{name}")
        }
        b'c' => format!("{}.class", field_ty(pool.utf8(c.u16()?)?)?),
        b'@' => annotation(c, pool, depth + 1)?,
        b'[' => {
            let n = c.u16()?;
            let mut items = Vec::new();
            for _ in 0..n {
                items.push(element(c, pool, depth + 1)?);
            }
            if items.len() == 1 {
                items.remove(0)
            } else {
                format!("{{{}}}", items.join(", "))
            }
        }
        _ => {
            return Err(ClassError::Broken(
                "an annotation's value is of no known kind",
            ));
        }
    })
}

fn annotation(c: &mut Cursor<'_>, pool: &Pool, depth: u8) -> R<String> {
    let ty = field_ty(pool.utf8(c.u16()?)?)?;
    let pairs = c.u16()?;
    let mut args = Vec::new();
    for _ in 0..pairs {
        let name = pool.utf8(c.u16()?)?.to_owned();
        let value = element(c, pool, depth)?;
        args.push(if pairs == 1 && name == "value" {
            value
        } else {
            format!("{name} = {value}")
        });
    }
    Ok(if args.is_empty() {
        format!("@{ty}")
    } else {
        format!("@{ty}({})", args.join(", "))
    })
}

fn read_attrs(c: &mut Cursor<'_>, pool: &Pool, owner: Owner, this: Option<u16>) -> R<Attrs> {
    let mut attrs = Attrs::default();
    let count = c.u16()?;
    for _ in 0..count {
        let name = pool.utf8(c.u16()?)?.to_owned();
        let len = c.u32()? as usize;
        let body = c.take(len)?;
        let mut a = Cursor { bytes: body, at: 0 };
        match name.as_str() {
            "Signature" => attrs.signature = Some(pool.utf8(a.u16()?)?.to_owned()),
            "Deprecated" => attrs.deprecated = true,
            "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                let n = a.u16()?;
                for _ in 0..n {
                    let text = annotation(&mut a, pool, 0)?;
                    if text == "@Deprecated" {
                        attrs.deprecated = true;
                    }
                    attrs.annotations.push(text);
                }
            }
            "Exceptions" if owner == Owner::Method => {
                let n = a.u16()?;
                for _ in 0..n {
                    attrs.exceptions.push(simple(&pool.class(a.u16()?)?));
                }
            }
            "ConstantValue" if owner == Owner::Field => attrs.constant = Some(a.u16()?.to_string()),
            "MethodParameters" if owner == Owner::Method => {
                let n = a.u8()?;
                for _ in 0..n {
                    let index = a.u16()?;
                    let _flags = a.u16()?;
                    attrs.param_names.push(if index == 0 {
                        None
                    } else {
                        Some(pool.utf8(index)?.to_owned())
                    });
                }
            }
            "SourceFile" if owner == Owner::Class => {
                attrs.source_file = Some(pool.utf8(a.u16()?)?.to_owned())
            }
            "PermittedSubclasses" if owner == Owner::Class => {
                let n = a.u16()?;
                for _ in 0..n {
                    attrs.permits.push(simple(&pool.class(a.u16()?)?));
                }
            }
            "InnerClasses" if owner == Owner::Class => {
                let n = a.u16()?;
                for _ in 0..n {
                    let inner = a.u16()?;
                    let _outer = a.u16()?;
                    let _name = a.u16()?;
                    let flags = a.u16()?;
                    if Some(inner) == this {
                        attrs.inner_flags = Some(flags);
                    }
                }
            }
            "Record" if owner == Owner::Class => {
                let n = a.u16()?;
                for _ in 0..n {
                    let cname = pool.utf8(a.u16()?)?.to_owned();
                    let desc = pool.utf8(a.u16()?)?.to_owned();
                    let inner = read_attrs(&mut a, pool, Owner::Component, None)?;
                    let ty = field_ty(inner.signature.as_deref().unwrap_or(&desc))?;
                    let ann = inner
                        .annotations
                        .iter()
                        .map(|x| format!("{x} "))
                        .collect::<String>();
                    attrs.record.push(format!("{ann}{ty} {cname}"));
                }
            }
            "Module" if owner == Owner::Class => read_module(&mut a, pool, &mut attrs)?,
            _ => {}
        }
    }
    Ok(attrs)
}

fn read_module(a: &mut Cursor<'_>, pool: &Pool, attrs: &mut Attrs) -> R<()> {
    attrs.module_name = Some(pool.named(a.u16()?)?);
    let _flags = a.u16()?;
    let version = a.u16()?;
    if version != 0 {
        attrs
            .module
            .push(format!("version {}", pool.utf8(version)?));
    }
    let requires = a.u16()?;
    for _ in 0..requires {
        let name = pool.named(a.u16()?)?;
        let flags = a.u16()?;
        let _ver = a.u16()?;
        if name == "java.base" {
            continue;
        }
        let mut words = vec!["requires"];
        if flags & 0x0020 != 0 {
            words.push("transitive");
        }
        if flags & 0x0040 != 0 {
            words.push("static");
        }
        attrs.module.push(format!("{} {name};", words.join(" ")));
    }
    for word in ["exports", "opens"] {
        let n = a.u16()?;
        for _ in 0..n {
            let package = pool.named(a.u16()?)?;
            let _flags = a.u16()?;
            let to = a.u16()?;
            let mut targets = Vec::new();
            for _ in 0..to {
                targets.push(pool.named(a.u16()?)?);
            }
            attrs.module.push(if targets.is_empty() {
                format!("{word} {package};")
            } else {
                format!("{word} {package} to {};", targets.join(", "))
            });
        }
    }
    let uses = a.u16()?;
    for _ in 0..uses {
        attrs
            .module
            .push(format!("uses {};", simple(&pool.class(a.u16()?)?)));
    }
    let provides = a.u16()?;
    for _ in 0..provides {
        let service = simple(&pool.class(a.u16()?)?);
        let n = a.u16()?;
        let mut with = Vec::new();
        for _ in 0..n {
            with.push(simple(&pool.class(a.u16()?)?));
        }
        attrs
            .module
            .push(format!("provides {service} with {};", with.join(", ")));
    }
    Ok(())
}

/// The simple name a constructor is written with: the class's name after its package and outer classes.
fn ctor_name(binary: &str) -> String {
    let tail = binary.rsplit('.').next().unwrap_or(binary);
    tail.rsplit('$').next().unwrap_or(tail).to_owned()
}

/// Reads `bytes` as a class file.
///
/// # Errors
///
/// [`ClassError`] for bytes that are not a class file or end or point wrong.
#[allow(clippy::too_many_lines)]
pub fn outline(bytes: &[u8]) -> Result<ClassOutline, ClassError> {
    let mut c = Cursor { bytes, at: 0 };
    if c.u32().map_err(|_| ClassError::NotClass)? != 0xCAFE_BABE {
        return Err(ClassError::NotClass);
    }
    let minor = c.u16()?;
    let major = c.u16()?;
    let pool = read_pool(&mut c)?;
    let class_flags = c.u16()?;
    let this = c.u16()?;
    let name = if class_flags & MODULE != 0 {
        String::new()
    } else {
        pool.class(this)?
    };
    let super_index = c.u16()?;
    let super_name = if super_index == 0 {
        None
    } else {
        Some(pool.class(super_index)?)
    };
    let n = c.u16()?;
    let mut interfaces = Vec::new();
    for _ in 0..n {
        interfaces.push(simple(&pool.class(c.u16()?)?));
    }

    let mut synthetic = 0u32;
    let mut fields = Vec::new();
    let fcount = c.u16()?;
    for _ in 0..fcount {
        let flags = c.u16()?;
        let fname = pool.utf8(c.u16()?)?.to_owned();
        let desc = pool.utf8(c.u16()?)?.to_owned();
        let attrs = read_attrs(&mut c, &pool, Owner::Field, None)?;
        if flags & SYNTHETIC != 0 {
            synthetic += 1;
            continue;
        }
        let ty =
            field_ty(attrs.signature.as_deref().unwrap_or(&desc)).or_else(|_| field_ty(&desc))?;
        let mut mods = Vec::new();
        let constant_of_enum = flags & ENUM != 0;
        if !constant_of_enum {
            visibility(flags, &mut mods);
            if flags & STATIC != 0 {
                mods.push("static");
            }
            if flags & FINAL != 0 {
                mods.push("final");
            }
            if flags & TRANSIENT != 0 {
                mods.push("transient");
            }
            if flags & VOLATILE != 0 {
                mods.push("volatile");
            }
        }
        let value = match attrs
            .constant
            .as_deref()
            .and_then(|i| i.parse::<u16>().ok())
        {
            Some(index) => Some(constant(&pool, index, &ty)?),
            None => None,
        };
        fields.push(Member {
            annotations: attrs
                .annotations
                .into_iter()
                .filter(|a| a != "@Deprecated")
                .collect(),
            modifiers: mods.join(" "),
            type_params: String::new(),
            ty,
            name: fname,
            params: None,
            throws: Vec::new(),
            value,
            deprecated: attrs.deprecated,
            constant: constant_of_enum,
        });
    }

    let is_interface = class_flags & INTERFACE != 0;
    let mut methods = Vec::new();
    let mcount = c.u16()?;
    for _ in 0..mcount {
        let flags = c.u16()?;
        let mname = pool.utf8(c.u16()?)?.to_owned();
        let desc = pool.utf8(c.u16()?)?.to_owned();
        let attrs = read_attrs(&mut c, &pool, Owner::Method, None)?;
        if flags & (SYNTHETIC | BRIDGE) != 0 || mname == "<clinit>" {
            if mname != "<clinit>" {
                synthetic += 1;
            }
            continue;
        }
        let erased = method_sig(&desc)?;
        let sig = match attrs.signature.as_deref().map(method_sig) {
            // A signature that names fewer parameters than the descriptor (an inner class's constructor, an enum's)
            // leaves out the ones the compiler adds; the descriptor's count is what is called.
            Some(Ok(sig)) => sig,
            _ => erased,
        };
        let throws = if sig.throws.is_empty() {
            attrs.exceptions.clone()
        } else {
            sig.throws.clone()
        };
        let count = sig.params.len();
        let params = sig
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let ty = if i + 1 == count && flags & VARARGS != 0 && ty.ends_with("[]") {
                    format!("{}...", &ty[..ty.len() - 2])
                } else {
                    ty.clone()
                };
                // Names where `MethodParameters` kept them. It names the compiler's own leading parameters too
                // (an inner class's outer, an enum's name and ordinal) where a signature leaves them out, so the
                // names are matched from the end; fewer names than parameters name none.
                let skip = attrs.param_names.len().checked_sub(count);
                match skip.and_then(|skip| attrs.param_names.get(skip + i).cloned().flatten()) {
                    Some(name) => format!("{ty} {name}"),
                    None => ty,
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let ctor = mname == "<init>";
        let mut mods = Vec::new();
        visibility(flags, &mut mods);
        if is_interface {
            // An interface's methods are public and abstract unless they say otherwise: say only what differs.
            mods.retain(|m| *m != "public");
            if flags & STATIC != 0 {
                mods.push("static");
            } else if flags & ABSTRACT == 0 && flags & PRIVATE == 0 {
                mods.push("default");
            }
        } else {
            if flags & ABSTRACT != 0 {
                mods.push("abstract");
            }
            if flags & STATIC != 0 {
                mods.push("static");
            }
            if flags & FINAL != 0 {
                mods.push("final");
            }
        }
        if flags & SYNCHRONIZED != 0 {
            mods.push("synchronized");
        }
        if flags & NATIVE != 0 {
            mods.push("native");
        }
        if flags & STRICT != 0 && major < 61 {
            mods.push("strictfp");
        }
        methods.push(Member {
            annotations: attrs
                .annotations
                .into_iter()
                .filter(|a| a != "@Deprecated")
                .collect(),
            modifiers: mods.join(" "),
            type_params: sig.type_params,
            ty: if ctor { String::new() } else { sig.ret },
            name: if ctor { ctor_name(&name) } else { mname },
            params: Some(params),
            throws,
            value: None,
            deprecated: attrs.deprecated,
            constant: false,
        });
    }

    let attrs = read_attrs(&mut c, &pool, Owner::Class, Some(this))?;
    // A nested class's own flags (`private`, `static`, `protected`) live in `InnerClasses`.
    let flags = attrs.inner_flags.unwrap_or(class_flags);
    let is_record = !attrs.record.is_empty() || super_name.as_deref() == Some("java.lang.Record");
    let kind = if class_flags & MODULE != 0 {
        "module"
    } else if class_flags & ANNOTATION != 0 {
        "@interface"
    } else if is_interface {
        "interface"
    } else if class_flags & ENUM != 0 {
        "enum"
    } else if is_record {
        "record"
    } else {
        "class"
    };
    let mut mods = Vec::new();
    visibility(flags, &mut mods);
    if attrs.inner_flags.is_some() && flags & STATIC != 0 && kind == "class" {
        mods.push("static");
    }
    if kind == "class" && flags & ABSTRACT != 0 {
        mods.push("abstract");
    }
    if !attrs.permits.is_empty() {
        mods.push("sealed");
    }
    if kind == "class" && flags & FINAL != 0 && attrs.permits.is_empty() {
        mods.push("final");
    }

    let (type_params, extends, implements) = match attrs.signature.as_deref() {
        Some(text) => {
            let mut s = Sig {
                s: text.as_bytes(),
                at: 0,
            };
            let tp = s.type_params()?;
            let sup = s.ty(0)?;
            let mut ifs = Vec::new();
            while s.peek().is_some() {
                ifs.push(s.ty(0)?);
            }
            (tp, Some(sup), ifs)
        }
        None => (String::new(), super_name.as_deref().map(simple), interfaces),
    };
    // What source never writes: `extends Object`, an enum's `Enum<E>`, a record's `Record`, an interface's `Object`.
    let extends = extends.filter(|s| {
        !(s == "Object" || s.starts_with("Enum<") || s == "Enum" || s == "Record" || is_interface)
    });
    let implements = if kind == "@interface" {
        implements
            .into_iter()
            .filter(|i| i != "java.lang.annotation.Annotation" && i != "annotation.Annotation")
            .collect()
    } else {
        implements
    };
    if kind == "enum" {
        // `values()` and `valueOf(String)` are the compiler's.
        methods.retain(|m| {
            !(m.modifiers.contains("static") && (m.name == "values" || m.name == "valueOf"))
        });
    }
    if kind == "record" {
        fields.retain(|f| f.modifiers.contains("static"));
    }

    Ok(ClassOutline {
        major,
        minor,
        release: release(major, minor),
        kind: kind.to_owned(),
        modifiers: mods.join(" "),
        name: if kind == "module" {
            attrs.module_name.clone().unwrap_or_default()
        } else {
            name
        },
        type_params,
        extends,
        implements,
        permits: attrs.permits,
        annotations: attrs
            .annotations
            .into_iter()
            .filter(|a| a != "@Deprecated")
            .collect(),
        components: attrs.record,
        fields,
        methods,
        synthetic,
        source_file: attrs.source_file,
        deprecated: attrs.deprecated,
        module: attrs.module,
    })
}

/// The class file version alone, read from the first eight bytes: what a class's row shows before it is opened.
#[must_use]
pub fn version(head: &[u8]) -> Option<(u16, u16)> {
    if head.len() < 8 || head[..4] != [0xCA, 0xFE, 0xBA, 0xBE] {
        return None;
    }
    Some((
        u16::from_be_bytes([head[6], head[7]]),
        u16::from_be_bytes([head[4], head[5]]),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_read_as_source_writes_them() {
        assert_eq!(
            field_ty("Ljava/util/List<Ljava/lang/String;>;").unwrap(),
            "java.util.List<String>"
        );
        assert_eq!(field_ty("[[I").unwrap(), "int[][]");
        assert_eq!(
            field_ty("Ljava/util/Map<TK;+Ljava/lang/Number;>;").unwrap(),
            "java.util.Map<K, ? extends Number>"
        );
        assert_eq!(
            field_ty("Ljava/util/Map$Entry;").unwrap(),
            "java.util.Map.Entry"
        );
        assert_eq!(
            field_ty("Lcom/x/Outer<TT;>.Inner<-TU;*>;").unwrap(),
            "com.x.Outer<T>.Inner<? super U, ?>"
        );
        let m = method_sig(
            "<T::Ljava/lang/Comparable<TT;>;>(TT;[Ljava/lang/Object;)TT;^Ljava/io/IOException;",
        )
        .unwrap();
        assert_eq!(m.type_params, "<T extends Comparable<T>>");
        assert_eq!(m.params, vec!["T", "Object[]"]);
        assert_eq!(m.ret, "T");
        assert_eq!(m.throws, vec!["java.io.IOException"]);
    }

    #[test]
    fn a_broken_signature_is_an_error_never_a_panic() {
        for bad in [
            "L",
            "Ljava/util/List<",
            "[",
            "(I",
            "<T:",
            "Q",
            "(",
            "<T::Lx;",
        ] {
            assert!(field_ty(bad).is_err() || method_sig(bad).is_err(), "{bad}");
        }
        let deep = "[".repeat(100) + "I";
        assert!(field_ty(&deep).is_err());
    }

    #[test]
    fn releases_are_named() {
        assert_eq!(release(52, 0), "Java 8");
        assert_eq!(release(65, 0), "Java 21");
        assert_eq!(release(48, 0), "Java 1.4");
        assert_eq!(release(65, 0xFFFF), "Java 21 (preview)");
        assert_eq!(
            version(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 61]),
            Some((61, 0))
        );
        assert_eq!(version(b"PK\x03\x04aaaa"), None);
    }

    #[test]
    fn modified_utf8_reads_nul_and_supplementary_characters() {
        assert_eq!(mutf8(&[0x61, 0xC0, 0x80, 0x62]), "a\u{0}b");
        // U+1F600 as a surrogate pair, three bytes each.
        assert_eq!(mutf8(&[0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]), "\u{1F600}");
    }

    #[test]
    fn not_a_class_and_a_cut_class_are_errors() {
        assert_eq!(outline(b"PK\x03\x04"), Err(ClassError::NotClass));
        assert_eq!(
            outline(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52, 0, 9]),
            Err(ClassError::Truncated)
        );
    }
}
