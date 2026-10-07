//! Blob format v1 (ARCHITECTURE.md §8 step 7; PLAN.md M2.4; map 09 D3): one fluid's record as versioned
//! little-endian bytes, validated whole before anything is decoded.
//!
//! - A 32-byte header: magic `PKITBLOB`, version (u32), section count (u32), total length (u64) and a checksum (u64,
//!   FNV-1a 64 of every byte after the header).
//! - The section table: one 24-byte entry per section (id u32, zero u32, offset u64, length u64).
//! - The sections, each starting at a multiple of 8 bytes, in id order, with no gaps beyond that padding.
//!
//! A pack (`Pack::new`; a browser's `withPack`) is several blobs with their index in one buffer, format v1 too: a
//! 32-byte header (magic `PKITPACK`, version, fluid count, length, checksum), then per fluid its names (canonical
//! first), the canonical names of its references, and its blob, 8-byte aligned. Every blob in it is validated when
//! the pack is read, and decoded only on first use.
//!
//! v1 lists every planned section from the start, each empty until the step that fills it, so filling one changes the
//! bytes, not the version. Only a layout change to a filled section bumps the version; the decoder refuses any other
//! version, and packs are regenerated, never migrated. Ids and tags are written from the tables here, never from
//! enum discriminants (ROT-055). The EOS section is `EosRecord::encode`'s bytes, the encoder the hash gate uses
//! (E14), so α⁰ lives there too. Restricted data (`FluidRecord::environmental`) is never written (D14), and
//! `applied` is runtime state.

use std::sync::Arc;

use crate::data::{
    CaloricCurves, CaloricStamp, Edit, EosRecord, FluidRecord, MeltingSegment, Patch, SaStamp, Superancillary,
};
use crate::error::LoadError;
use crate::model::{Citation, CitationRole, CriticalOrigin, CriticalPoint, DataTerms, Limits, ModelKey, Source};

/// The first eight bytes of every v1 blob.
const MAGIC: &[u8; 8] = b"PKITBLOB";
/// The blob format version this decoder reads and this encoder writes. Version 2 (M2.10) adds the role-tagged citations
/// to the filled metadata section; packs keep their own version.
pub(crate) const VERSION: u32 = 2;
/// The pack format version.
const PACK_VERSION: u32 = 1;
const HEADER: usize = 32;
const ENTRY: usize = 24;

/// The sections of v1 in id order: id, name, and the step that fills a section this decoder cannot read yet.
pub(crate) const SECTIONS: [(u32, &str, Option<&str>); 10] = [
    (1, "metadata", None),
    (2, "eos", None),
    (3, "superancillary fit", None),
    (4, "superancillary", None),
    (5, "caloric curves", None),
    (6, "ancillaries", Some("M6.3")),
    (7, "transport", Some("M8.1")),
    (8, "surface tension", Some("M8.4")),
    (9, "melting", None),
    (10, "corrections", None),
];

fn bad(message: String) -> LoadError {
    LoadError::Format(message.into())
}

/// A cursor over one section's bytes; every read is bounds-checked.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    section: &'static str,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], section: &'static str) -> Self {
        Self { bytes, at: 0, section }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], LoadError> {
        let end = self.at.checked_add(N).filter(|&end| end <= self.bytes.len());
        let chunk = end.and_then(|end| self.bytes.get(self.at..end)).and_then(|c| <[u8; N]>::try_from(c).ok());
        let chunk = chunk.ok_or_else(|| bad(format!("{} section is truncated", self.section)))?;
        self.at += N;
        Ok(chunk)
    }

    fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.take::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, LoadError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, LoadError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    fn f64(&mut self) -> Result<f64, LoadError> {
        Ok(f64::from_le_bytes(self.take()?))
    }

    fn str(&mut self) -> Result<String, LoadError> {
        let len = self.u32()? as usize;
        let end = self.at.checked_add(len).filter(|&end| end <= self.bytes.len());
        let bytes = end.and_then(|end| self.bytes.get(self.at..end));
        let bytes = bytes.ok_or_else(|| bad(format!("{} section is truncated", self.section)))?;
        let text =
            core::str::from_utf8(bytes).map_err(|_| bad(format!("{} section: a string is not UTF-8", self.section)))?;
        self.at += len;
        Ok(text.to_string())
    }

    /// A presence byte (0 or 1), then the value.
    fn opt<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T, LoadError>) -> Result<Option<T>, LoadError> {
        match self.u8()? {
            0 => Ok(None),
            1 => read(self).map(Some),
            flag => Err(bad(format!("{} section: presence byte {flag}", self.section))),
        }
    }

    fn done(&self) -> Result<(), LoadError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(bad(format!("{} section has bytes left over ({})", self.section, self.bytes.len() - self.at)))
        }
    }
}

fn put_u32(out: &mut Vec<u8>, x: u32) {
    out.extend_from_slice(&x.to_le_bytes());
}

fn put_f64(out: &mut Vec<u8>, x: f64) {
    out.extend_from_slice(&x.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn put_opt<T>(out: &mut Vec<u8>, x: Option<T>, put: impl FnOnce(&mut Vec<u8>, T)) {
    match x {
        None => out.push(0),
        Some(x) => {
            out.push(1);
            put(out, x);
        }
    }
}

const TERMS: [(DataTerms, u8); 3] =
    [(DataTerms::Published, 0), (DataTerms::Unpublished, 1), (DataTerms::Restricted, 2)];
const ORIGINS: [(CriticalOrigin, u8); 2] = [(CriticalOrigin::Published, 0), (CriticalOrigin::Model, 1)];
const ROLES: [(CitationRole, u8); 4] = [
    (CitationRole::Coefficients, 0),
    (CitationRole::IdealGas, 1),
    (CitationRole::Erratum, 2),
    (CitationRole::Related, 3),
];

fn tag_of<T: PartialEq + Copy>(table: &[(T, u8)], x: T) -> u8 {
    table.iter().find(|(t, _)| *t == x).map_or(u8::MAX, |(_, tag)| *tag)
}

fn from_tag<T: Copy>(table: &[(T, u8)], tag: u8, what: &str) -> Result<T, LoadError> {
    table.iter().find(|(_, t)| *t == tag).map(|(x, _)| *x).ok_or_else(|| bad(format!("unknown {what} tag {tag}")))
}

fn metadata(r: &FluidRecord) -> Vec<u8> {
    let mut out = Vec::new();
    put_str(&mut out, &r.name);
    put_u32(&mut out, r.aliases.len() as u32);
    r.aliases.iter().for_each(|a| put_str(&mut out, a));
    for id in [&r.cas, &r.refprop_name, &r.inchi_key] {
        put_opt(&mut out, id.as_deref(), put_str);
    }
    put_f64(&mut out, r.molar_mass);
    let l = &r.limits;
    [l.t_min(), l.t_max(), l.p_max()].into_iter().for_each(|x| put_f64(&mut out, x));
    put_opt(&mut out, l.t_triple(), put_f64);
    put_opt(&mut out, r.critical, |out, c| {
        [c.t, c.p, c.rho].into_iter().for_each(|x| put_f64(out, x));
        out.push(tag_of(&ORIGINS, c.origin));
    });
    put_str(&mut out, &r.source.bibkey);
    put_opt(&mut out, r.source.doi.as_deref(), put_str);
    out.push(tag_of(&TERMS, r.source.terms));
    put_u32(&mut out, r.source.citations.len() as u32);
    for c in &r.source.citations {
        put_str(&mut out, &c.key);
        put_opt(&mut out, c.doi.as_deref(), put_str);
        out.push(tag_of(&ROLES, c.role));
    }
    out
}

/// The record a blob decodes to, before its EOS, fit, melting and corrections are added.
fn read_metadata(bytes: &[u8], eos: EosRecord) -> Result<FluidRecord, LoadError> {
    let mut r = Reader::new(bytes, "metadata");
    let name = r.str()?;
    let aliases = (0..r.u32()?).map(|_| r.str()).collect::<Result<Vec<_>, _>>()?;
    let [cas, refprop_name, inchi_key] = [r.opt(Reader::str)?, r.opt(Reader::str)?, r.opt(Reader::str)?];
    let molar_mass = r.f64()?;
    let (t_min, t_max, p_max) = (r.f64()?, r.f64()?, r.f64()?);
    let limits = Limits::new(t_min, t_max, p_max).map_err(|e| bad(format!("metadata section: {e}")))?;
    let limits = match r.opt(Reader::f64)? {
        Some(t) => limits.with_t_triple(t),
        None => limits,
    };
    let critical = r.opt(|r| {
        let (t, p, rho) = (r.f64()?, r.f64()?, r.f64()?);
        Ok(CriticalPoint { t, p, rho, origin: from_tag(&ORIGINS, r.u8()?, "critical-point origin")? })
    })?;
    let bibkey: String = r.str()?;
    let doi = r.opt(Reader::str)?;
    let terms = from_tag(&TERMS, r.u8()?, "data terms")?;
    let citations = (0..r.u32()?)
        .map(|_| {
            let (key, doi) = (r.str()?.into(), r.opt(Reader::str)?.map(Into::into));
            Ok(Citation { key, doi, role: from_tag(&ROLES, r.u8()?, "citation role")? })
        })
        .collect::<Result<Vec<_>, LoadError>>()?;
    r.done()?;
    let mut source = Source::new(&bibkey, doi.as_deref(), terms);
    source.citations = citations;
    let mut record = FluidRecord::new(&name, molar_mass, source, eos, limits);
    (record.aliases, record.cas, record.refprop_name, record.inchi_key) = (aliases, cas, refprop_name, inchi_key);
    record.critical = critical;
    Ok(record)
}

fn fit(stamp: Option<SaStamp>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(s) = stamp {
        out.extend_from_slice(&s.shape.get().to_le_bytes());
        put_f64(&mut out, s.gas_constant);
        put_f64(&mut out, s.rho_reducing);
    }
    out
}

fn read_fit(bytes: &[u8]) -> Result<Option<SaStamp>, LoadError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut r = Reader::new(bytes, "superancillary fit");
    let stamp = SaStamp { shape: ModelKey::from_raw(r.u64()?), gas_constant: r.f64()?, rho_reducing: r.f64()? };
    r.done()?;
    Ok(Some(stamp))
}

/// The superancillary section (M5.2): the T boundaries (count, values), ρ′, ρ″ and p (each a piece count, then 13
/// coefficients per piece), the extrema of each curve (count, values), the ln p boundaries and the T(ln p) pieces.
fn superancillary(sa: Option<&Superancillary>) -> Vec<u8> {
    let mut out = Vec::new();
    let list = |out: &mut Vec<u8>, values: &[f64]| {
        put_u32(out, values.len() as u32);
        values.iter().for_each(|x| put_f64(out, *x));
    };
    let pieces = |out: &mut Vec<u8>, pieces: &[[f64; 13]]| {
        put_u32(out, pieces.len() as u32);
        pieces.iter().flatten().for_each(|x| put_f64(out, *x));
    };
    if let Some(sa) = sa {
        list(&mut out, &sa.breaks);
        sa.curves.iter().for_each(|curve| pieces(&mut out, curve));
        sa.extrema.iter().for_each(|extrema| list(&mut out, extrema));
        list(&mut out, &sa.ln_p_breaks);
        pieces(&mut out, &sa.t_of_ln_p);
    }
    out
}

fn read_superancillary(bytes: &[u8]) -> Result<Option<Superancillary>, LoadError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut r = Reader::new(bytes, "superancillary");
    let list = |r: &mut Reader<'_>| (0..r.u32()?).map(|_| r.f64()).collect::<Result<Vec<_>, _>>();
    let pieces = |r: &mut Reader<'_>| {
        (0..r.u32()?)
            .map(|_| {
                let mut piece = [0.0; 13];
                for c in &mut piece {
                    *c = r.f64()?;
                }
                Ok(piece)
            })
            .collect::<Result<Vec<_>, LoadError>>()
    };
    let breaks = list(&mut r)?;
    let curves = [pieces(&mut r)?, pieces(&mut r)?, pieces(&mut r)?];
    let extrema = [list(&mut r)?, list(&mut r)?, list(&mut r)?];
    let (ln_p_breaks, t_of_ln_p) = (list(&mut r)?, pieces(&mut r)?);
    r.done()?;
    let sa = Superancillary { breaks, curves, extrema, ln_p_breaks, t_of_ln_p };
    sa.check()?;
    Ok(Some(sa))
}

/// The caloric section (M2.11 layout, filled from M5.2a): the stamp (superancillary shape, R, ρ_r; α⁰ a1, a2), the
/// piece boundaries (count, values), then each of the six curves (piece count, 13 coefficients per piece).
fn caloric(curves: Option<&CaloricCurves>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(c) = curves {
        let s = &c.stamp;
        out.extend_from_slice(&s.superancillary.shape.get().to_le_bytes());
        [s.superancillary.gas_constant, s.superancillary.rho_reducing, s.a1, s.a2]
            .into_iter()
            .for_each(|x| put_f64(&mut out, x));
        put_u32(&mut out, c.breaks.len() as u32);
        c.breaks.iter().for_each(|x| put_f64(&mut out, *x));
        for curve in &c.curves {
            put_u32(&mut out, curve.len() as u32);
            curve.iter().flatten().for_each(|x| put_f64(&mut out, *x));
        }
    }
    out
}

fn read_caloric(bytes: &[u8], fit: Option<SaStamp>) -> Result<Option<CaloricCurves>, LoadError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut r = Reader::new(bytes, "caloric curves");
    let shape = ModelKey::from_raw(r.u64()?);
    let superancillary = SaStamp { shape, gas_constant: r.f64()?, rho_reducing: r.f64()? };
    let stamp = CaloricStamp { superancillary, a1: r.f64()?, a2: r.f64()? };
    let breaks = (0..r.u32()?).map(|_| r.f64()).collect::<Result<Vec<_>, _>>()?;
    let mut curves: [Vec<[f64; 13]>; 6] = Default::default();
    for curve in &mut curves {
        for _ in 0..r.u32()? {
            let mut piece = [0.0; 13];
            for c in &mut piece {
                *c = r.f64()?;
            }
            curve.push(piece);
        }
    }
    r.done()?;
    let curves = CaloricCurves { stamp, breaks, curves };
    curves.check()?;
    if fit != Some(stamp.superancillary) {
        return Err(bad("caloric curves: sampled on another superancillary than the blob's".into()));
    }
    Ok(Some(curves))
}

fn melting(segments: &[MeltingSegment]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in segments {
        [s.t0, s.p0, s.t_min, s.t_max].into_iter().for_each(|x| put_f64(&mut out, x));
    }
    out
}

fn read_melting(bytes: &[u8]) -> Result<Vec<MeltingSegment>, LoadError> {
    let mut r = Reader::new(bytes, "melting");
    let mut segments = Vec::new();
    while r.at < bytes.len() {
        segments.push(MeltingSegment { t0: r.f64()?, p0: r.f64()?, t_min: r.f64()?, t_max: r.f64()? });
    }
    Ok(segments)
}

fn corrections(patches: &[Patch]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in patches {
        put_str(&mut out, &p.divergence);
        match p.edit {
            Edit::GasConstant(r) => {
                out.push(1);
                put_f64(&mut out, r);
            }
            Edit::ReducingDensity(rho) => {
                out.push(2);
                put_f64(&mut out, rho);
            }
            Edit::MolarMass(m) => {
                out.push(3);
                put_f64(&mut out, m);
            }
            Edit::MeltingP0 { segment, p0 } => {
                out.push(4);
                out.push(segment);
                put_f64(&mut out, p0);
            }
        }
    }
    out
}

fn read_corrections(bytes: &[u8]) -> Result<Vec<Patch>, LoadError> {
    let mut r = Reader::new(bytes, "corrections");
    let mut patches = Vec::new();
    while r.at < bytes.len() {
        let divergence = r.str()?.into();
        let edit = match r.u8()? {
            1 => Edit::GasConstant(r.f64()?),
            2 => Edit::ReducingDensity(r.f64()?),
            3 => Edit::MolarMass(r.f64()?),
            4 => Edit::MeltingP0 { segment: r.u8()?, p0: r.f64()? },
            tag => return Err(bad(format!("corrections section: unknown edit tag {tag}"))),
        };
        patches.push(Patch { divergence, edit });
    }
    Ok(patches)
}

/// The number of sections of v1.
pub(crate) const SECTION_COUNT: usize = SECTIONS.len();

/// The v1 blob of `record`.
pub(crate) fn encode(record: &FluidRecord) -> Vec<u8> {
    assemble(&sections(record))
}

/// The section bodies of `record`, in id order; the sections no step fills yet are empty.
pub(crate) fn sections(record: &FluidRecord) -> [Vec<u8>; SECTION_COUNT] {
    let mut eos = Vec::new();
    record.eos.encode(&mut eos);
    let mut bodies: [Vec<u8>; SECTION_COUNT] = Default::default();
    bodies[0] = metadata(record);
    bodies[1] = eos;
    bodies[2] = fit(record.superancillary_fit);
    bodies[3] = superancillary(record.superancillary.as_ref());
    bodies[4] = caloric(record.caloric.as_ref());
    bodies[8] = melting(&record.melting);
    bodies[9] = corrections(&record.corrections);
    bodies
}

/// A v1 blob holding `bodies` as its sections, in id order.
pub(crate) fn assemble(bodies: &[Vec<u8>; SECTION_COUNT]) -> Vec<u8> {
    let mut offset = HEADER + ENTRY * SECTIONS.len();
    let mut table = Vec::new();
    let mut body = Vec::new();
    for ((id, ..), bytes) in SECTIONS.iter().zip(bodies) {
        let padding = offset.next_multiple_of(8) - offset;
        body.resize(body.len() + padding, 0);
        offset += padding;
        put_u32(&mut table, *id);
        put_u32(&mut table, 0);
        table.extend_from_slice(&(offset as u64).to_le_bytes());
        table.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        body.extend_from_slice(bytes);
        offset += bytes.len();
    }
    let checked: Vec<u8> = table.into_iter().chain(body).collect();
    let mut out = Vec::with_capacity(HEADER + checked.len());
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, VERSION);
    put_u32(&mut out, SECTIONS.len() as u32);
    out.extend_from_slice(&((HEADER + checked.len()) as u64).to_le_bytes());
    out.extend_from_slice(&ModelKey::from_content(&checked).get().to_le_bytes());
    out.extend_from_slice(&checked);
    out
}

/// The header, checksum and section table of a v1 blob, checked whole: its sections, in id order.
fn validated_sections(bytes: &[u8]) -> Result<Vec<&[u8]>, LoadError> {
    let header = bytes.get(..HEADER).ok_or_else(|| bad(format!("blob of {} bytes has no header", bytes.len())))?;
    let mut h = Reader::new(header, "header");
    if h.take::<8>()? != *MAGIC {
        return Err(bad("not a phasekit blob".into()));
    }
    let version = h.u32()?;
    if version != VERSION {
        return Err(bad(format!("blob version {version}; this build reads version {VERSION} (regenerate the pack)")));
    }
    let (count, length, checksum) = (h.u32()? as usize, h.u64()?, h.u64()?);
    if length != bytes.len() as u64 {
        return Err(bad(format!("blob says {length} bytes, has {}", bytes.len())));
    }
    if count != SECTIONS.len() {
        return Err(bad(format!("blob lists {count} sections; version {VERSION} has {}", SECTIONS.len())));
    }
    let checked = bytes.get(HEADER..).unwrap_or_default();
    if ModelKey::from_content(checked).get() != checksum {
        return Err(bad("blob checksum mismatch".into()));
    }
    let mut table = Reader::new(bytes.get(HEADER..HEADER + ENTRY * count).unwrap_or_default(), "section table");
    let mut next = HEADER + ENTRY * count;
    let mut sections: Vec<&[u8]> = Vec::with_capacity(count);
    for &(id, name, lands_at) in &SECTIONS {
        let (got, zero, offset, len) = (table.u32()?, table.u32()?, table.u64()?, table.u64()?);
        let start = next.next_multiple_of(8);
        let padding = bytes.get(next..start).unwrap_or_default();
        if got != id || zero != 0 || offset != start as u64 || padding.iter().any(|&b| b != 0) {
            return Err(bad(format!("section table: entry {got} at {offset} where section {id} ({name}) belongs")));
        }
        let end = usize::try_from(len).ok().and_then(|len| start.checked_add(len));
        let section = end.and_then(|end| bytes.get(start..end));
        let section = section.ok_or_else(|| bad(format!("section table: {name} runs past the blob")))?;
        if let (Some(step), false) = (lands_at, section.is_empty()) {
            return Err(bad(format!("the {name} section lands at {step}")));
        }
        next = start + section.len();
        sections.push(section);
    }
    if next != bytes.len() {
        return Err(bad(format!("blob has {} bytes after its last section", bytes.len() - next)));
    }
    Ok(sections)
}

/// Validates a v1 blob whole (header, checksum, section table), then decodes every section.
pub(crate) fn decode(bytes: &[u8]) -> Result<FluidRecord, LoadError> {
    let mut record = decode_eager_parts(bytes)?;
    record.superancillary = read_superancillary(validated_sections(bytes)?[3])?;
    Ok(record)
}

/// [`decode`] without the superancillary, which the registry decodes on first saturation use
/// ([`decode_superancillary`]); the superancillary is most of a fluid's bytes (map 09).
pub(crate) fn decode_eager_parts(bytes: &[u8]) -> Result<FluidRecord, LoadError> {
    let sections = validated_sections(bytes)?;
    let eos = EosRecord::decode(sections[1])?;
    let mut record = read_metadata(sections[0], eos)?;
    record.superancillary_fit = read_fit(sections[2])?;
    record.caloric = read_caloric(sections[4], record.superancillary_fit)?;
    record.melting = read_melting(sections[8])?;
    record.corrections = read_corrections(sections[9])?;
    Ok(record)
}

/// The superancillary of a v1 blob, validated whole again; `None` for a fluid without one.
pub(crate) fn decode_superancillary(bytes: &[u8]) -> Result<Option<Superancillary>, LoadError> {
    read_superancillary(validated_sections(bytes)?[3])
}

/// The first eight bytes of every pack.
const PACK_MAGIC: &[u8; 8] = b"PKITPACK";

/// One fluid of a pack: its names (canonical first, then aliases), the canonical names of its references, its blob.
pub type PackFluid = (Vec<String>, Vec<String>, Arc<[u8]>);

/// The v1 pack of `fluids`.
pub(crate) fn pack(fluids: &[PackFluid]) -> Vec<u8> {
    let mut body = Vec::new();
    for (names, requires, blob) in fluids {
        for list in [names, requires] {
            put_u32(&mut body, list.len() as u32);
            list.iter().for_each(|n| put_str(&mut body, n));
        }
        body.extend_from_slice(&(blob.len() as u64).to_le_bytes());
        let padding = (HEADER + body.len()).next_multiple_of(8) - (HEADER + body.len());
        body.resize(body.len() + padding, 0);
        body.extend_from_slice(blob);
    }
    let mut out = Vec::with_capacity(HEADER + body.len());
    out.extend_from_slice(PACK_MAGIC);
    put_u32(&mut out, PACK_VERSION);
    put_u32(&mut out, fluids.len() as u32);
    out.extend_from_slice(&((HEADER + body.len()) as u64).to_le_bytes());
    out.extend_from_slice(&ModelKey::from_content(&body).get().to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Reads a v1 pack: header and checksum, then each fluid's names, references and blob, every blob validated whole.
pub(crate) fn unpack(bytes: &[u8]) -> Result<Vec<PackFluid>, LoadError> {
    let header = bytes.get(..HEADER).ok_or_else(|| bad(format!("pack of {} bytes has no header", bytes.len())))?;
    let mut h = Reader::new(header, "pack header");
    if h.take::<8>()? != *PACK_MAGIC {
        return Err(bad("not a phasekit pack".into()));
    }
    let version = h.u32()?;
    if version != PACK_VERSION {
        return Err(bad(format!(
            "pack version {version}; this build reads version {PACK_VERSION} (regenerate the pack)"
        )));
    }
    let (count, length, checksum) = (h.u32()?, h.u64()?, h.u64()?);
    if length != bytes.len() as u64 {
        return Err(bad(format!("pack says {length} bytes, has {}", bytes.len())));
    }
    let body = bytes.get(HEADER..).unwrap_or_default();
    if ModelKey::from_content(body).get() != checksum {
        return Err(bad("pack checksum mismatch".into()));
    }
    let mut r = Reader::new(body, "pack");
    let mut fluids = Vec::new();
    for _ in 0..count {
        let mut lists = [Vec::new(), Vec::new()];
        for list in &mut lists {
            *list = (0..r.u32()?).map(|_| r.str()).collect::<Result<_, _>>()?;
        }
        let len = usize::try_from(r.u64()?).map_err(|_| bad("pack: a blob length overflows".into()))?;
        let padding = (HEADER + r.at).next_multiple_of(8) - (HEADER + r.at);
        let start = r.at + padding;
        let blob = start.checked_add(len).and_then(|end| body.get(start..end));
        let blob = blob.ok_or_else(|| bad("pack is truncated".into()))?;
        if body.get(r.at..start).is_some_and(|pad| pad.iter().any(|&b| b != 0)) {
            return Err(bad("pack: nonzero padding".into()));
        }
        r.at = start + len;
        validated_sections(blob)?;
        let [names, requires] = lists;
        if names.is_empty() {
            return Err(bad("pack: a fluid without a name".into()));
        }
        fluids.push((names, requires, Arc::from(blob)));
    }
    r.done()?;
    Ok(fluids)
}
