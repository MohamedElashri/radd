//! Partial merge cache.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    executor::ExecutableJob,
    hadd::HaddOptions,
    input::{InputFile, InputSet},
    planner::{MergeJob, MergePlan, MergePolicy},
};

const CACHE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRoot {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedExecution {
    pub stages: Vec<Vec<ExecutableJob>>,
    pub pending_stores: Vec<PendingStore>,
    pub hits: usize,
    pub misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingStore {
    source_output: PathBuf,
    entry: CacheEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheList {
    pub root: PathBuf,
    pub entries: Vec<ListEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    pub key: String,
    pub size_bytes: Option<u64>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanReport {
    pub root: PathBuf,
    pub removed_files: usize,
    pub removed_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheEntry {
    key: String,
    chunk_path: PathBuf,
    manifest_path: PathBuf,
    manifest: CacheManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CacheManifest {
    schema_version: u8,
    key: String,
    radd_version: String,
    created_time: Option<CacheUnixTime>,
    output_size_bytes: u64,
    inputs: Vec<CacheInput>,
    options: CacheOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CacheInput {
    path: PathBuf,
    size_bytes: u64,
    modified_time: Option<CacheUnixTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CacheOptions {
    policy: MergePolicy,
    hadd_version: Option<String>,
    hadd_jobs: Option<usize>,
    keep_going: bool,
    max_open_files: Option<usize>,
    no_trees: bool,
    object_selection: Option<CacheObjectSelection>,
    radd_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CacheObjectSelection {
    mode: crate::hadd::ObjectSelectionMode,
    objects: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct CacheUnixTime {
    seconds: u64,
    nanos: u32,
}

impl CacheRoot {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    fn chunks_dir(&self) -> PathBuf {
        self.root.join("chunks")
    }

    fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }

    fn entry(&self, manifest: CacheManifest) -> CacheEntry {
        let key = manifest.key.clone();

        CacheEntry {
            chunk_path: self.chunks_dir().join(format!("{key}.root")),
            manifest_path: self.manifests_dir().join(format!("{key}.json")),
            key,
            manifest,
        }
    }
}

#[must_use]
pub fn default_cache_root() -> CacheRoot {
    if let Some(path) = env::var_os("RADD_CACHE_DIR") {
        return CacheRoot::new(PathBuf::from(path));
    }

    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return CacheRoot::new(PathBuf::from(path).join("radd"));
    }

    if let Some(path) = env::var_os("HOME") {
        return CacheRoot::new(PathBuf::from(path).join(".cache").join("radd"));
    }

    CacheRoot::new(env::temp_dir().join("radd-cache"))
}

pub fn prepare_execution(
    root: &CacheRoot,
    input_set: &InputSet,
    plan: &MergePlan,
    stages: &[Vec<ExecutableJob>],
    hadd_options: &HaddOptions,
) -> Result<PreparedExecution> {
    let mut prepared_stages = Vec::with_capacity(stages.len());
    let mut pending_stores = Vec::new();
    let mut hits = 0;
    let mut misses = 0;

    for (stage_index, stage) in stages.iter().enumerate() {
        let mut prepared_stage = Vec::new();

        for executable in stage {
            let plan_job = plan_job(plan, stage_index, executable.job_id)?;

            if !is_cacheable(plan, stage_index, plan_job) {
                prepared_stage.push(executable.clone());
                continue;
            }

            let entry = cache_entry_for_job(root, plan_job, input_set, hadd_options)?;
            if entry_is_valid(&entry)? {
                copy_cached_chunk(&entry, &executable.output)?;
                hits += 1;
            } else {
                misses += 1;
                pending_stores.push(PendingStore {
                    source_output: executable.output.clone(),
                    entry,
                });
                prepared_stage.push(executable.clone());
            }
        }

        prepared_stages.push(prepared_stage);
    }

    Ok(PreparedExecution {
        stages: prepared_stages,
        pending_stores,
        hits,
        misses,
    })
}

pub fn store_pending(pending_stores: &[PendingStore]) -> Result<()> {
    for pending in pending_stores {
        store_output(&pending.source_output, &pending.entry)?;
    }

    Ok(())
}

pub fn list_cache(root: &CacheRoot) -> Result<CacheList> {
    let manifests_dir = root.manifests_dir();
    let mut entries = Vec::new();

    if !manifests_dir.exists() {
        return Ok(CacheList {
            root: root.path().to_path_buf(),
            entries,
        });
    }

    for entry in fs::read_dir(&manifests_dir).with_context(|| {
        format!(
            "could not read cache manifests: {}",
            manifests_dir.display()
        )
    })? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

        let manifest_path = entry.path();
        let manifest = read_manifest(&manifest_path);
        let Some(manifest) = manifest.ok() else {
            continue;
        };
        let cache_entry = root.entry(manifest);
        let chunk_metadata = fs::metadata(&cache_entry.chunk_path).ok();
        let size_bytes = chunk_metadata.as_ref().map(std::fs::Metadata::len);
        let complete = chunk_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.is_file() && metadata.len() > 0);

        entries.push(ListEntry {
            key: cache_entry.key,
            size_bytes,
            complete,
        });
    }

    entries.sort_by(|left, right| left.key.cmp(&right.key));

    Ok(CacheList {
        root: root.path().to_path_buf(),
        entries,
    })
}

pub fn clean_cache(root: &CacheRoot) -> Result<CleanReport> {
    let mut report = CleanReport {
        root: root.path().to_path_buf(),
        removed_files: 0,
        removed_bytes: 0,
    };

    for directory in [root.chunks_dir(), root.manifests_dir()] {
        clean_directory_files(&directory, &mut report)?;
    }

    Ok(report)
}

#[must_use]
pub fn format_cache_list(list: &CacheList) -> String {
    let mut output = String::new();

    output.push_str("radd cache list\n\n");
    writeln!(&mut output, "root: {}", list.root.display()).expect("write to string");
    writeln!(&mut output, "entries: {}", list.entries.len()).expect("write to string");

    for entry in &list.entries {
        let size = entry
            .size_bytes
            .map_or_else(|| "missing".to_string(), |size| format!("{size} bytes"));
        writeln!(
            &mut output,
            "{}  {}  {}",
            entry.key,
            size,
            if entry.complete {
                "complete"
            } else {
                "incomplete"
            }
        )
        .expect("write to string");
    }

    output
}

#[must_use]
pub fn format_clean_report(report: &CleanReport) -> String {
    format!(
        "radd cache clean\n\nroot: {}\nremoved files: {}\nremoved bytes: {}\n",
        report.root.display(),
        report.removed_files,
        report.removed_bytes
    )
}

pub fn cache_key_for_job(
    job: &MergeJob,
    input_set: &InputSet,
    hadd_options: &HaddOptions,
) -> Result<String> {
    let manifest = manifest_for_job(job, input_set, hadd_options, String::new(), 0)?;
    let material = serde_json::to_vec(&manifest)?;
    let digest = Sha256::digest(material);

    Ok(format!("{digest:x}"))
}

fn cache_entry_for_job(
    root: &CacheRoot,
    job: &MergeJob,
    input_set: &InputSet,
    hadd_options: &HaddOptions,
) -> Result<CacheEntry> {
    let key = cache_key_for_job(job, input_set, hadd_options)?;
    let manifest = manifest_for_job(job, input_set, hadd_options, key, 0)?;

    Ok(root.entry(manifest))
}

fn manifest_for_job(
    job: &MergeJob,
    input_set: &InputSet,
    hadd_options: &HaddOptions,
    key: String,
    output_size_bytes: u64,
) -> Result<CacheManifest> {
    Ok(CacheManifest {
        schema_version: CACHE_SCHEMA_VERSION,
        key,
        radd_version: env!("CARGO_PKG_VERSION").to_string(),
        created_time: None,
        output_size_bytes,
        inputs: cache_inputs(job, input_set)?,
        options: CacheOptions {
            policy: hadd_options.policy,
            hadd_version: hadd_options.version.clone(),
            hadd_jobs: hadd_options.hadd_jobs,
            keep_going: hadd_options.keep_going,
            max_open_files: hadd_options.max_open_files,
            no_trees: hadd_options.no_trees,
            object_selection: hadd_options.object_selection.as_ref().map(|selection| {
                CacheObjectSelection {
                    mode: selection.mode,
                    objects: selection.objects.clone(),
                }
            }),
            radd_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    })
}

fn cache_inputs(job: &MergeJob, input_set: &InputSet) -> Result<Vec<CacheInput>> {
    job.inputs
        .iter()
        .map(|path| {
            let input = input_set
                .files
                .iter()
                .find(|candidate| candidate.path == *path)
                .with_context(|| {
                    format!("cache key input is not in input set: {}", path.display())
                })?;

            Ok(cache_input(input))
        })
        .collect()
}

fn cache_input(input: &InputFile) -> CacheInput {
    CacheInput {
        path: input.path.clone(),
        size_bytes: input.size_bytes,
        modified_time: input.modified_time.and_then(unix_time),
    }
}

fn is_cacheable(plan: &MergePlan, stage_index: usize, job: &MergeJob) -> bool {
    stage_index == 0 && job.output != plan.output
}

fn plan_job(plan: &MergePlan, stage_index: usize, job_id: usize) -> Result<&MergeJob> {
    plan.stages
        .get(stage_index)
        .and_then(|stage| stage.jobs.iter().find(|job| job.id == job_id))
        .with_context(|| format!("could not find planned job {job_id} in stage {stage_index}"))
}

fn entry_is_valid(entry: &CacheEntry) -> Result<bool> {
    if !entry.manifest_path.is_file() || !entry.chunk_path.is_file() {
        return Ok(false);
    }

    let manifest = read_manifest(&entry.manifest_path)?;
    if manifest.schema_version != entry.manifest.schema_version
        || manifest.key != entry.manifest.key
        || manifest.radd_version != entry.manifest.radd_version
        || manifest.inputs != entry.manifest.inputs
        || manifest.options != entry.manifest.options
    {
        return Ok(false);
    }

    let metadata = fs::metadata(&entry.chunk_path)
        .with_context(|| format!("could not stat cache chunk: {}", entry.chunk_path.display()))?;

    Ok(metadata.is_file() && metadata.len() > 0 && metadata.len() == manifest.output_size_bytes)
}

fn copy_cached_chunk(entry: &CacheEntry, output: &Path) -> Result<()> {
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create scratch directory: {}", parent.display()))?;
    }

    fs::copy(&entry.chunk_path, output).with_context(|| {
        format!(
            "could not copy cached chunk {} to {}",
            entry.chunk_path.display(),
            output.display()
        )
    })?;

    Ok(())
}

fn store_output(source_output: &Path, entry: &CacheEntry) -> Result<()> {
    let metadata = fs::metadata(source_output)
        .with_context(|| format!("cannot cache missing output: {}", source_output.display()))?;

    if !metadata.is_file() || metadata.len() == 0 {
        bail!("cannot cache invalid output: {}", source_output.display());
    }

    fs::create_dir_all(entry.chunk_path.parent().expect("chunk parent"))?;
    fs::create_dir_all(entry.manifest_path.parent().expect("manifest parent"))?;

    let tmp_chunk = temporary_path(&entry.chunk_path);
    fs::copy(source_output, &tmp_chunk).with_context(|| {
        format!(
            "could not copy output {} into cache",
            source_output.display()
        )
    })?;
    replace_file(&tmp_chunk, &entry.chunk_path)?;

    let mut manifest = entry.manifest.clone();
    manifest.created_time = unix_time(SystemTime::now());
    manifest.output_size_bytes = metadata.len();
    let tmp_manifest = temporary_path(&entry.manifest_path);
    fs::write(&tmp_manifest, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("could not write cache manifest: {}", tmp_manifest.display()))?;
    replace_file(&tmp_manifest, &entry.manifest_path)?;

    Ok(())
}

fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(destination)?;
            fs::rename(source, destination)?;
            Ok(())
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "could not move {} to {}",
                source.display(),
                destination.display()
            )
        }),
    }
}

fn read_manifest(path: &Path) -> Result<CacheManifest> {
    let manifest: CacheManifest = serde_json::from_slice(
        &fs::read(path)
            .with_context(|| format!("could not read cache manifest: {}", path.display()))?,
    )
    .with_context(|| format!("could not parse cache manifest: {}", path.display()))?;

    Ok(manifest)
}

fn clean_directory_files(directory: &Path, report: &mut CleanReport) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(directory)
        .with_context(|| format!("could not read cache directory: {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file() {
            continue;
        }

        let size = fs::metadata(&path).map_or(0, |metadata| metadata.len());
        fs::remove_file(&path)
            .with_context(|| format!("could not remove cache file: {}", path.display()))?;
        report.removed_files += 1;
        report.removed_bytes = report.removed_bytes.saturating_add(size);
    }

    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let suffix = unix_time(SystemTime::now()).map_or(0, |time| time.nanos);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("tmp");

    path.with_extension(format!("{extension}.tmp-{}-{suffix}", std::process::id()))
}

fn unix_time(time: SystemTime) -> Option<CacheUnixTime> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;

    Some(CacheUnixTime {
        seconds: duration.as_secs(),
        nanos: duration.subsec_nanos(),
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use assert_fs::TempDir;

    use super::{CacheRoot, cache_key_for_job, clean_cache, list_cache, store_output};
    use crate::{
        hadd::{HaddOptions, ObjectSelection, ObjectSelectionMode},
        input::{InputFile, InputSet},
        planner::{MergeJob, MergePolicy},
    };

    #[test]
    fn cache_key_is_stable() {
        let first = cache_key_for_job(&job(), &input_set(7), &options()).expect("first key");
        let second = cache_key_for_job(&job(), &input_set(7), &options()).expect("second key");

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn cache_key_changes_when_input_size_changes() {
        let first = cache_key_for_job(&job(), &input_set(7), &options()).expect("first key");
        let second = cache_key_for_job(&job(), &input_set(8), &options()).expect("second key");

        assert_ne!(first, second);
    }

    #[test]
    fn cache_key_changes_when_flags_change() {
        let first = cache_key_for_job(&job(), &input_set(7), &options()).expect("first key");
        let second = cache_key_for_job(
            &job(),
            &input_set(7),
            &HaddOptions {
                no_trees: true,
                ..options()
            },
        )
        .expect("second key");

        assert_ne!(first, second);
    }

    #[test]
    fn cache_key_changes_when_hadd_version_changes() {
        let first = cache_key_for_job(&job(), &input_set(7), &options()).expect("first key");
        let second = cache_key_for_job(
            &job(),
            &input_set(7),
            &HaddOptions {
                version: Some("hadd fake 2.0".to_string()),
                ..options()
            },
        )
        .expect("second key");

        assert_ne!(first, second);
    }

    #[test]
    fn cache_key_changes_when_object_selection_changes() {
        let first = cache_key_for_job(&job(), &input_set(7), &options()).expect("first key");
        let second = cache_key_for_job(
            &job(),
            &input_set(7),
            &HaddOptions {
                object_selection: Some(ObjectSelection {
                    mode: ObjectSelectionMode::OnlyListed,
                    objects: vec!["DecayTree".to_string()],
                    list_path: PathBuf::from("scratch/objects-a.txt"),
                }),
                ..options()
            },
        )
        .expect("second key");
        let third = cache_key_for_job(
            &job(),
            &input_set(7),
            &HaddOptions {
                object_selection: Some(ObjectSelection {
                    mode: ObjectSelectionMode::OnlyListed,
                    objects: vec!["DecayTree".to_string()],
                    list_path: PathBuf::from("scratch/objects-b.txt"),
                }),
                ..options()
            },
        )
        .expect("third key");

        assert_ne!(first, second);
        assert_eq!(second, third);
    }

    #[test]
    fn list_and_clean_cache_entries() {
        let temp = TempDir::new().expect("temp dir");
        let root = CacheRoot::new(temp.path().join("cache"));
        let source = temp.path().join("chunk.root");
        fs::write(&source, b"cached chunk").expect("write chunk");
        let entry = super::cache_entry_for_job(&root, &job(), &input_set(7), &options())
            .expect("cache entry");

        store_output(&source, &entry).expect("store output");

        let list = list_cache(&root).expect("list cache");
        assert_eq!(list.entries.len(), 1);
        assert!(list.entries[0].complete);

        let report = clean_cache(&root).expect("clean cache");
        assert_eq!(report.removed_files, 2);
    }

    fn input_set(size_bytes: u64) -> InputSet {
        InputSet {
            files: vec![InputFile {
                path: PathBuf::from("/tmp/a.root"),
                size_bytes,
                modified_time: Some(SystemTime::UNIX_EPOCH),
            }],
            total_size_bytes: size_bytes,
        }
    }

    fn job() -> MergeJob {
        MergeJob {
            id: 0,
            output: PathBuf::from("scratch/chunk.root"),
            inputs: vec![PathBuf::from("/tmp/a.root")],
            input_size_bytes: 7,
            hadd_argv: None,
        }
    }

    fn options() -> HaddOptions {
        HaddOptions {
            executable: PathBuf::from("hadd"),
            version: Some("hadd fake 1.0".to_string()),
            policy: MergePolicy::Fastest,
            hadd_jobs: None,
            temp_dir: None,
            keep_going: false,
            max_open_files: None,
            no_trees: false,
            object_selection: None,
        }
    }
}
