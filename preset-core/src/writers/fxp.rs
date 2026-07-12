// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Generic VST2 FXP (.fxp) container writer.
//
// Ported from the legacy Python `generators/fxp_generator.py`. FXP files are a
// big-endian VST2 preset container with two flavours:
//
//   - `FxCk` "parameter" format: header + N big-endian float params.
//   - `FPCh` "chunk"    format: header + a length-prefixed opaque chunk. This is
//                         what plugins like Surge XT actually use (their patch
//                         format lives inside the chunk).
//
// Layout (all multi-byte integers big-endian, matching the VST2 `fxProgram`
// struct and the reference Python writer):
//
//   offset  size  field
//   0       4     chunkMagic  'CcnK'
//   4       4     byteSize    (of everything after this field)
//   8       4     fxMagic     'FxCk' | 'FPCh'
//   12      4     version
//   16      4     fxID        4-char plugin id
//   20      4     fxVersion
//   24      4     numParams (FxCk) | numPrograms (FPCh)
//   28      28    prgName     (null-padded ASCII)
//   56 ..         FxCk: numParams * 4 bytes of big-endian floats
//                 FPCh: 4-byte big-endian chunkSize, then chunkSize bytes
//
// The `byteSize` field, per the VST2 SDK and the reference generators, is the
// size of the file minus the first 8 bytes (magic + byteSize).

/// FXP container magic (`'CcnK'`).
pub const CHUNK_MAGIC: &[u8; 4] = b"CcnK";
/// FXP parameter-format magic (`'FxCk'`).
pub const FXP_PARAM_MAGIC: &[u8; 4] = b"FxCk";
/// FXP chunk-format magic (`'FPCh'`).
pub const FXP_CHUNK_MAGIC: &[u8; 4] = b"FPCh";
/// FXP format version written into the header.
pub const FORMAT_VERSION: u32 = 1;

/// Truncate + null-pad a preset name into the fixed 28-byte `prgName` field.
///
/// Non-ASCII characters are dropped (matching the Python `encode('ascii',
/// 'ignore')`), keeping the field a clean ASCII string.
pub fn prg_name_bytes(name: &str) -> [u8; 28] {
    let mut out = [0u8; 28];
    let ascii: Vec<u8> = name
        .bytes()
        .filter(|b| b.is_ascii() && *b != 0)
        .take(28)
        .collect();
    out[..ascii.len()].copy_from_slice(&ascii);
    out
}

/// The fixed 56-byte FXP header (magic → prgName), excluding the trailing body.
///
/// `body_len` is the number of bytes that will follow the header; it is used to
/// compute the `byteSize` field. `count` is `numParams` for `FxCk` or
/// `numPrograms` for `FPCh`.
pub fn write_header(
    out: &mut Vec<u8>,
    fx_magic: &[u8; 4],
    fx_id: &[u8; 4],
    fx_version: u32,
    count: u32,
    name: &str,
    body_len: usize,
) {
    out.extend_from_slice(CHUNK_MAGIC);
    // byteSize = everything after this field = 48 header-tail bytes + body.
    // (48 = fxMagic..prgName inclusive = 4+4+4+4+4+28.)
    let byte_size = 48u32 + body_len as u32;
    out.extend_from_slice(&byte_size.to_be_bytes());
    out.extend_from_slice(fx_magic);
    out.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    out.extend_from_slice(fx_id);
    out.extend_from_slice(&fx_version.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&prg_name_bytes(name));
}

/// Build a `FxCk` (parameter-format) FXP: header + big-endian float params.
///
/// This is the generic parameter chunk writer ported from the Python
/// `_create_generic_chunk` path (each float clamped to `[0, 1]`).
pub fn build_param_fxp(fx_id: &[u8; 4], fx_version: u32, name: &str, params: &[f32]) -> Vec<u8> {
    let body_len = params.len() * 4;
    let mut out = Vec::with_capacity(56 + body_len);
    write_header(
        &mut out,
        FXP_PARAM_MAGIC,
        fx_id,
        fx_version,
        params.len() as u32,
        name,
        body_len,
    );
    for p in params {
        let clamped = p.clamp(0.0, 1.0);
        out.extend_from_slice(&clamped.to_be_bytes());
    }
    out
}

/// Build a `FPCh` (chunk-format) FXP: header + `numPrograms` + length-prefixed
/// opaque chunk. This is the container Surge XT (and most modern plugins) use.
pub fn build_chunk_fxp(
    fx_id: &[u8; 4],
    fx_version: u32,
    name: &str,
    num_programs: u32,
    chunk: &[u8],
) -> Vec<u8> {
    // Body = 4-byte chunkSize + chunk bytes.
    let body_len = 4 + chunk.len();
    let mut out = Vec::with_capacity(56 + body_len);
    write_header(
        &mut out,
        FXP_CHUNK_MAGIC,
        fx_id,
        fx_version,
        num_programs,
        name,
        body_len,
    );
    out.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk);
    out
}

/// A parsed FXP header (fields common to both formats).
#[derive(Debug, Clone, PartialEq)]
pub struct FxpHeader {
    pub chunk_magic: [u8; 4],
    pub byte_size: u32,
    pub fx_magic: [u8; 4],
    pub version: u32,
    pub fx_id: [u8; 4],
    pub fx_version: u32,
    /// `numParams` for `FxCk`, `numPrograms` for `FPCh`.
    pub count: u32,
    pub name: String,
    /// For `FPCh`, the raw chunk bytes; for `FxCk`, empty (params live in `params`).
    pub chunk: Vec<u8>,
    /// For `FxCk`, the decoded float params; for `FPCh`, empty.
    pub params: Vec<f32>,
}

impl FxpHeader {
    /// Is this a `FPCh` (chunk) file?
    pub fn is_chunk_format(&self) -> bool {
        &self.fx_magic == FXP_CHUNK_MAGIC
    }
    /// Is this a `FxCk` (parameter) file?
    pub fn is_param_format(&self) -> bool {
        &self.fx_magic == FXP_PARAM_MAGIC
    }
}

/// Parse an FXP file's header (and body, for round-trip testing).
pub fn parse_fxp(data: &[u8]) -> Result<FxpHeader, String> {
    if data.len() < 56 {
        return Err("FXP too short (< 56 bytes)".into());
    }
    let mut chunk_magic = [0u8; 4];
    chunk_magic.copy_from_slice(&data[0..4]);
    if &chunk_magic != CHUNK_MAGIC {
        return Err("bad chunkMagic (expected CcnK)".into());
    }
    let byte_size = u32::from_be_bytes(data[4..8].try_into().unwrap());
    let mut fx_magic = [0u8; 4];
    fx_magic.copy_from_slice(&data[8..12]);
    let version = u32::from_be_bytes(data[12..16].try_into().unwrap());
    let mut fx_id = [0u8; 4];
    fx_id.copy_from_slice(&data[16..20]);
    let fx_version = u32::from_be_bytes(data[20..24].try_into().unwrap());
    let count = u32::from_be_bytes(data[24..28].try_into().unwrap());
    let name = {
        let raw = &data[28..56];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        String::from_utf8_lossy(&raw[..end]).into_owned()
    };

    let mut chunk = Vec::new();
    let mut params = Vec::new();
    if &fx_magic == FXP_CHUNK_MAGIC {
        if data.len() < 60 {
            return Err("FPCh missing chunkSize".into());
        }
        let chunk_size = u32::from_be_bytes(data[56..60].try_into().unwrap()) as usize;
        let start = 60;
        let end = start + chunk_size;
        if end > data.len() {
            return Err("FPCh chunkSize exceeds file".into());
        }
        chunk = data[start..end].to_vec();
    } else if &fx_magic == FXP_PARAM_MAGIC {
        let n = count as usize;
        let need = 56 + n * 4;
        if need > data.len() {
            return Err("FxCk param count exceeds file".into());
        }
        for i in 0..n {
            let off = 56 + i * 4;
            params.push(f32::from_be_bytes(data[off..off + 4].try_into().unwrap()));
        }
    } else {
        return Err(format!("unknown fxMagic {:?}", fx_magic));
    }

    Ok(FxpHeader {
        chunk_magic,
        byte_size,
        fx_magic,
        version,
        fx_id,
        fx_version,
        count,
        name,
        chunk,
        params,
    })
}
