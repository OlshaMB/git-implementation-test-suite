use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use flate2::{Decompress, FlushDecompress, Status};
use memmap2::MmapOptions;
use serde::Serialize;
use sha1::{Digest, Sha1};

const PACK_HEADER_SIZE: usize = 12;
const SHA1_SIZE: usize = 20;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStats {
    pub version: u32,
    pub object_count: u32,
    pub pack_bytes: u64,
    pub base_commit_objects: u32,
    pub base_tree_objects: u32,
    pub base_blob_objects: u32,
    pub base_tag_objects: u32,
    pub ofs_deltas: u32,
    pub ref_deltas: u32,
    pub compressed_entry_bytes: u64,
    pub inflated_entry_bytes: u64,
    pub trailer_checksum: String,
}

impl PackStats {
    pub fn delta_count(&self) -> u32 {
        self.ofs_deltas + self.ref_deltas
    }
}

pub fn scan(path: &Path) -> Result<PackStats> {
    let file =
        File::open(path).with_context(|| format!("could not open pack {}", path.display()))?;
    // Mapping keeps the structural scanner from copying a Linux-sized pack into
    // the Rust heap. Pages are faulted in as the sequential scan needs them.
    let data = unsafe { MmapOptions::new().map(&file) }
        .with_context(|| format!("could not map pack {}", path.display()))?;

    ensure!(
        data.len() >= PACK_HEADER_SIZE + SHA1_SIZE,
        "pack is too short"
    );
    ensure!(&data[0..4] == b"PACK", "invalid pack signature");
    let version = read_u32(&data, 4)?;
    ensure!(
        matches!(version, 2 | 3),
        "unsupported pack version {version}"
    );
    let object_count = read_u32(&data, 8)?;
    let payload_end = data.len() - SHA1_SIZE;

    let actual_checksum = Sha1::digest(&data[..payload_end]);
    let expected_checksum = &data[payload_end..];
    ensure!(
        actual_checksum.as_slice() == expected_checksum,
        "pack trailer checksum mismatch: expected {}, computed {}",
        hex::encode(expected_checksum),
        hex::encode(actual_checksum)
    );

    let mut stats = PackStats {
        version,
        object_count,
        pack_bytes: data.len() as u64,
        base_commit_objects: 0,
        base_tree_objects: 0,
        base_blob_objects: 0,
        base_tag_objects: 0,
        ofs_deltas: 0,
        ref_deltas: 0,
        compressed_entry_bytes: 0,
        inflated_entry_bytes: 0,
        trailer_checksum: hex::encode(expected_checksum),
    };

    let mut position = PACK_HEADER_SIZE;
    let mut entry_offsets = HashSet::with_capacity(object_count as usize);
    for index in 0..object_count {
        let entry_offset = position;
        let (kind, inflated_size) = read_entry_header(&data, &mut position, payload_end)
            .with_context(|| format!("invalid header for pack entry {index}"))?;

        match kind {
            1 => stats.base_commit_objects += 1,
            2 => stats.base_tree_objects += 1,
            3 => stats.base_blob_objects += 1,
            4 => stats.base_tag_objects += 1,
            6 => {
                stats.ofs_deltas += 1;
                let distance = read_ofs_delta_distance(&data, &mut position, payload_end)
                    .with_context(|| format!("invalid OFS_DELTA base for entry {index}"))?;
                let distance =
                    usize::try_from(distance).context("OFS_DELTA distance does not fit usize")?;
                let base_offset = entry_offset.checked_sub(distance);
                ensure!(
                    base_offset.is_some_and(|offset| entry_offsets.contains(&offset)),
                    "OFS_DELTA entry {index} does not point to an earlier object"
                );
            }
            7 => {
                stats.ref_deltas += 1;
                ensure!(
                    position + SHA1_SIZE <= payload_end,
                    "truncated REF_DELTA base for entry {index}"
                );
                position += SHA1_SIZE;
            }
            other => bail!("unsupported object type {other} in pack entry {index}"),
        }

        let compressed_start = position;
        position = consume_zlib_stream(&data, position, payload_end, inflated_size)
            .with_context(|| format!("invalid zlib data for pack entry {index}"))?;
        stats.compressed_entry_bytes += (position - compressed_start) as u64;
        stats.inflated_entry_bytes = stats
            .inflated_entry_bytes
            .checked_add(inflated_size)
            .context("inflated pack size overflow")?;
        entry_offsets.insert(entry_offset);
    }

    ensure!(
        position == payload_end,
        "pack object data ended at byte {position}, but trailer starts at byte {payload_end}"
    );
    Ok(stats)
}

fn read_entry_header(data: &[u8], position: &mut usize, end: usize) -> Result<(u8, u64)> {
    ensure!(*position < end, "truncated object header");
    let mut byte = data[*position];
    *position += 1;
    let kind = (byte >> 4) & 0x07;
    let mut size = u64::from(byte & 0x0f);
    let mut shift = 4;

    while byte & 0x80 != 0 {
        ensure!(*position < end, "truncated object size");
        ensure!(shift < 64, "object size overflows 64 bits");
        byte = data[*position];
        *position += 1;
        let chunk = u64::from(byte & 0x7f);
        ensure!(
            chunk <= (u64::MAX >> shift),
            "object size overflows 64 bits"
        );
        size |= chunk << shift;
        shift += 7;
    }
    Ok((kind, size))
}

fn read_ofs_delta_distance(data: &[u8], position: &mut usize, end: usize) -> Result<u64> {
    ensure!(*position < end, "missing OFS_DELTA offset");
    let mut byte = data[*position];
    *position += 1;
    let mut distance = u64::from(byte & 0x7f);
    while byte & 0x80 != 0 {
        ensure!(*position < end, "truncated OFS_DELTA offset");
        byte = data[*position];
        *position += 1;
        distance = distance
            .checked_add(1)
            .and_then(|value| value.checked_shl(7))
            .and_then(|value| value.checked_add(u64::from(byte & 0x7f)))
            .context("OFS_DELTA offset overflows 64 bits")?;
    }
    ensure!(distance > 0, "OFS_DELTA offset must be nonzero");
    Ok(distance)
}

fn consume_zlib_stream(data: &[u8], start: usize, end: usize, expected_size: u64) -> Result<usize> {
    ensure!(start < end, "missing zlib stream");
    let mut decompressor = Decompress::new(true);
    let mut input_position = start;
    let mut output = [0_u8; 64 * 1024];

    loop {
        let input_before = decompressor.total_in();
        let output_before = decompressor.total_out();
        let status = decompressor
            .decompress(
                &data[input_position..end],
                &mut output,
                FlushDecompress::None,
            )
            .context("zlib decompression failed")?;
        let consumed = decompressor.total_in() - input_before;
        let produced = decompressor.total_out() - output_before;
        input_position += usize::try_from(consumed).context("zlib input offset overflow")?;

        if status == Status::StreamEnd {
            break;
        }
        ensure!(
            consumed != 0 || produced != 0,
            "zlib stream made no progress"
        );
    }

    ensure!(
        decompressor.total_out() == expected_size,
        "entry declares {expected_size} inflated bytes, zlib stream produced {}",
        decompressor.total_out()
    );
    Ok(input_position)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .context("truncated 32-bit integer")?
        .try_into()
        .expect("slice length was checked");
    Ok(u32::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::{read_entry_header, read_ofs_delta_distance};

    #[test]
    fn parses_multibyte_object_size() {
        // Blob, size 0x1234.
        let bytes = [0xb4, 0xa3, 0x02];
        let mut position = 0;
        let (kind, size) = read_entry_header(&bytes, &mut position, bytes.len()).unwrap();
        assert_eq!(kind, 3);
        assert_eq!(size, 0x1234);
        assert_eq!(position, bytes.len());
    }

    #[test]
    fn parses_ofs_delta_distance() {
        let bytes = [0x80, 0x00];
        let mut position = 0;
        let distance = read_ofs_delta_distance(&bytes, &mut position, bytes.len()).unwrap();
        assert_eq!(distance, 128);
    }

    #[test]
    fn rejects_zero_ofs_delta_distance() {
        let bytes = [0x00];
        let mut position = 0;
        assert!(read_ofs_delta_distance(&bytes, &mut position, bytes.len()).is_err());
    }
}
