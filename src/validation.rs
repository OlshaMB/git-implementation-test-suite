use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use git2::{Indexer, Odb, Oid};
use serde::Serialize;

const INDEX_MAGIC: &[u8; 4] = b"\xfftOc";
const SHA1_SIZE: usize = 20;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Libgit2Metrics {
    pub total_objects: usize,
    pub indexed_objects: usize,
    pub received_objects: usize,
    pub local_objects: usize,
    pub total_deltas: usize,
    pub indexed_deltas: usize,
    pub received_bytes: usize,
    pub ingest_microseconds: u128,
    pub finalize_microseconds: u128,
    pub object_resolution_microseconds: u128,
    pub logical_object_bytes: u64,
    pub logical_to_pack_size_ratio: f64,
}

pub struct IndexReport {
    pub object_ids: Vec<String>,
    pub metrics: Libgit2Metrics,
}

pub fn index_with_libgit2(pack: &Path) -> Result<IndexReport> {
    let directory = tempfile::tempdir().context("could not create libgit2 index directory")?;
    let objects_directory = directory.path().join("objects");
    let pack_directory = objects_directory.join("pack");
    fs::create_dir_all(&pack_directory)?;
    let mut indexer = Indexer::new(None, &pack_directory, 0, true)
        .context("libgit2 could not create a pack indexer")?;
    let metrics = Rc::new(RefCell::new(Libgit2Metrics::default()));
    let progress_metrics = Rc::clone(&metrics);
    indexer.progress(move |progress| {
        let mut metrics = progress_metrics.borrow_mut();
        metrics.total_objects = metrics.total_objects.max(progress.total_objects());
        metrics.indexed_objects = metrics.indexed_objects.max(progress.indexed_objects());
        metrics.received_objects = metrics.received_objects.max(progress.received_objects());
        metrics.local_objects = metrics.local_objects.max(progress.local_objects());
        metrics.total_deltas = metrics.total_deltas.max(progress.total_deltas());
        metrics.indexed_deltas = metrics.indexed_deltas.max(progress.indexed_deltas());
        metrics.received_bytes = metrics.received_bytes.max(progress.received_bytes());
        true
    });
    let mut input =
        File::open(pack).with_context(|| format!("could not open pack {}", pack.display()))?;
    let ingest_started = Instant::now();
    let received_bytes =
        io::copy(&mut input, &mut indexer).context("libgit2 rejected pack data")?;
    let received_bytes =
        usize::try_from(received_bytes).context("pack size does not fit in memory")?;
    {
        let mut metrics = metrics.borrow_mut();
        metrics.received_bytes = metrics.received_bytes.max(received_bytes);
        metrics.ingest_microseconds = ingest_started.elapsed().as_micros();
    }
    let finalize_started = Instant::now();
    let hash = indexer
        .commit()
        .context("libgit2 could not resolve and index the pack")?;
    metrics.borrow_mut().finalize_microseconds = finalize_started.elapsed().as_micros();
    let index_path = pack_directory.join(format!("pack-{hash}.idx"));
    let object_ids = read_v2_sha1_index(&index_path)?;

    let odb = Odb::new().context("could not create libgit2 object database")?;
    let objects_path = objects_directory
        .to_str()
        .context("temporary object database path is not UTF-8")?;
    odb.add_disk_alternate(objects_path)
        .context("could not open indexed pack as a libgit2 object database")?;
    let resolution_started = Instant::now();
    let mut logical_object_bytes = 0_u64;
    for object_id in &object_ids {
        let object_id = Oid::from_str(object_id).context("invalid indexed object ID")?;
        let object = odb
            .read(object_id)
            .with_context(|| format!("libgit2 could not resolve object {object_id}"))?;
        logical_object_bytes = logical_object_bytes
            .checked_add(object.len() as u64)
            .context("logical object size overflow")?;
    }
    {
        let mut metrics = metrics.borrow_mut();
        metrics.object_resolution_microseconds = resolution_started.elapsed().as_micros();
        metrics.logical_object_bytes = logical_object_bytes;
        if metrics.received_bytes != 0 {
            metrics.logical_to_pack_size_ratio =
                logical_object_bytes as f64 / metrics.received_bytes as f64;
        }
    }

    let metrics = Rc::try_unwrap(metrics)
        .expect("libgit2 indexer retained its progress callback")
        .into_inner();
    Ok(IndexReport {
        object_ids,
        metrics,
    })
}

pub fn validate_with_git(pack: &Path) -> Result<()> {
    let directory = tempfile::tempdir().context("could not create Git validation directory")?;
    let copied_pack = directory.path().join("input.pack");
    fs::copy(pack, &copied_pack).with_context(|| {
        format!(
            "could not copy {} for canonical Git validation",
            pack.display()
        )
    })?;

    let home = directory.path().join("home");
    fs::create_dir(&home)?;
    let output = Command::new("git")
        .args(["index-pack", "--strict"])
        .arg(&copied_pack)
        .current_dir(directory.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env("LC_ALL", "C")
        .output()
        .context("could not execute canonical git index-pack")?;
    if !output.status.success() {
        bail!(
            "canonical git index-pack rejected the pack (status {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub fn read_expected_objects(path: &Path) -> Result<Vec<String>> {
    let input = BufReader::new(
        File::open(path)
            .with_context(|| format!("could not read expected objects {}", path.display()))?,
    );
    let mut result = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "could not read line {} of expected objects {}",
                index + 1,
                path.display()
            )
        })?;
        let object_id = line.trim();
        ensure!(
            object_id.len() == 40 && object_id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid SHA-1 object ID on line {} of {}",
            index + 1,
            path.display()
        );
        let object_id = object_id.to_ascii_lowercase();
        if let Some(previous) = result.last() {
            ensure!(
                previous < &object_id,
                "object IDs in {} must be sorted and unique; {object_id} follows {previous}",
                path.display()
            );
        }
        result.push(object_id);
    }
    ensure!(
        !result.is_empty(),
        "expected object set {} is empty",
        path.display()
    );
    Ok(result)
}

pub fn compare_object_sets(expected: &[String], actual: &[String]) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    let (missing, missing_count, unexpected, unexpected_count) =
        sorted_set_difference(expected, actual);
    bail!(
        "packed object set differs: expected {}, got {}; missing [{}]{}; unexpected [{}]{}",
        expected.len(),
        actual.len(),
        missing.join(", "),
        if missing_count > missing.len() {
            " (more omitted)"
        } else {
            ""
        },
        unexpected.join(", "),
        if unexpected_count > unexpected.len() {
            " (more omitted)"
        } else {
            ""
        }
    )
}

fn sorted_set_difference(
    expected: &[String],
    actual: &[String],
) -> (Vec<String>, usize, Vec<String>, usize) {
    let (mut expected_index, mut actual_index) = (0, 0);
    let (mut missing, mut unexpected) = (Vec::new(), Vec::new());
    let (mut missing_count, mut unexpected_count) = (0, 0);
    while expected_index < expected.len() && actual_index < actual.len() {
        match expected[expected_index].cmp(&actual[actual_index]) {
            std::cmp::Ordering::Less => {
                missing_count += 1;
                if missing.len() < 10 {
                    missing.push(expected[expected_index].clone());
                }
                expected_index += 1;
            }
            std::cmp::Ordering::Greater => {
                unexpected_count += 1;
                if unexpected.len() < 10 {
                    unexpected.push(actual[actual_index].clone());
                }
                actual_index += 1;
            }
            std::cmp::Ordering::Equal => {
                expected_index += 1;
                actual_index += 1;
            }
        }
    }
    for object_id in &expected[expected_index..] {
        missing_count += 1;
        if missing.len() < 10 {
            missing.push(object_id.clone());
        }
    }
    for object_id in &actual[actual_index..] {
        unexpected_count += 1;
        if unexpected.len() < 10 {
            unexpected.push(object_id.clone());
        }
    }
    (missing, missing_count, unexpected, unexpected_count)
}

fn read_v2_sha1_index(path: &Path) -> Result<Vec<String>> {
    let data =
        fs::read(path).with_context(|| format!("could not read pack index {}", path.display()))?;
    ensure!(data.len() >= 8 + 256 * 4, "pack index is too short");
    ensure!(&data[0..4] == INDEX_MAGIC, "pack index is not version 2+");
    let version = read_u32(&data, 4)?;
    ensure!(version == 2, "unsupported pack index version {version}");

    let count = usize::try_from(read_u32(&data, 8 + 255 * 4)?)
        .context("pack index object count does not fit in memory")?;
    let names_start: usize = 8 + 256 * 4;
    let names_end = names_start
        .checked_add(
            count
                .checked_mul(SHA1_SIZE)
                .context("index size overflow")?,
        )
        .context("index size overflow")?;
    ensure!(names_end <= data.len(), "truncated object-name table");

    let mut result = Vec::with_capacity(count);
    let (object_ids, remainder) = data[names_start..names_end].as_chunks::<SHA1_SIZE>();
    debug_assert!(remainder.is_empty());
    for object_id in object_ids {
        result.push(hex::encode(object_id));
    }
    ensure!(
        result.windows(2).all(|pair| pair[0] < pair[1]),
        "pack index object IDs are not sorted and unique"
    );
    Ok(result)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .context("truncated pack index integer")?
        .try_into()
        .expect("slice length was checked");
    Ok(u32::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::compare_object_sets;

    #[test]
    fn object_set_comparison_reports_mismatch() {
        let expected = vec!["a".repeat(40)];
        let actual = vec!["b".repeat(40)];
        let error = compare_object_sets(&expected, &actual).unwrap_err();
        assert!(error.to_string().contains("missing"));
        assert!(error.to_string().contains("unexpected"));
    }
}
