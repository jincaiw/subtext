//! The pixel size of an image file, read from its header.
//!
//! *File Drop*'s `<<<IMAGEWIDTH>>>` and `<<<IMAGEHEIGHT>>>` tokens need the
//! size of the image being dropped. CotEditor asks ImageIO, which decodes every
//! format the system knows. SubText has no image library — it is built on a UI
//! toolkit and nothing else — so this module reads the header itself.
//!
//! Five formats are covered: PNG, GIF, JPEG, BMP and WebP. They are what a
//! document is likely to name, and for all five the header carries the size
//! without decoding a pixel. A format that is not covered yields `None`, and
//! the caller leaves the token in the text — which is exactly what CotEditor
//! does for an image whose size ImageIO cannot read.

use std::path::Path;

/// The pixel size of the image at `path`, if its header can be read.
pub fn read(path: &Path) -> Option<(u32, u32)> {
    dimensions(&std::fs::read(path).ok()?)
}

/// The pixel size of an image whose bytes are `bytes`.
///
/// The formats are tried in turn rather than by extension: the bytes are what
/// decides, and a file named `.png` that is not one should not be read as one.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    png(bytes).or_else(|| gif(bytes)).or_else(|| bmp(bytes)).or_else(|| webp(bytes)).or_else(|| jpeg(bytes))
}

// ---- the formats ---------------------------------------------------------

/// The eight bytes every PNG starts with.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn png(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return None;
    }
    // The signature is followed by the IHDR chunk: its length and type, then
    // the two big-endian dimensions.
    Some((be_u32(bytes, 16)?, be_u32(bytes, 20)?))
}

fn gif(bytes: &[u8]) -> Option<(u32, u32)> {
    if !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return None;
    }
    Some((u32::from(le_u16(bytes, 6)?), u32::from(le_u16(bytes, 8)?)))
}

fn bmp(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"BM") {
        return None;
    }
    // The DIB header starts at 14 and its first field is its own size, which
    // is what distinguishes the versions; every one of them puts the
    // dimensions next, as signed 32-bit values. A negative height means a
    // top-down bitmap, and the sign is not part of the size.
    let width = le_i32(bytes, 18)?;
    let height = le_i32(bytes, 22)?;
    Some((width.unsigned_abs(), height.unsigned_abs()))
}

fn webp(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"RIFF") || bytes.get(8..12)? != b"WEBP".as_slice() {
        return None;
    }
    match bytes.get(12..16)? {
        chunk if chunk == b"VP8 ".as_slice() => webp_lossy(bytes),
        chunk if chunk == b"VP8L".as_slice() => webp_lossless(bytes),
        // The extended container stores the canvas size minus one, as two
        // 24-bit little-endian values.
        chunk if chunk == b"VP8X".as_slice() => {
            Some((le_u24(bytes, 24)? + 1, le_u24(bytes, 27)? + 1))
        }
        _ => None,
    }
}

/// A lossy WebP: three bytes of frame tag, the three-byte start code, then the
/// two 14-bit dimensions.
fn webp_lossy(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(23..26)? != &[0x9D, 0x01, 0x2A][..] {
        return None;
    }
    let packed = le_u32(bytes, 26)?;
    Some((packed & 0x3FFF, (packed >> 16) & 0x3FFF))
}

/// A lossless WebP: a signature byte, then both dimensions minus one packed
/// into 14 bits each.
fn webp_lossless(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(20)? != &0x2F {
        return None;
    }
    let packed = le_u32(bytes, 21)?;
    Some(((packed & 0x3FFF) + 1, ((packed >> 14) & 0x3FFF) + 1))
}

fn jpeg(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // Walk the marker segments. The frame header is the one that carries the
    // dimensions; everything before it — the application segments, the
    // quantisation and Huffman tables — is skipped by its own length.
    let mut index = 2;
    while index + 1 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        // Any number of 0xFF bytes may pad the space before a marker.
        if marker == 0xFF {
            index += 1;
            continue;
        }
        // `TEM` and the restart markers stand alone and carry no length.
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            index += 2;
            continue;
        }
        let length = usize::from(be_u16(bytes, index + 2)?);
        if length < 2 {
            return None;
        }
        // `SOF0`–`SOF15` are the frame headers, except `DHT`, `JPG` and `DAC`,
        // which share the range without being one. The dimensions follow one
        // byte of sample precision.
        let is_frame = (0xC0..=0xCF).contains(&marker)
            && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            return Some((
                u32::from(be_u16(bytes, index + 7)?),
                u32::from(be_u16(bytes, index + 5)?),
            ));
        }
        index += 2 + length;
    }
    None
}

// ---- reading numbers -----------------------------------------------------

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?))
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

fn le_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

fn le_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?))
}

fn le_u24(bytes: &[u8], offset: usize) -> Option<u32> {
    let three = bytes.get(offset..offset + 3)?;
    Some(u32::from(three[0]) | u32::from(three[1]) << 8 | u32::from(three[2]) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG holding a 640 × 480 image, as far as the header goes.
    fn png_bytes() -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes()); // IHDR length
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&640u32.to_be_bytes());
        bytes.extend_from_slice(&480u32.to_be_bytes());
        bytes
    }

    fn gif_bytes() -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&64u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes
    }

    fn bmp_bytes(width: i32, height: i32) -> Vec<u8> {
        let mut bytes = b"BM".to_vec();
        bytes.resize(14, 0);
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes
    }

    /// A JPEG whose first segment is an application segment to be skipped and
    /// whose second is the frame header.
    fn jpeg_bytes() -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE0]); // APP0
        bytes.extend_from_slice(&16u16.to_be_bytes()); // length, including itself
        bytes.extend_from_slice(&[0; 14]);
        bytes.extend_from_slice(&[0xFF, 0xC0]); // SOF0
        bytes.extend_from_slice(&17u16.to_be_bytes());
        bytes.push(8); // precision
        bytes.extend_from_slice(&768u16.to_be_bytes()); // height
        bytes.extend_from_slice(&1024u16.to_be_bytes()); // width
        bytes
    }

    fn webp_lossy_bytes() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8 ");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]); // frame tag
        bytes.extend_from_slice(&[0x9D, 0x01, 0x2A]); // start code
        bytes.extend_from_slice(&((200u32) | (100u32 << 16)).to_le_bytes());
        bytes
    }

    fn webp_lossless_bytes() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8L");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.push(0x2F); // signature
        bytes.extend_from_slice(&((299u32) | (199u32 << 14)).to_le_bytes());
        bytes
    }

    fn webp_extended_bytes() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]); // flags and reserved
        bytes.extend_from_slice(&[0x7F, 0x02, 0x00]); // width - 1 = 639
        bytes.extend_from_slice(&[0xDF, 0x01, 0x00]); // height - 1 = 479
        bytes
    }

    #[test]
    fn a_png_reports_its_header_size() {
        assert_eq!(dimensions(&png_bytes()), Some((640, 480)));
    }

    #[test]
    fn a_gif_reports_its_header_size() {
        assert_eq!(dimensions(&gif_bytes()), Some((64, 32)));
    }

    #[test]
    fn a_bmp_reports_its_header_size() {
        assert_eq!(dimensions(&bmp_bytes(800, 600)), Some((800, 600)));
    }

    /// A top-down bitmap stores a negative height; the sign is not part of the
    /// size.
    #[test]
    fn a_top_down_bmp_reports_a_positive_height() {
        assert_eq!(dimensions(&bmp_bytes(800, -600)), Some((800, 600)));
    }

    #[test]
    fn a_jpeg_reports_the_frame_header_size() {
        assert_eq!(dimensions(&jpeg_bytes()), Some((1024, 768)));
    }

    #[test]
    fn every_webp_container_reports_its_size() {
        assert_eq!(dimensions(&webp_lossy_bytes()), Some((200, 100)));
        assert_eq!(dimensions(&webp_lossless_bytes()), Some((300, 200)));
        assert_eq!(dimensions(&webp_extended_bytes()), Some((640, 480)));
    }

    /// The bytes decide, not the name: something that is not an image has no
    /// size to report.
    #[test]
    fn text_is_not_an_image() {
        assert_eq!(dimensions(b"not an image at all"), None);
        assert_eq!(dimensions(b""), None);
    }

    /// A header cut short must not panic, and must not invent a size.
    #[test]
    fn a_truncated_header_has_no_size() {
        for length in 0..png_bytes().len() {
            assert_eq!(dimensions(&png_bytes()[..length]), None, "{length} bytes");
        }
        for length in 0..jpeg_bytes().len() {
            assert_eq!(dimensions(&jpeg_bytes()[..length]), None, "{length} bytes");
        }
        for length in 0..webp_lossy_bytes().len() {
            assert_eq!(dimensions(&webp_lossy_bytes()[..length]), None, "{length} bytes");
        }
    }

    /// A JPEG marker that shares the frame-header range without being one must
    /// be skipped rather than read as dimensions.
    #[test]
    fn a_huffman_table_is_not_a_frame_header() {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xC4]); // DHT
        bytes.extend_from_slice(&8u16.to_be_bytes());
        bytes.extend_from_slice(&[0; 6]);
        bytes.extend_from_slice(&[0xFF, 0xC0]); // SOF0
        bytes.extend_from_slice(&17u16.to_be_bytes());
        bytes.push(8);
        bytes.extend_from_slice(&10u16.to_be_bytes());
        bytes.extend_from_slice(&20u16.to_be_bytes());
        assert_eq!(dimensions(&bytes), Some((20, 10)));
    }

    /// A real file on disk goes through the same path.
    #[test]
    fn a_file_is_read_from_disk() {
        let path = std::env::temp_dir().join("subtext-image-size-test.png");
        std::fs::write(&path, png_bytes()).unwrap();
        assert_eq!(read(&path), Some((640, 480)));
        std::fs::remove_file(&path).ok();

        assert_eq!(read(&std::env::temp_dir().join("subtext-no-such-image.png")), None);
    }
}
