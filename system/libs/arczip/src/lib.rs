//! Reading archives: DEFLATE (RFC 1951) with its gzip (RFC 1952) and zlib
//! (RFC 1950) wrappers, tar (ustar and GNU long names) and zip.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn need(&mut self, n: u32) -> Option<()> {
        while self.nbits < n {
            let b = *self.data.get(self.pos)?;
            self.pos += 1;
            self.bit |= (b as u32) << self.nbits;
            self.nbits += 8;
        }
        Some(())
    }

    fn get(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Some(v)
    }

    fn align(&mut self) {
        let drop = self.nbits % 8;
        self.bit >>= drop;
        self.nbits -= drop;
    }
}

/// A canonical Huffman code: counts per length and symbols by code.
struct Huff {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huff {
    fn new(lengths: &[u8]) -> Option<Huff> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Some(Huff { counts, symbols })
    }

    fn decode(&self, b: &mut Bits<'_>) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return self.symbols.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

const LBASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DEXT: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Raw DEFLATE; returns the data and how many input bytes it used.
pub fn inflate_raw(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut b = Bits { data, pos: 0, bit: 0, nbits: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 3);
    loop {
        let last = b.get(1)?;
        match b.get(2)? {
            0 => {
                b.align();
                let len = b.get(16)? as usize;
                let nlen = b.get(16)? as usize;
                if len != !nlen & 0xffff {
                    return None;
                }
                // Whole bytes now: the bit buffer is empty after align+16+16.
                let start = b.pos;
                out.extend_from_slice(data.get(start..start + len)?);
                b.pos += len;
            }
            kind @ (1 | 2) => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [8u8; 288];
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    (Huff::new(&l)?, Huff::new(&[5u8; 30])?)
                } else {
                    let hlit = b.get(5)? as usize + 257;
                    let hdist = b.get(5)? as usize + 1;
                    let hclen = b.get(4)? as usize + 4;
                    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORDER.iter().take(hclen) {
                        cl[o] = b.get(3)? as u8;
                    }
                    let ch = Huff::new(&cl)?;
                    let mut lens = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < hlit + hdist {
                        let sym = ch.decode(&mut b)?;
                        match sym {
                            0..=15 => {
                                lens[i] = sym as u8;
                                i += 1;
                            }
                            16 => {
                                let prev = *lens.get(i.checked_sub(1)?)?;
                                for _ in 0..3 + b.get(2)? {
                                    *lens.get_mut(i)? = prev;
                                    i += 1;
                                }
                            }
                            17 => i += 3 + b.get(3)? as usize,
                            _ => i += 11 + b.get(7)? as usize,
                        }
                    }
                    if i > hlit + hdist {
                        return None;
                    }
                    (Huff::new(&lens[..hlit])?, Huff::new(&lens[hlit..])?)
                };
                loop {
                    let sym = lit.decode(&mut b)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let i = (sym - 257) as usize;
                        let len = *LBASE.get(i)? as usize + b.get(*LEXT.get(i)? as u32)? as usize;
                        let d = dist.decode(&mut b)? as usize;
                        let dd = *DBASE.get(d)? as usize + b.get(*DEXT.get(d)? as u32)? as usize;
                        if dd > out.len() {
                            return None;
                        }
                        let start = out.len() - dd;
                        for k in 0..len {
                            let v = out[start + k];
                            out.push(v);
                        }
                    }
                }
            }
            _ => return None,
        }
        if last == 1 {
            break;
        }
    }
    // Bytes held in the bit buffer were not used.
    Some((out, b.pos - (b.nbits / 8) as usize))
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// A gzip file (one or more members), checked with its CRC.
pub fn gunzip(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut d = data;
    while d.len() >= 18 && d[0] == 0x1f && d[1] == 0x8b {
        if d[2] != 8 {
            return None;
        }
        let flags = d[3];
        let mut i = 10;
        if flags & 4 != 0 {
            let xlen = u16::from_le_bytes([*d.get(i)?, *d.get(i + 1)?]) as usize;
            i += 2 + xlen;
        }
        for bit in [8u8, 16] {
            if flags & bit != 0 {
                while *d.get(i)? != 0 {
                    i += 1;
                }
                i += 1;
            }
        }
        if flags & 2 != 0 {
            i += 2;
        }
        let (member, used) = inflate_raw(d.get(i..)?)?;
        let t = i + used;
        let crc = u32::from_le_bytes(d.get(t..t + 4)?.try_into().ok()?);
        if crc != crc32(&member) {
            return None;
        }
        out.extend(member);
        d = &d[t + 8..];
    }
    if out.is_empty() && !data.is_empty() {
        return None;
    }
    Some(out)
}

/// A zlib stream.
pub fn zlib(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 2 || data[0] & 0x0f != 8 || (u16::from_be_bytes([data[0], data[1]]) % 31) != 0 {
        // Some servers send raw DEFLATE as "deflate".
        return inflate_raw(data).map(|(d, _)| d);
    }
    inflate_raw(&data[2..]).map(|(d, _)| d)
}

/// One file in an archive.
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
    pub dir: bool,
    /// Unix permission bits (0 if the archive has none).
    pub mode: u32,
    /// For a symbolic link: its target.
    pub link: Option<String>,
}

fn octal(b: &[u8]) -> u64 {
    b.iter().take_while(|&&c| c != 0 && c != b' ').fold(0, |acc, &c| if c.is_ascii_digit() { acc * 8 + (c - b'0') as u64 } else { acc })
}

/// The files of a tar archive.
pub fn untar(data: &[u8]) -> Option<Vec<Entry>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut long_name: Option<String> = None;
    while i + 512 <= data.len() {
        let h = &data[i..i + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let size = octal(&h[124..136]) as usize;
        let kind = h[156];
        let mut name = String::from_utf8_lossy(&h[..100]).trim_end_matches('\0').into();
        if &h[257..262] == b"ustar" {
            let prefix: String = String::from_utf8_lossy(&h[345..500]).trim_end_matches('\0').into();
            if !prefix.is_empty() {
                name = alloc::format!("{prefix}/{name}");
            }
        }
        if let Some(n) = long_name.take() {
            name = n;
        }
        let body = data.get(i + 512..i + 512 + size)?;
        i += 512 + size.div_ceil(512) * 512;
        match kind {
            b'L' => {
                long_name = Some(String::from_utf8_lossy(body).trim_end_matches('\0').into());
            }
            b'x' | b'g' => {
                // pax: a "path" record replaces the name.
                for rec in String::from_utf8_lossy(body).lines() {
                    if let Some((_, kv)) = rec.split_once(' ') {
                        if let Some(p) = kv.strip_prefix("path=") {
                            if kind == b'x' {
                                long_name = Some(p.into());
                            }
                        }
                    }
                }
            }
            b'0' | 0 | b'5' | b'2' => out.push(Entry {
                name,
                data: if kind == b'5' || kind == b'2' { Vec::new() } else { body.to_vec() },
                dir: kind == b'5',
                mode: octal(&h[100..108]) as u32,
                link: (kind == b'2').then(|| String::from_utf8_lossy(&h[157..257]).trim_end_matches('\0').into()),
            }),
            _ => {}
        }
    }
    Some(out)
}

/// The files of a zip archive (stored or deflated).
pub fn unzip(data: &[u8]) -> Option<Vec<Entry>> {
    let le16 = |o: usize| -> Option<usize> { Some(u16::from_le_bytes(data.get(o..o + 2)?.try_into().ok()?) as usize) };
    let le32 = |o: usize| -> Option<usize> { Some(u32::from_le_bytes(data.get(o..o + 4)?.try_into().ok()?) as usize) };
    // The end of central directory record, near the end.
    let from = data.len().saturating_sub(65536 + 22);
    let eocd = (from..data.len().saturating_sub(21)).rev().find(|&i| data[i..i + 4] == [0x50, 0x4b, 5, 6])?;
    let count = le16(eocd + 10)?;
    let mut p = le32(eocd + 16)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if data.get(p..p + 4)? != [0x50, 0x4b, 1, 2] {
            return None;
        }
        let method = le16(p + 10)?;
        let csize = le32(p + 20)?;
        let nlen = le16(p + 28)?;
        let elen = le16(p + 30)?;
        let clen = le16(p + 32)?;
        let attrs = le32(p + 38)?;
        let local = le32(p + 42)?;
        let name: String = String::from_utf8_lossy(data.get(p + 46..p + 46 + nlen)?).into();
        p += 46 + nlen + elen + clen;
        let lnlen = le16(local + 26)?;
        let lelen = le16(local + 28)?;
        let start = local + 30 + lnlen + lelen;
        let raw = data.get(start..start + csize)?;
        let dir = name.ends_with('/');
        let body = match method {
            0 => raw.to_vec(),
            8 => inflate_raw(raw)?.0,
            _ => return None,
        };
        let mode = (attrs >> 16) as u32 & 0o7777;
        let link = ((attrs >> 16) as u32 & 0o170000 == 0o120000).then(|| String::from_utf8_lossy(&body).into());
        out.push(Entry { name, data: if link.is_some() { Vec::new() } else { body }, dir, mode, link });
    }
    Some(out)
}

#[cfg(test)]
mod tests;
