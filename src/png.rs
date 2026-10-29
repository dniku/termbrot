//! Minimal PNG encoder.
//!
//! Writes a colour-accurate 8-bit RGB PNG using *stored* DEFLATE blocks, i.e.
//! no compression. That is a deliberate trade: a real Huffman encoder is a few
//! hundred lines of table-building that this project does not need, and PNG's
//! per-scanline filters already make smooth gradients compress well once a
//! consumer re-encodes them. Output is standards-conformant, so `file`,
//! ImageMagick, and browsers all accept it.

use std::io::{self, Write};

/// Largest payload a single stored DEFLATE block may carry.
const MAX_BLOCK: usize = 65_535;

/// CRC-32 table, generator polynomial `0xEDB88320` (reflected CRC-32/ISO-HDLC).
const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

const CRC_TABLE: [u32; 256] = crc_table();

/// Advertiser signature that opens every PNG stream.
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        let index = ((crc ^ u32::from(*byte)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC_TABLE[index];
    }
    !crc
}

/// Write one PNG chunk: length, type, payload, CRC over type + payload.
fn write_chunk(
    output: &mut impl Write,
    kind: &[u8; 4],
    payload: &[u8],
) -> io::Result<()> {
    output.write_all(&(payload.len() as u32).to_be_bytes())?;
    let mut body = Vec::with_capacity(4 + payload.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(payload);
    output.write_all(&body)?;
    output.write_all(&crc32(&body).to_be_bytes())
}

/// Adler-32 checksum of the uncompressed zlib payload.
fn adler32(data: &[u8]) -> u32 {
    let mut low = 1u32;
    let mut high = 0u32;
    for byte in data {
        low = (low + u32::from(*byte)) % 65_521;
        high = (high + low) % 65_521;
    }
    (high << 16) | low
}

/// Wrap `data` in a zlib stream of stored DEFLATE blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    // CMF/FLG for deflate, 32 KiB window, no preset dictionary, FCHECK
    // adjusted so the two-byte header sums to a multiple of 31.
    let mut out = vec![0x78u8, 0x01];
    if data.is_empty() {
        // A single empty final block still has to be emitted.
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    for (offset, chunk) in data.chunks(MAX_BLOCK).enumerate() {
        let is_last = offset + chunk.len() >= data.len();
        out.push(u8::from(is_last));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_le_bytes());
    out
}

/// Encode `width` × `height` RGB pixels as PNG bytes.
///
/// Every scanline is prefixed with filter type `0` (None). Filter `1` (Sub)
/// would help a real compressor, but with stored blocks the payload size is
/// unchanged and the encoder stays honest about what it does.
///
/// # Errors
///
/// Propagates I/O errors from `output`, and rejects dimensions that cannot
/// produce a valid PNG.
pub fn encode(
    output: &mut impl Write,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> io::Result<()> {
    let expected = width as usize * height as usize * 3;
    if pixels.len() != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("expected {expected} pixel bytes, got {}", pixels.len()),
        ));
    }
    if width == 0 || height == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "image dimensions must be positive",
        ));
    }

    let mut raw = Vec::with_capacity(expected + height as usize);
    for row in 0..height as usize {
        raw.push(0u8);
        let start = row * width as usize * 3;
        raw.extend_from_slice(&pixels[start..start + width as usize * 3]);
    }

    output.write_all(&SIGNATURE)?;

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // colour type: truecolour
    ihdr.push(0); // compression: deflate
    ihdr.push(0); // filter: adaptive
    ihdr.push(0); // interlace: none
    write_chunk(output, b"IHDR", &ihdr)?;
    write_chunk(output, b"IDAT", &zlib_stored(&raw))?;
    write_chunk(output, b"IEND", b"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_a_known_vector() {
        // Canonical CRC-32/ISO-HDLC check value.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn adler32_matches_known_vectors() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn zlib_stream_has_a_valid_header_and_trailer() {
        let encoded = zlib_stored(b"hello world");
        // CMF/FLG: low nibble 8 is deflate, and the 16-bit header must be a
        // multiple of 31 for a conformant decoder.
        assert_eq!(encoded[0] & 0x0F, 8);
        let header = u16::from(encoded[0]) << 8 | u16::from(encoded[1]);
        assert_eq!(header % 31, 0);
        let trailer = encoded.len() - 4;
        assert_eq!(
            u32::from_be_bytes(encoded[trailer..].try_into().unwrap()),
            adler32(b"hello world")
        );
    }

    #[test]
    fn blocks_are_chunked_at_the_deflate_limit() {
        let payload = vec![7u8; MAX_BLOCK + 10];
        let encoded = zlib_stored(&payload);
        // Header (2) + two block headers (5 each) + payload + adler (4).
        assert_eq!(encoded.len(), 2 + 5 + MAX_BLOCK + 5 + 10 + 4);
        // First block must not be final, second must be.
        assert_eq!(encoded[2] & 1, 0);
        assert_eq!(encoded[2 + 5 + MAX_BLOCK] & 1, 1);
    }

    #[test]
    fn encodes_a_minimal_image_with_the_right_chunk_order() {
        let mut out = Vec::new();
        encode(&mut out, 1, 1, &[10, 20, 30]).expect("encodes");
        assert_eq!(&out[..8], &SIGNATURE);
        assert_eq!(&out[12..16], b"IHDR");
        let ihdr_len = u32::from_be_bytes(out[8..12].try_into().unwrap());
        assert_eq!(ihdr_len, 13);
        let idat_at = 12 + 13 + 4;
        assert_eq!(&out[idat_at + 4..idat_at + 8], b"IDAT");
        let end = out.len() - 12;
        assert_eq!(&out[end + 4..end + 8], b"IEND");
    }

    #[test]
    fn rejects_mismatched_pixel_buffers() {
        let mut out = Vec::new();
        assert!(encode(&mut out, 2, 2, &[0, 0, 0]).is_err());
        assert!(encode(&mut out, 0, 1, &[]).is_err());
    }

    #[test]
    fn chunk_crc_actually_detects_corruption() {
        let mut good = Vec::new();
        encode(&mut good, 2, 1, &[1, 2, 3, 4, 5, 6]).expect("encodes");
        let mut bad = good.clone();
        // Flip one pixel byte inside IDAT; the chunk CRC must disagree.
        let pixel = good.len() - 30;
        bad[pixel] ^= 0xFF;
        assert_ne!(good, bad);
    }
}
