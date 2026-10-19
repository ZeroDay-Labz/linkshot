//! The recording spool (`.lsv`): a lossless, append-only sequence of RGBA frames.
//!
//! Each record is either a *key frame* (the raw RGBA buffer, LZ4 compressed) or a *delta* (the XOR
//! of the frame against the previous one, LZ4 compressed; unchanged pixels become zero bytes and
//! compress to almost nothing, which is what screen recordings mostly consist of). A key frame is
//! forced every [`KEY_INTERVAL`] frames so seeking stays cheap. `finish` appends an index and a
//! footer; a spool that was never finished (crash, power loss) is still readable because every
//! record carries its own length and the reader rebuilds the index by scanning.
//!
//! Layout: `"LSV1" width:u32 height:u32 fps_hint:u32 reserved:u32` then records
//! `kind:u8 pts_ms:u32 len:u32 payload[len]`, then the index `count:u32 {pts:u32 offset:u64 len:u32
//! kind:u8}*count` and the footer `index_offset:u64 "LSVI"`. All integers little endian.

use crate::source::FrameSource;
use anyhow::{bail, ensure, Context, Result};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"LSV1";
const INDEX_MAGIC: &[u8; 4] = b"LSVI";
const HEADER_LEN: u64 = 20;
const RECORD_HEADER_LEN: u64 = 9;
/// A key frame at least this often (frames).
/// Bytes of one RGBA frame, refusing absurd sizes from a damaged header (> 1 GiB).
fn frame_len(width: u32, height: u32) -> Result<usize> {
    let n = width as u64 * height as u64 * 4;
    ensure!(width > 0 && height > 0 && n <= 1 << 30, "frame size {width}x{height} out of range");
    Ok(n as usize)
}

pub const KEY_INTERVAL: u32 = 90;
const KIND_KEY: u8 = 0;
const KIND_DELTA: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameIndex {
    pub pts_ms: u32,
    /// Offset of the record header.
    pub offset: u64,
    pub len: u32,
    pub key: bool,
}

/// Summary returned by [`SpoolWriter::finish`].
#[derive(Clone, Debug)]
pub struct SpoolInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frames: usize,
    pub duration_ms: u32,
    pub bytes: u64,
}

pub struct SpoolWriter {
    out: BufWriter<File>,
    path: PathBuf,
    width: u32,
    height: u32,
    fps_hint: u32,
    prev: Vec<u8>,
    scratch: Vec<u8>,
    comp: Vec<u8>,
    index: Vec<FrameIndex>,
    since_key: u32,
    pos: u64,
}

impl SpoolWriter {
    pub fn create(path: &Path, width: u32, height: u32, fps_hint: u32) -> Result<Self> {
        ensure!(width > 0 && height > 0, "empty frame size");
        if let Some(p) = path.parent() { std::fs::create_dir_all(p).ok(); }
        let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut out = BufWriter::with_capacity(1 << 20, file);
        out.write_all(MAGIC)?;
        out.write_all(&width.to_le_bytes())?;
        out.write_all(&height.to_le_bytes())?;
        out.write_all(&fps_hint.to_le_bytes())?;
        out.write_all(&0u32.to_le_bytes())?;
        Ok(Self { out, path: path.to_path_buf(), width, height, fps_hint, prev: vec![], scratch: vec![], comp: vec![], index: vec![], since_key: 0, pos: HEADER_LEN })
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn frames(&self) -> usize { self.index.len() }
    pub fn last_pts(&self) -> Option<u32> { self.index.last().map(|f| f.pts_ms) }
    pub fn bytes_written(&self) -> u64 { self.pos }
    pub fn path(&self) -> &Path { &self.path }

    /// Appends a frame. `rgba` must be `width * height * 4` bytes; timestamps must not decrease.
    pub fn push(&mut self, pts_ms: u32, rgba: &[u8]) -> Result<()> {
        let frame_len = frame_len(self.width, self.height)?;
        ensure!(rgba.len() == frame_len, "frame is {} bytes, expected {}", rgba.len(), frame_len);
        if let Some(last) = self.last_pts() { ensure!(pts_ms >= last, "timestamps must not decrease ({pts_ms} < {last})"); }
        let key = self.prev.is_empty() || self.since_key >= KEY_INTERVAL;
        let src: &[u8] = if key {
            rgba
        } else {
            self.scratch.resize(frame_len, 0);
            xor_into(&mut self.scratch, &self.prev, rgba);
            &self.scratch
        };
        let max = lz4_flex::block::get_maximum_output_size(src.len());
        if self.comp.len() < max { self.comp.resize(max, 0); }
        let n = lz4_flex::block::compress_into(src, &mut self.comp).map_err(|e| anyhow::anyhow!("lz4: {e}"))?;
        let kind = if key { KIND_KEY } else { KIND_DELTA };
        self.out.write_all(&[kind])?;
        self.out.write_all(&pts_ms.to_le_bytes())?;
        self.out.write_all(&(n as u32).to_le_bytes())?;
        self.out.write_all(&self.comp[..n])?;
        self.index.push(FrameIndex { pts_ms, offset: self.pos, len: n as u32, key });
        self.pos += RECORD_HEADER_LEN + n as u64;
        self.since_key = if key { 1 } else { self.since_key + 1 };
        if self.prev.len() != frame_len { self.prev.resize(frame_len, 0); }
        self.prev.copy_from_slice(rgba);
        Ok(())
    }

    /// Writes the index and footer and closes the file.
    pub fn finish(mut self) -> Result<SpoolInfo> {
        let index_offset = self.pos;
        self.out.write_all(&(self.index.len() as u32).to_le_bytes())?;
        for f in &self.index {
            self.out.write_all(&f.pts_ms.to_le_bytes())?;
            self.out.write_all(&f.offset.to_le_bytes())?;
            self.out.write_all(&f.len.to_le_bytes())?;
            self.out.write_all(&[if f.key { KIND_KEY } else { KIND_DELTA }])?;
        }
        self.out.write_all(&index_offset.to_le_bytes())?;
        self.out.write_all(INDEX_MAGIC)?;
        self.out.flush()?;
        let bytes = self.out.get_ref().metadata().map(|m| m.len()).unwrap_or(self.pos);
        let duration_ms = duration_from_index(&self.index, self.fps_hint);
        Ok(SpoolInfo { path: self.path, width: self.width, height: self.height, frames: self.index.len(), duration_ms, bytes })
    }

    /// Deletes the file (cancelled recording).
    pub fn abort(self) {
        let path = self.path.clone();
        drop(self);
        let _ = std::fs::remove_file(path);
    }
}

fn xor_into(dst: &mut [u8], a: &[u8], b: &[u8]) {
    // Chunked so the compiler vectorises it.
    let n = dst.len().min(a.len()).min(b.len());
    let (dc, ac, bc) = (dst[..n].chunks_exact_mut(8), a[..n].chunks_exact(8), b[..n].chunks_exact(8));
    for ((d, x), y) in dc.zip(ac).zip(bc) {
        let v = u64::from_ne_bytes(x.try_into().unwrap()) ^ u64::from_ne_bytes(y.try_into().unwrap());
        d.copy_from_slice(&v.to_ne_bytes());
    }
    let tail = n - n % 8;
    for i in tail..n { dst[i] = a[i] ^ b[i]; }
}

fn duration_from_index(index: &[FrameIndex], fps_hint: u32) -> u32 {
    match index.len() {
        0 => 0,
        1 => index[0].pts_ms + if fps_hint > 0 { 1000 / fps_hint } else { 100 },
        n => {
            let last = index[n - 1].pts_ms;
            let avg = last.saturating_sub(index[0].pts_ms) / (n as u32 - 1);
            last + avg.max(1)
        }
    }
}

pub struct SpoolReader {
    file: BufReader<File>,
    path: PathBuf,
    width: u32,
    height: u32,
    fps_hint: u32,
    index: Vec<FrameIndex>,
    duration_ms: u32,
    cur: Option<usize>,
    buf: Vec<u8>,
    comp: Vec<u8>,
    scratch: Vec<u8>,
}

impl SpoolReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let total = file.metadata()?.len();
        let mut file = BufReader::with_capacity(1 << 20, file);
        let mut header = [0u8; HEADER_LEN as usize];
        file.read_exact(&mut header).context("spool header")?;
        ensure!(&header[0..4] == MAGIC, "{} is not a linkshot recording", path.display());
        let width = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let height = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let fps_hint = u32::from_le_bytes(header[12..16].try_into().unwrap());
        ensure!(width > 0 && height > 0 && (width as u64 * height as u64) < (1u64 << 31), "bad frame size {width}x{height}");
        let index = match read_index(&mut file, total) {
            Ok(idx) => idx,
            Err(e) => {
                tracing::warn!("{}: no index ({e}); scanning records", path.display());
                scan_records(&mut file, total)?
            }
        };
        let duration_ms = duration_from_index(&index, fps_hint);
        Ok(Self { file, path: path.to_path_buf(), width, height, fps_hint, index, duration_ms, cur: None, buf: vec![], comp: vec![], scratch: vec![] })
    }

    pub fn path(&self) -> &Path { &self.path }
    pub fn fps_hint(&self) -> u32 { self.fps_hint }
    pub fn index(&self) -> &[FrameIndex] { &self.index }

    fn read_record(&mut self, i: usize) -> Result<bool> {
        let f = self.index[i];
        self.file.seek(SeekFrom::Start(f.offset))?;
        let mut h = [0u8; RECORD_HEADER_LEN as usize];
        self.file.read_exact(&mut h)?;
        let len = u32::from_le_bytes(h[5..9].try_into().unwrap()) as usize;
        ensure!(len == f.len as usize, "record {i}: length mismatch");
        self.comp.resize(len, 0);
        self.file.read_exact(&mut self.comp)?;
        let frame_len = frame_len(self.width, self.height)?;
        self.scratch.resize(frame_len, 0);
        let n = lz4_flex::block::decompress_into(&self.comp, &mut self.scratch).map_err(|e| anyhow::anyhow!("record {i}: lz4: {e}"))?;
        ensure!(n == frame_len, "record {i}: decoded {n} bytes, expected {frame_len}");
        Ok(h[0] == KIND_KEY)
    }

    fn load(&mut self, i: usize) -> Result<()> {
        ensure!(i < self.index.len(), "frame {i} out of range ({} frames)", self.index.len());
        if self.cur == Some(i) { return Ok(()); }
        let start = match self.cur {
            Some(c) if c + 1 == i && !self.index[i].key => i,
            Some(c) if c < i && !(c + 1..=i).any(|k| self.index[k].key) => c + 1,
            _ => (0..=i).rev().find(|k| self.index[*k].key).unwrap_or(0),
        };
        let frame_len = frame_len(self.width, self.height)?;
        for k in start..=i {
            let key = self.read_record(k)?;
            if key || self.buf.len() != frame_len {
                self.buf.clear();
                self.buf.extend_from_slice(&self.scratch);
            } else {
                let (buf, scratch) = (&mut self.buf, &self.scratch);
                for (d, s) in buf.chunks_exact_mut(8).zip(scratch.chunks_exact(8)) {
                    let v = u64::from_ne_bytes(d.as_ref().try_into().unwrap()) ^ u64::from_ne_bytes(s.try_into().unwrap());
                    d.copy_from_slice(&v.to_ne_bytes());
                }
            }
        }
        self.cur = Some(i);
        Ok(())
    }
}

fn read_index(file: &mut BufReader<File>, total: u64) -> Result<Vec<FrameIndex>> {
    ensure!(total >= HEADER_LEN + 12, "file too short");
    file.seek(SeekFrom::Start(total - 12))?;
    let mut footer = [0u8; 12];
    file.read_exact(&mut footer)?;
    ensure!(&footer[8..12] == INDEX_MAGIC, "no footer");
    let index_offset = u64::from_le_bytes(footer[0..8].try_into().unwrap());
    ensure!(index_offset >= HEADER_LEN && index_offset < total - 12, "bad index offset");
    file.seek(SeekFrom::Start(index_offset))?;
    let mut n = [0u8; 4];
    file.read_exact(&mut n)?;
    let count = u32::from_le_bytes(n) as usize;
    ensure!((count as u64) * 17 + 4 + 12 == total - index_offset, "index size mismatch");
    let mut out = Vec::with_capacity(count);
    let mut rec = [0u8; 17];
    for _ in 0..count {
        file.read_exact(&mut rec)?;
        out.push(FrameIndex {
            pts_ms: u32::from_le_bytes(rec[0..4].try_into().unwrap()),
            offset: u64::from_le_bytes(rec[4..12].try_into().unwrap()),
            len: u32::from_le_bytes(rec[12..16].try_into().unwrap()),
            key: rec[16] == KIND_KEY,
        });
    }
    Ok(out)
}

fn scan_records(file: &mut BufReader<File>, total: u64) -> Result<Vec<FrameIndex>> {
    let mut out = vec![];
    let mut pos = HEADER_LEN;
    file.seek(SeekFrom::Start(pos))?;
    let mut h = [0u8; RECORD_HEADER_LEN as usize];
    while pos + RECORD_HEADER_LEN <= total {
        file.read_exact(&mut h)?;
        let kind = h[0];
        if kind != KIND_KEY && kind != KIND_DELTA { break; }
        let pts = u32::from_le_bytes(h[1..5].try_into().unwrap());
        let len = u32::from_le_bytes(h[5..9].try_into().unwrap());
        if pos + RECORD_HEADER_LEN + len as u64 > total { break; } // truncated last record
        out.push(FrameIndex { pts_ms: pts, offset: pos, len, key: kind == KIND_KEY });
        pos += RECORD_HEADER_LEN + len as u64;
        file.seek(SeekFrom::Start(pos))?;
    }
    if out.is_empty() { bail!("no frames"); }
    Ok(out)
}

impl FrameSource for SpoolReader {
    fn width(&self) -> u32 { self.width }
    fn height(&self) -> u32 { self.height }
    fn len(&self) -> usize { self.index.len() }
    fn pts_ms(&self, i: usize) -> u32 { self.index[i].pts_ms }
    fn duration_ms(&self) -> u32 { self.duration_ms }
    fn frame(&mut self, i: usize) -> Result<&[u8]> {
        self.load(i)?;
        Ok(&self.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, seed: u32) -> Vec<u8> {
        let mut v = vec![0u8; (w * h * 4) as usize];
        for (i, px) in v.chunks_exact_mut(4).enumerate() {
            let x = (i as u32 % w) ^ seed;
            px.copy_from_slice(&[(x & 255) as u8, ((i as u32 / w) & 255) as u8, (seed * 37 & 255) as u8, 255]);
        }
        v
    }

    #[test]
    fn roundtrip_random_access_and_scan() {
        let dir = std::env::temp_dir().join(format!("linkshot-spool-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.lsv");
        let (w, h) = (64, 48);
        let n = (KEY_INTERVAL + 5) as usize;
        let mut wr = SpoolWriter::create(&path, w, h, 30).unwrap();
        for i in 0..n { wr.push(i as u32 * 33, &frame(w, h, if i % 7 == 0 { i as u32 } else { (i / 7) as u32 })).unwrap(); }
        let info = wr.finish().unwrap();
        assert_eq!(info.frames, n);
        let mut rd = SpoolReader::open(&path).unwrap();
        assert_eq!(rd.len(), n);
        assert_eq!(rd.index_at(33 * 5 + 10), 5);
        for i in [0usize, 3, 2, 92, 91, 1, n - 1, 50] {
            let expect = frame(w, h, if i % 7 == 0 { i as u32 } else { (i / 7) as u32 });
            assert_eq!(rd.frame(i).unwrap(), &expect[..], "frame {i}");
        }
        // Truncate the footer: the reader must rebuild the index by scanning.
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 40]).unwrap();
        let mut rd = SpoolReader::open(&path).unwrap();
        assert_eq!(rd.len(), n);
        assert_eq!(rd.frame(7).unwrap(), &frame(w, h, 7)[..]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
