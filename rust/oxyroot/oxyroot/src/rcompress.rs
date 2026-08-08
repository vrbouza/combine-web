use flate2::read::ZlibDecoder;
use std::io::{ErrorKind, Read};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

// Note: this contains ZL[src][dst] where src and dst are 3 bytes each.
const HEADER_SIZE: usize = 9;

// because each zipped block contains:
// - the size of the input data
// - the size of the compressed data
// where each size is saved on 3 bytes, the maximal size
// of each block can not be bigger than 16Mb.
#[allow(dead_code)]
const K_MAX_COMPRESSED_BLOCK_SIZE: usize = 0xffffff;

#[allow(dead_code)]
#[derive(PartialEq)]
enum Kind {
    #[allow(dead_code)]
    Inherit = -1,
    UseGlobal = 0,
    Zlib = 1,
    Lzma = 2,
    OldCompression = 3,
    LZ4 = 4,
    Zstd = 5,
    UndefinedCompression = 6,
}

// kindOf returns the kind of compression algorithm.
fn kind_of(buf: &[u8]) -> Kind {
    match (buf[0] as char, buf[1] as char) {
        ('Z', 'L') => Kind::Zlib,
        ('X', 'Z') => Kind::Lzma,
        ('L', '4') => Kind::LZ4,
        ('Z', 'S') => Kind::Zstd,
        ('C', 'S') => Kind::OldCompression,

        _ => Kind::UndefinedCompression,
    }
}

fn invalid(what: &str) -> Error {
    Error::Io(std::io::Error::new(
        ErrorKind::InvalidData,
        format!("rcompress: {what}"),
    ))
}

/// Decompress a ROOT buffer into `dst`, returning the number of bytes written.
///
/// A ROOT compressed buffer is a *sequence* of blocks, each prefixed by a 9-byte
/// header carrying the algorithm tag and the compressed/uncompressed sizes. Payloads
/// larger than 16 MiB uncompressed are split across several blocks, so this must loop
/// until `dst` is filled rather than stopping after the first block.
pub fn decompress(dst: &mut [u8], mut src: &[u8]) -> Result<usize> {
    let mut hdr = [0_u8; HEADER_SIZE];
    let mut written = 0usize;

    while written < dst.len() {
        if src.len() < HEADER_SIZE {
            break;
        }
        src.read_exact(&mut hdr)?;

        let srcsz = (hdr[3] as usize) | (hdr[4] as usize) << 8 | (hdr[5] as usize) << 16;
        let tgtsz = (hdr[6] as usize) | (hdr[7] as usize) << 8 | (hdr[8] as usize) << 16;

        if srcsz > src.len() {
            return Err(invalid("block claims more compressed bytes than remain"));
        }

        let (payload, rest) = src.split_at(srcsz);
        let end = (written + tgtsz).min(dst.len());
        let out = &mut dst[written..end];

        match kind_of(hdr.as_ref()) {
            Kind::Zlib => {
                let mut d = ZlibDecoder::new(payload);
                d.read_exact(out)?;
            }
            Kind::Lzma => {
                let mut buf = Vec::with_capacity(out.len());
                let mut inp = std::io::BufReader::new(payload);
                lzma_rs::xz_decompress(&mut inp, &mut buf).map_err(|_| invalid("xz"))?;
                let n = buf.len().min(out.len());
                out[..n].copy_from_slice(&buf[..n]);
            }
            Kind::LZ4 => {
                // ROOT prefixes each LZ4 block with an 8-byte xxhash64 checksum,
                // which is not part of the LZ4 block itself.
                if payload.len() < 8 {
                    return Err(invalid("lz4 block shorter than its checksum"));
                }
                lz4_flex::block::decompress_into(&payload[8..], out).map_err(|_| invalid("lz4"))?;
            }
            Kind::Zstd => {
                let mut d = ruzstd::StreamingDecoder::new(payload).map_err(|_| invalid("zstd"))?;
                d.read_exact(out)?;
            }
            Kind::OldCompression => return Err(invalid("unsupported codec: old ROOT compression")),
            Kind::Inherit | Kind::UseGlobal => return Err(invalid("unresolved compression kind")),
            Kind::UndefinedCompression => return Err(invalid("unknown compression tag")),
        }

        src = rest;
        written = end;
    }

    Ok(written)
}

fn root_compress_algo_level(algo: i32) -> (Kind, i32) {
    let kind = algo / 100;
    let level = algo % 100;
    let kind = match kind {
        0 => Kind::UseGlobal,
        1 => Kind::Zlib,
        2 => Kind::Lzma,
        3 => Kind::OldCompression,
        4 => Kind::LZ4,
        5 => Kind::Zstd,
        _ => Kind::UndefinedCompression,
    };
    (kind, level)
}

pub fn compress(src: Vec<u8>, compression: i32) -> Result<Vec<u8>> {
    assert_eq!(compression, 1);

    let (kind, level) = root_compress_algo_level(compression);

    if kind == Kind::UseGlobal || level == 0 || src.len() < 512 {
        // no compression
        // let mut dst = Vec::new();
        //std::io::copy(src, &mut dst)?;
        return Ok(src);
    }

    unimplemented!();

    // let mut dst = Vec::new();
    // {
    //     let mut w = flate2::write::ZlibEncoder::new(&mut dst, flate2::Compression::default());
    //     std::io::copy(&mut src, &mut w)?;
    // }
    //
    // Ok(dst)
}
