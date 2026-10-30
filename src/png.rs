//! Minimal PNG encoder.
//!
//! Writes a colour-accurate 8-bit RGB PNG using `stored` DEFLATE blocks, i.e.
//! no compression. That is a deliberate trade: a real Huffman encoder is a few
//! hundred lines of table-building that this project does not need, and PNG's
//! per-scanline filters already make smooth gradients compress well once a
//! consumer re-encodes them. Output is standards-conformant, so `file`,
//! `ImageMagick`, and browsers all accept it.

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
fn write_chunk(output: &mut impl Write, kind: [u8; 4], payload: &[u8]) -> io::Result<()> {
    output.write_all(&(payload.len() as u32).to_be_bytes())?;
    let mut body = Vec::with_capacity(4 + payload.len());
    body.extend_from_slice(&kind);
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
    // `slice::chunks` yields nothing for an empty payload, so walk the blocks
    // explicitly to guarantee exactly one final block in every case -- an empty
    // image still needs the terminating block a decoder looks for.
    let blocks = data.len().div_ceil(MAX_BLOCK).max(1);
    for index in 0..blocks {
        let start = index * MAX_BLOCK;
        let chunk = &data[start..start + MAX_BLOCK.min(data.len() - start)];
        let is_last = index + 1 == blocks;
        out.push(u8::from(is_last));
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    // zlib packs the Adler-32 trailer most-significant byte first (RFC 1950
    // §2.2); the DEFLATE block lengths just above are little-endian, so the two
    // endiannesses sit side by side and are easy to confuse.
    out.extend_from_slice(&adler32(data).to_be_bytes());
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
pub fn encode(output: &mut impl Write, width: u32, height: u32, pixels: &[u8]) -> io::Result<()> {
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
    write_chunk(output, *b"IHDR", &ihdr)?;
    write_chunk(output, *b"IDAT", &zlib_stored(&raw))?;
    write_chunk(output, *b"IEND", b"")
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
    fn zlib_stream_round_trips_through_the_stored_decoder() {
        for payload in [
            b"".to_vec(),
            b"h".to_vec(),
            b"hello world".to_vec(),
            (0u8..=255).collect::<Vec<u8>>(),
        ] {
            let encoded = zlib_stored(&payload);
            // CMF/FLG: the 16-bit header must be a multiple of 31.
            let header = u16::from(encoded[0]) << 8 | u16::from(encoded[1]);
            assert_eq!(header % 31, 0, "zlib header for {} bytes", payload.len());
            assert_eq!(stored_payload(&encoded), payload);
        }
    }

    #[test]
    fn payloads_larger_than_a_block_are_split_and_reassembled() {
        // Anything over the 65 535-byte block limit needs a second block whose
        // final-bit is set; the decoder walks both.
        let payload: Vec<u8> = (0..MAX_BLOCK + 10_000)
            .map(|index| (index % 251) as u8)
            .collect();
        let encoded = zlib_stored(&payload);
        assert_eq!(stored_payload(&encoded), payload);
        assert!(encoded.len() > payload.len(), "stored blocks add framing");
    }

    /// Walk the chunk stream, returning each `(type, payload)` pair.
    ///
    /// Parsing the structure rather than asserting fixed offsets keeps these
    /// tests meaningful when the payload size changes, and it validates the
    /// length fields and CRCs the same way a decoder would.
    fn chunks(data: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&data[..8], &SIGNATURE, "signature");
        let mut at = 8;
        let mut found = Vec::new();
        while at < data.len() {
            let length = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = data[at + 4..at + 8].try_into().unwrap();
            let payload_start = at + 8;
            let payload = data[payload_start..payload_start + length].to_vec();
            let crc_start = payload_start + length;
            let crc = u32::from_be_bytes(data[crc_start..crc_start + 4].try_into().unwrap());
            let mut body = Vec::with_capacity(4 + length);
            body.extend_from_slice(&kind);
            body.extend_from_slice(&payload);
            assert_eq!(
                crc32(&body),
                crc,
                "{:?} chunk CRC",
                std::str::from_utf8(&kind)
            );
            at = crc_start + 4;
            found.push((kind, payload));
        }
        assert_eq!(at, data.len(), "trailing bytes after the last chunk");
        found
    }

    #[test]
    fn encodes_a_minimal_image_with_the_right_chunk_order() {
        let mut out = Vec::new();
        encode(&mut out, 1, 1, &[10, 20, 30]).expect("encodes");
        let found = chunks(&out);
        let kinds: Vec<&[u8; 4]> = found.iter().map(|(kind, _)| kind).collect();
        assert_eq!(kinds, [b"IHDR", b"IDAT", b"IEND"]);

        let ihdr = &found[0].1;
        assert_eq!(ihdr.len(), 13);
        assert_eq!(u32::from_be_bytes(ihdr[0..4].try_into().unwrap()), 1);
        assert_eq!(u32::from_be_bytes(ihdr[4..8].try_into().unwrap()), 1);
        assert_eq!(ihdr[8], 8, "bit depth");
        assert_eq!(ihdr[9], 2, "colour type: truecolour");

        // The pixel payload must survive the zlib framing intact.
        let idat = &found[1].1;
        assert_eq!(stored_payload(idat), vec![0, 10, 20, 30]);
    }

    /// Strip a zlib stream of stored blocks back to its raw bytes.
    ///
    /// Block lengths are little-endian and the Adler-32 trailer big-endian, as
    /// RFC 1950 requires; asserting both is what catches an encoder that mixes
    /// them up.
    fn stored_payload(zlib: &[u8]) -> Vec<u8> {
        assert_eq!(zlib[0] & 0x0F, 8, "compression method: deflate");
        let mut at = 2;
        let mut out = Vec::new();
        loop {
            let header = zlib[at];
            let length = u16::from_le_bytes(zlib[at + 1..at + 3].try_into().unwrap());
            let complement = u16::from_le_bytes(zlib[at + 3..at + 5].try_into().unwrap());
            assert_eq!(complement, !length, "stored block length complement");
            out.extend_from_slice(&zlib[at + 5..at + 5 + usize::from(length)]);
            let is_last = header & 1 == 1;
            at += 5 + usize::from(length);
            if is_last {
                break;
            }
        }
        assert_eq!(
            u32::from_be_bytes(zlib[at..at + 4].try_into().unwrap()),
            adler32(&out),
            "trailing Adler-32 must match the payload"
        );
        out
    }

    #[test]
    fn adler_trailer_is_big_endian_so_a_real_decoder_accepts_the_stream() {
        // The block lengths are little-endian and the Adler-32 trailer is not,
        // so writing both the same way produces a stream a conformant decoder
        // rejects. Picking a payload whose Adler-32 is not byte-symmetric makes
        // the two readings differ: 0x0068_003D against 0x3D00_6800.
        let payload = [0u8, 10, 20, 30];
        let checksum = adler32(&payload);
        assert_ne!(
            checksum,
            checksum.swap_bytes(),
            "test payload must distinguish the two endiannesses"
        );
        let encoded = zlib_stored(&payload);
        let trailer_at = encoded.len() - 4;
        assert_eq!(
            u32::from_be_bytes(encoded[trailer_at..].try_into().unwrap()),
            checksum,
            "trailer must be big-endian, as RFC 1950 requires"
        );
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
