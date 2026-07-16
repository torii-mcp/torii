use std::collections::HashSet;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::{ConfigPaths, ProviderPaths};
use crate::error::{Error, Result};
use crate::jasper::rules::{self, Rules};
use crate::providers::config::ProviderConfig;
use crate::providers::registry::{self, valid_name};

const PACKAGE_DIR: &str = ".torii-package";
const MANIFEST_FILE: &str = "manifest.yaml";
const LOCK_FILE: &str = "lock.yaml";
const MAX_DOWNLOAD_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 512;

pub const DEFAULT_CATALOG_URL: Option<&str> =
    Some("https://raw.githubusercontent.com/torii-mcp/torii-canon-providers/main/index.yaml");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub version: String,
    pub name: String,
    pub package_version: String,
    pub description: String,
    pub provider: String,
    pub rules: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default)]
    pub setups: Vec<SetupManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupManifest {
    pub name: String,
    pub kind: SetupKind,
    pub description: String,
    pub rules: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupKind {
    Readonly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageLock {
    pub version: String,
    pub name: String,
    pub package_version: String,
    pub source: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: String,
    #[serde(default)]
    pub providers: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub package_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Created,
    AlreadyExists,
}

struct LoadedPackage {
    root: PathBuf,
    manifest: PackageManifest,
    provider: ProviderConfig,
    digest: String,
    source: String,
    _temp: Option<tempfile::TempDir>,
}

pub async fn install(
    paths: &ConfigPaths,
    source: &str,
) -> Result<(InstallStatus, InstalledPackage)> {
    let package = load_package(source).await?;
    let installed = InstalledPackage {
        name: package.manifest.name.clone(),
        package_version: package.manifest.package_version.clone(),
    };
    let destination = paths.provider(&package.manifest.name);
    if destination.base().exists() {
        return Ok((InstallStatus::AlreadyExists, installed));
    }

    std::fs::create_dir_all(paths.providers()).map_err(|source| Error::Write {
        path: paths.providers(),
        source,
    })?;
    let temp = tempfile::Builder::new()
        .prefix(&format!(".{}-install-", package.manifest.name))
        .tempdir_in(paths.providers())
        .map_err(|source| Error::Write {
            path: paths.providers(),
            source,
        })?;
    let staged = ProviderPaths::new(temp.path().to_path_buf());
    staged.ensure()?;
    copy_file(&package.file(&package.manifest.provider)?, &staged.config())?;
    copy_file(&package.file(&package.manifest.rules)?, &staged.rules())?;
    if let Some(environment) = &package.manifest.environment {
        let destination = staged.base().join(&package.provider.environment.file);
        copy_file(&package.file(environment)?, &destination)?;
    }
    write_package_metadata(&package, staged.base())?;

    std::fs::rename(temp.path(), destination.base()).map_err(|source| Error::Write {
        path: destination.base().to_path_buf(),
        source,
    })?;
    Ok((InstallStatus::Created, installed))
}

pub fn setup(paths: &ConfigPaths, provider_name: &str, setup_name: &str) -> Result<()> {
    let provider = paths.provider(provider_name);
    let manifest = load_installed_manifest(&provider)?;
    let setup = manifest
        .setups
        .iter()
        .find(|setup| setup.name == setup_name)
        .ok_or_else(|| {
            let available = manifest
                .setups
                .iter()
                .map(|setup| setup.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Error::Package(format!(
                "unknown setup {setup_name:?} for provider {provider_name:?}; available: [{available}]"
            ))
        })?;
    let current = rules::load(&provider.rules())?;
    if !current.accept.is_empty() || !current.deny.is_empty() {
        return Err(Error::Package(format!(
            "rules.yaml for provider {provider_name:?} is not empty; refusing to overwrite"
        )));
    }
    let source = installed_content_path(&provider, &setup.rules)?;
    let candidate = rules::load(&source)?;
    let config = load_provider_config(&provider.config())?;
    if !candidate
        .invalid_accepts(config.policy.minimum_accept_tokens)
        .is_empty()
    {
        return Err(Error::Package(format!(
            "setup {setup_name:?} contains accept rules shorter than the provider minimum"
        )));
    }
    atomic_copy(&source, &provider.rules())
}

pub async fn update(paths: &ConfigPaths, provider_name: &str) -> Result<InstalledPackage> {
    let provider = paths.provider(provider_name);
    let lock = load_lock(&provider)?;
    let package = load_package(&lock.source).await?;
    if package.manifest.name != lock.name || package.manifest.name != provider_name {
        return Err(Error::Package(format!(
            "update resolved package {:?}, expected {provider_name:?}",
            package.manifest.name
        )));
    }

    let stage = tempfile::Builder::new()
        .prefix(".update-")
        .tempdir_in(provider.base())
        .map_err(|source| Error::Write {
            path: provider.base().to_path_buf(),
            source,
        })?;
    let staged_provider = stage.path().join("provider.yaml");
    copy_file(&package.file(&package.manifest.provider)?, &staged_provider)?;
    let staged_metadata = stage.path().join(PACKAGE_DIR);
    write_package_metadata_at(&package, &staged_metadata)?;
    replace_managed_files(&provider, &staged_provider, &staged_metadata)?;

    Ok(InstalledPackage {
        name: package.manifest.name.clone(),
        package_version: package.manifest.package_version.clone(),
    })
}

pub async fn search(query: Option<&str>) -> Result<Vec<CatalogEntry>> {
    let location = catalog_location()?;
    search_at(&location, query).await
}

async fn search_at(location: &str, query: Option<&str>) -> Result<Vec<CatalogEntry>> {
    let (catalog, _) = load_catalog(location).await?;
    let needle = query.unwrap_or_default().to_ascii_lowercase();
    Ok(catalog
        .providers
        .into_iter()
        .filter(|entry| {
            needle.is_empty()
                || entry.name.to_ascii_lowercase().contains(&needle)
                || entry.description.to_ascii_lowercase().contains(&needle)
        })
        .collect())
}

pub fn installed_lock(provider: &ProviderPaths) -> Option<PackageLock> {
    load_lock(provider).ok()
}

impl LoadedPackage {
    fn file(&self, relative: &str) -> Result<PathBuf> {
        package_file(&self.root, relative)
    }
}

async fn load_package(input: &str) -> Result<LoadedPackage> {
    if let Some(name) = input.strip_prefix("canonical:") {
        return load_canonical(name).await;
    }
    if let Some(path) = input.strip_prefix("path:") {
        let path = Path::new(path);
        return if path.is_dir() {
            open_package(path.to_path_buf(), input.into(), None, None)
        } else {
            load_local_archive_with(path, input.into(), None)
        };
    }
    if let Some(url) = input.strip_prefix("url:") {
        return load_remote_archive(url, input.into(), None).await;
    }
    let path = Path::new(input);
    if path.exists() {
        return if path.is_dir() {
            let path = canonical_path(path)?;
            open_package(path.clone(), format!("path:{}", path.display()), None, None)
        } else {
            load_local_archive(path)
        };
    }
    if input.starts_with("https://") || input.starts_with("http://") {
        return load_remote_archive(input, format!("url:{input}"), None).await;
    }
    if looks_like_path(input) {
        return Err(Error::Package(format!(
            "package path {input:?} does not exist"
        )));
    }
    load_canonical(input).await
}

async fn load_canonical(name: &str) -> Result<LoadedPackage> {
    if !valid_name(name) {
        return Err(Error::Package(format!(
            "invalid canonical package name {name:?}"
        )));
    }
    let location = catalog_location()?;
    load_canonical_at(name, &location).await
}

async fn load_canonical_at(name: &str, location: &str) -> Result<LoadedPackage> {
    let (catalog, base) = load_catalog(location).await?;
    let entry = catalog
        .providers
        .iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| {
            Error::Package(format!(
                "provider {name:?} was not found in the canonical catalog"
            ))
        })?;
    let resolved = resolve_catalog_source(&base, &entry.source)?;
    let mut package = if resolved.starts_with("https://") || resolved.starts_with("http://") {
        load_remote_archive(&resolved, format!("canonical:{name}"), Some(&entry.sha256)).await?
    } else {
        let path = Path::new(&resolved);
        if path.is_dir() {
            open_package(
                path.to_path_buf(),
                format!("canonical:{name}"),
                None,
                Some(&entry.sha256),
            )?
        } else {
            load_local_archive_with(path, format!("canonical:{name}"), Some(&entry.sha256))?
        }
    };
    if package.manifest.package_version != entry.version || package.manifest.name != entry.name {
        return Err(Error::Package(format!(
            "catalog metadata does not match package manifest for {name:?}"
        )));
    }
    package.source = format!("canonical:{name}");
    Ok(package)
}

fn load_local_archive(path: &Path) -> Result<LoadedPackage> {
    load_local_archive_with(
        path,
        format!("path:{}", canonical_path(path)?.display()),
        None,
    )
}

fn load_local_archive_with(
    path: &Path,
    source: String,
    expected_sha256: Option<&str>,
) -> Result<LoadedPackage> {
    let bytes = std::fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(Error::Package(
            "package archive exceeds the size limit".into(),
        ));
    }
    load_archive_bytes(
        &bytes,
        path.to_string_lossy().as_ref(),
        source,
        expected_sha256,
    )
}

async fn load_remote_archive(
    url: &str,
    source: String,
    expected_sha256: Option<&str>,
) -> Result<LoadedPackage> {
    let bytes = download(url).await?;
    load_archive_bytes(&bytes, url, source, expected_sha256)
}

fn load_archive_bytes(
    bytes: &[u8],
    hint: &str,
    source: String,
    expected_sha256: Option<&str>,
) -> Result<LoadedPackage> {
    verify_sha256(bytes, expected_sha256)?;
    let temp = tempfile::TempDir::new().map_err(|source| Error::Write {
        path: std::env::temp_dir(),
        source,
    })?;
    extract_archive(bytes, hint, temp.path())?;
    let root = locate_package_root(temp.path())?;
    open_package(root, source, Some(temp), None)
}

fn open_package(
    root: PathBuf,
    source: String,
    temp: Option<tempfile::TempDir>,
    expected_sha256: Option<&str>,
) -> Result<LoadedPackage> {
    let manifest_path = root.join(MANIFEST_FILE);
    let manifest: PackageManifest = load_yaml(&manifest_path)?;
    validate_manifest(&manifest)?;
    let provider_path = package_file(&root, &manifest.provider)?;
    let provider = load_provider_config(&provider_path)?;
    registry::validate(&provider, &root)?;
    if provider.name != manifest.name {
        return Err(Error::Package(format!(
            "package name {:?} does not match provider name {:?}",
            manifest.name, provider.name
        )));
    }
    let base_rules = load_rules(&package_file(&root, &manifest.rules)?)?;
    if !base_rules.accept.is_empty() || !base_rules.deny.is_empty() {
        return Err(Error::Package(
            "package rules.yaml must contain empty accept and deny lists".into(),
        ));
    }
    for setup in &manifest.setups {
        let setup_rules = load_rules(&package_file(&root, &setup.rules)?)?;
        if !setup_rules
            .invalid_accepts(provider.policy.minimum_accept_tokens)
            .is_empty()
        {
            return Err(Error::Package(format!(
                "setup {:?} contains accept rules shorter than the provider minimum",
                setup.name
            )));
        }
    }
    if let Some(environment) = &manifest.environment {
        package_file(&root, environment)?;
    }
    let digest = digest_package(&root, &manifest)?;
    if let Some(expected) = expected_sha256 {
        if !expected.eq_ignore_ascii_case(&digest) {
            return Err(Error::Package(
                "package directory digest does not match the catalog".into(),
            ));
        }
    }
    Ok(LoadedPackage {
        root,
        manifest,
        provider,
        digest,
        source,
        _temp: temp,
    })
}

fn validate_manifest(manifest: &PackageManifest) -> Result<()> {
    if manifest.version != "1" {
        return Err(Error::Package(format!(
            "unsupported package manifest version {:?}",
            manifest.version
        )));
    }
    if !valid_name(&manifest.name) {
        return Err(Error::Package(format!(
            "invalid package name {:?}",
            manifest.name
        )));
    }
    if manifest.package_version.trim().is_empty() || manifest.description.trim().is_empty() {
        return Err(Error::Package(
            "package_version and description cannot be empty".into(),
        ));
    }
    let mut names = HashSet::new();
    for setup in &manifest.setups {
        if !valid_name(&setup.name) || !names.insert(setup.name.as_str()) {
            return Err(Error::Package(format!(
                "invalid or duplicate setup {:?}",
                setup.name
            )));
        }
        if setup.description.trim().is_empty() {
            return Err(Error::Package(format!(
                "setup {:?} has an empty description",
                setup.name
            )));
        }
        safe_relative(&setup.rules)?;
    }
    safe_relative(&manifest.provider)?;
    safe_relative(&manifest.rules)?;
    if let Some(environment) = &manifest.environment {
        safe_relative(environment)?;
    }
    Ok(())
}

fn write_package_metadata(package: &LoadedPackage, provider_root: &Path) -> Result<()> {
    write_package_metadata_at(package, &provider_root.join(PACKAGE_DIR))
}

fn write_package_metadata_at(package: &LoadedPackage, metadata: &Path) -> Result<()> {
    std::fs::create_dir_all(metadata.join("content")).map_err(|source| Error::Write {
        path: metadata.to_path_buf(),
        source,
    })?;
    write_yaml(&metadata.join(MANIFEST_FILE), &package.manifest)?;
    let lock = PackageLock {
        version: "1".into(),
        name: package.manifest.name.clone(),
        package_version: package.manifest.package_version.clone(),
        source: package.source.clone(),
        sha256: package.digest.clone(),
    };
    write_yaml(&metadata.join(LOCK_FILE), &lock)?;
    for setup in &package.manifest.setups {
        let source = package.file(&setup.rules)?;
        let destination = metadata.join("content").join(&setup.rules);
        copy_file(&source, &destination)?;
    }
    Ok(())
}

fn replace_managed_files(
    provider: &ProviderPaths,
    staged_provider: &Path,
    staged_metadata: &Path,
) -> Result<()> {
    let provider_backup = provider.base().join(".provider.yaml.update-backup");
    let metadata = provider.base().join(PACKAGE_DIR);
    let metadata_backup = provider.base().join(".torii-package.update-backup");
    if provider_backup.exists() || metadata_backup.exists() {
        return Err(Error::Package(
            "stale update backup exists; inspect the provider directory before retrying".into(),
        ));
    }
    reject_symlink(&provider.config())?;
    reject_symlink(&metadata)?;
    std::fs::rename(provider.config(), &provider_backup).map_err(|source| Error::Write {
        path: provider_backup.clone(),
        source,
    })?;
    if let Err(error) = std::fs::rename(&metadata, &metadata_backup) {
        let _ = std::fs::rename(&provider_backup, provider.config());
        return Err(Error::Write {
            path: metadata_backup,
            source: error,
        });
    }
    let install_result = std::fs::rename(staged_provider, provider.config())
        .and_then(|_| std::fs::rename(staged_metadata, &metadata));
    if let Err(source) = install_result {
        let _ = std::fs::remove_file(provider.config());
        let _ = std::fs::remove_dir_all(&metadata);
        let _ = std::fs::rename(&provider_backup, provider.config());
        let _ = std::fs::rename(&metadata_backup, &metadata);
        return Err(Error::Write {
            path: provider.base().to_path_buf(),
            source,
        });
    }
    let _ = std::fs::remove_file(provider_backup);
    let _ = std::fs::remove_dir_all(metadata_backup);
    Ok(())
}

fn load_installed_manifest(provider: &ProviderPaths) -> Result<PackageManifest> {
    load_yaml(&provider.base().join(PACKAGE_DIR).join(MANIFEST_FILE))
}

fn load_lock(provider: &ProviderPaths) -> Result<PackageLock> {
    load_yaml(&provider.base().join(PACKAGE_DIR).join(LOCK_FILE))
}

fn installed_content_path(provider: &ProviderPaths, relative: &str) -> Result<PathBuf> {
    let base = provider.base().join(PACKAGE_DIR).join("content");
    package_file(&base, relative)
}

fn package_file(base: &Path, relative: &str) -> Result<PathBuf> {
    let relative = safe_relative(relative)?;
    let path = base.join(relative);
    let metadata = std::fs::symlink_metadata(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Error::Package(format!(
            "package path {} is not a regular file",
            path.display()
        )));
    }
    Ok(path)
}

fn safe_relative(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(Error::Package(format!(
            "package path {value:?} must stay inside the package"
        )));
    }
    Ok(path.to_path_buf())
}

fn reject_symlink(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(Error::Package(format!(
            "refusing to update symlink {}",
            path.display()
        )));
    }
    Ok(())
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::Package("rules destination has no parent".into()))?;
    let bytes = std::fs::read(source).map_err(|error| Error::Read {
        path: source.to_path_buf(),
        source: error,
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|source| Error::Write {
        path: destination.to_path_buf(),
        source,
    })?;
    temp.write_all(&bytes)
        .and_then(|_| temp.flush())
        .map_err(|source| Error::Write {
            path: destination.to_path_buf(),
            source,
        })?;
    temp.persist(destination).map_err(|error| Error::Write {
        path: destination.to_path_buf(),
        source: error.error,
    })?;
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::copy(source, destination)
        .map(|_| ())
        .map_err(|source| Error::Write {
            path: destination.to_path_buf(),
            source,
        })
}

fn load_provider_config(path: &Path) -> Result<ProviderConfig> {
    load_yaml(path)
}

fn load_rules(path: &Path) -> Result<Rules> {
    rules::load(path)
}

fn load_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let contents = serde_yaml::to_string(value).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, contents).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn digest_package(root: &Path, manifest: &PackageManifest) -> Result<String> {
    let mut paths = vec![manifest.provider.as_str(), manifest.rules.as_str()];
    if let Some(environment) = &manifest.environment {
        paths.push(environment);
    }
    for setup in &manifest.setups {
        paths.push(&setup.rules);
    }
    paths.sort_unstable();
    let mut digest = Sha256::new();
    let manifest_bytes = serde_yaml::to_string(manifest).map_err(|source| Error::Yaml {
        path: root.join(MANIFEST_FILE),
        source,
    })?;
    digest.update(MANIFEST_FILE.as_bytes());
    digest.update(manifest_bytes.as_bytes());
    for relative in paths {
        digest.update(relative.as_bytes());
        let path = package_file(root, relative)?;
        digest.update(std::fs::read(&path).map_err(|source| Error::Read { path, source })?);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_sha256(bytes: &[u8], expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(Error::Package(
            "archive SHA-256 does not match the catalog".into(),
        ));
    }
    Ok(())
}

fn extract_archive(bytes: &[u8], hint: &str, destination: &Path) -> Result<()> {
    if bytes.starts_with(b"PK\x03\x04") || hint.to_ascii_lowercase().ends_with(".zip") {
        return extract_zip(bytes, destination);
    }
    if bytes.starts_with(&[0x1f, 0x8b])
        || hint.to_ascii_lowercase().ends_with(".tar.gz")
        || hint.to_ascii_lowercase().ends_with(".tgz")
    {
        let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
        return extract_tar(decoder, destination);
    }
    if hint.to_ascii_lowercase().ends_with(".tar") {
        return extract_tar(Cursor::new(bytes), destination);
    }
    Err(Error::Package(
        "unsupported package archive; expected .zip, .tar or .tar.gz/.tgz".into(),
    ))
}

fn extract_zip(bytes: &[u8], destination: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Error::Package(format!("invalid zip archive: {error}")))?;
    if archive.len() > MAX_ARCHIVE_FILES {
        return Err(Error::Package("archive contains too many files".into()));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| Error::Package(format!("invalid zip entry: {error}")))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| Error::Package("zip entry escapes the package root".into()))?
            .to_path_buf();
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(Error::Package(
                "symlinks are not allowed in packages".into(),
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_EXTRACTED_BYTES {
            return Err(Error::Package(
                "archive expands beyond the size limit".into(),
            ));
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output).map_err(|source| Error::Write {
                path: output,
                source,
            })?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|source| Error::Write {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let mut file = std::fs::File::create(&output).map_err(|source| Error::Write {
                path: output.clone(),
                source,
            })?;
            std::io::copy(&mut entry, &mut file).map_err(|source| Error::Write {
                path: output,
                source,
            })?;
        }
    }
    Ok(())
}

fn extract_tar<R: Read>(reader: R, destination: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut count = 0_usize;
    let mut total = 0_u64;
    let entries = archive
        .entries()
        .map_err(|error| Error::Package(format!("invalid tar archive: {error}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|error| Error::Package(format!("invalid tar entry: {error}")))?;
        count += 1;
        if count > MAX_ARCHIVE_FILES {
            return Err(Error::Package("archive contains too many files".into()));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(Error::Package(
                "links and special files are not allowed in packages".into(),
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_EXTRACTED_BYTES {
            return Err(Error::Package(
                "archive expands beyond the size limit".into(),
            ));
        }
        let relative = entry
            .path()
            .map_err(|error| Error::Package(format!("invalid tar path: {error}")))?;
        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(Error::Package("tar entry escapes the package root".into()));
        }
        let unpacked = entry
            .unpack_in(destination)
            .map_err(|error| Error::Package(format!("failed to extract tar entry: {error}")))?;
        if !unpacked {
            return Err(Error::Package("tar entry escapes the package root".into()));
        }
    }
    Ok(())
}

fn locate_package_root(extracted: &Path) -> Result<PathBuf> {
    if extracted.join(MANIFEST_FILE).is_file() {
        return Ok(extracted.to_path_buf());
    }
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(extracted).map_err(|source| Error::Read {
        path: extracted.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Read {
            path: extracted.to_path_buf(),
            source,
        })?;
        if entry
            .file_type()
            .map_err(|source| Error::Read {
                path: entry.path(),
                source,
            })?
            .is_dir()
            && entry.path().join(MANIFEST_FILE).is_file()
        {
            candidates.push(entry.path());
        }
    }
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err(Error::Package(
            "archive must contain one package root with manifest.yaml".into(),
        ))
    }
}

async fn download(url: &str) -> Result<Vec<u8>> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| Error::Package("invalid provider package URL".into()))?;
    if parsed.scheme() != "https" {
        return Err(Error::Package(
            "remote provider packages and catalogs require HTTPS".into(),
        ));
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(Error::Package(
            "provider URLs cannot contain credentials, query strings or fragments".into(),
        ));
    }
    let response = reqwest::Client::new()
        .get(parsed)
        .send()
        .await
        .map_err(|_| Error::Package("failed to download provider package".into()))?;
    if !response.status().is_success() || response.url().scheme() != "https" {
        return Err(Error::Package(format!(
            "provider package download returned HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD_BYTES)
    {
        return Err(Error::Package(
            "provider package exceeds the size limit".into(),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| Error::Package("failed to read provider package download".into()))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(Error::Package(
            "provider package exceeds the size limit".into(),
        ));
    }
    Ok(bytes.to_vec())
}

fn catalog_location() -> Result<String> {
    std::env::var("TORII_PROVIDER_CATALOG")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| DEFAULT_CATALOG_URL.map(str::to_string))
        .ok_or_else(|| {
            Error::Package(
                "canonical provider catalog is not configured; set TORII_PROVIDER_CATALOG".into(),
            )
        })
}

async fn load_catalog(location: &str) -> Result<(Catalog, String)> {
    let (contents, base) = if location.starts_with("https://") || location.starts_with("http://") {
        let bytes = download(location).await?;
        let url = reqwest::Url::parse(location)
            .map_err(|_| Error::Package("invalid catalog URL".into()))?;
        let base = url
            .join(".")
            .map_err(|_| Error::Package("invalid catalog base URL".into()))?
            .to_string();
        (
            String::from_utf8(bytes).map_err(|_| Error::Package("catalog is not UTF-8".into()))?,
            base,
        )
    } else {
        let path = canonical_path(Path::new(location))?;
        let contents = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let base = path
            .parent()
            .ok_or_else(|| Error::Package("catalog path has no parent".into()))?
            .to_string_lossy()
            .into_owned();
        (contents, base)
    };
    let catalog: Catalog = serde_yaml::from_str(&contents)
        .map_err(|error| Error::Package(format!("invalid provider catalog: {error}")))?;
    if catalog.version != "1" {
        return Err(Error::Package(format!(
            "unsupported provider catalog version {:?}",
            catalog.version
        )));
    }
    let mut names = HashSet::new();
    for entry in &catalog.providers {
        if !valid_name(&entry.name) || !names.insert(entry.name.as_str()) {
            return Err(Error::Package(format!(
                "invalid or duplicate catalog provider {:?}",
                entry.name
            )));
        }
        if entry.version.trim().is_empty()
            || entry.description.trim().is_empty()
            || entry.source.trim().is_empty()
            || entry.sha256.len() != 64
            || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::Package(format!(
                "incomplete catalog entry for {:?}",
                entry.name
            )));
        }
    }
    Ok((catalog, base))
}

fn resolve_catalog_source(base: &str, source: &str) -> Result<String> {
    if source.starts_with("https://") || source.starts_with("http://") {
        return Ok(source.into());
    }
    if base.starts_with("https://") {
        return reqwest::Url::parse(base)
            .and_then(|url| url.join(source))
            .map(|url| url.to_string())
            .map_err(|_| Error::Package("invalid relative source in catalog".into()));
    }
    Ok(Path::new(base).join(source).to_string_lossy().into_owned())
}

fn canonical_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn looks_like_path(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || value.starts_with('.')
        || [".zip", ".tar", ".tar.gz", ".tgz"]
            .iter()
            .any(|extension| value.to_ascii_lowercase().ends_with(extension))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn package(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("providers")
            .join(name)
    }

    #[tokio::test]
    async fn install_creates_empty_rules_and_setup_is_the_only_policy_writer() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().join("config"));
        let source = package("aws").to_string_lossy().into_owned();
        let (status, installed) = install(&paths, &source).await.unwrap();
        assert_eq!(status, InstallStatus::Created);
        assert_eq!(installed.name, "aws");
        let active = rules::load(&paths.provider("aws").rules()).unwrap();
        assert!(active.accept.is_empty() && active.deny.is_empty());

        setup(&paths, "aws", "readonly").unwrap();
        let active = rules::load(&paths.provider("aws").rules()).unwrap();
        assert!(active.accept.contains(&"ec2 describe-instances".into()));
        assert!(setup(&paths, "aws", "readonly").is_err());
    }

    #[tokio::test]
    async fn update_preserves_rules_environment_and_runtime_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().join("config"));
        let source = package("aws").to_string_lossy().into_owned();
        install(&paths, &source).await.unwrap();
        let provider = paths.provider("aws");
        std::fs::write(
            provider.rules(),
            "version: '1.0'\ndeny: []\naccept: ['custom read']\n",
        )
        .unwrap();
        std::fs::write(provider.env(), "CUSTOM=1\n").unwrap();
        std::fs::write(provider.grants(), "future grant\n").unwrap();

        update(&paths, "aws").await.unwrap();

        assert!(std::fs::read_to_string(provider.rules())
            .unwrap()
            .contains("custom read"));
        assert_eq!(
            std::fs::read_to_string(provider.env()).unwrap(),
            "CUSTOM=1\n"
        );
        assert_eq!(
            std::fs::read_to_string(provider.grants()).unwrap(),
            "future grant\n"
        );
    }

    #[test]
    fn package_rejects_non_empty_base_rules() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join(MANIFEST_FILE),
            "version: '1'\nname: bad\npackage_version: 1.0.0\ndescription: bad\nprovider: provider.yaml\nrules: rules.yaml\n",
        )
        .unwrap();
        std::fs::write(
            root.join("provider.yaml"),
            "version: '1'\nname: bad\ntool: bad\ndescription: bad\ncommand: bad\n",
        )
        .unwrap();
        std::fs::write(
            root.join("rules.yaml"),
            "version: '1.0'\ndeny: []\naccept: ['bad read']\n",
        )
        .unwrap();
        assert!(open_package(root.to_path_buf(), "test".into(), None, None).is_err());
    }

    #[tokio::test]
    async fn installs_zip_and_tar_gz_archives() {
        let temp = tempfile::TempDir::new().unwrap();
        let zip_path = temp.path().join("aws.zip");
        write_zip(&package("aws"), &zip_path);
        let config_zip = ConfigPaths::new(temp.path().join("config-zip"));
        install(&config_zip, zip_path.to_str().unwrap())
            .await
            .unwrap();
        assert!(config_zip.provider("aws").config().is_file());

        let tar_path = temp.path().join("aws.tar.gz");
        let file = std::fs::File::create(&tar_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        archive.append_dir_all("aws", package("aws")).unwrap();
        let encoder = archive.into_inner().unwrap();
        encoder.finish().unwrap();
        let config_tar = ConfigPaths::new(temp.path().join("config-tar"));
        install(&config_tar, tar_path.to_str().unwrap())
            .await
            .unwrap();
        assert!(config_tar.provider("aws").config().is_file());
    }

    #[tokio::test]
    async fn canonical_catalog_supports_search_and_name_resolution() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = package("aws");
        let loaded = open_package(root.clone(), "test".into(), None, None).unwrap();
        let catalog_path = temp.path().join("index.yaml");
        std::fs::write(
            &catalog_path,
            format!(
                "version: '1'\nproviders:\n  - name: aws\n    version: '{}'\n    description: Provider AWS\n    source: '{}'\n    sha256: {}\n",
                loaded.manifest.package_version,
                root.to_string_lossy().replace('\'', "''"),
                loaded.digest
            ),
        )
        .unwrap();

        let results = search_at(catalog_path.to_str().unwrap(), Some("aws"))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        let resolved = load_canonical_at("aws", catalog_path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(resolved.manifest.name, "aws");
        assert_eq!(resolved.source, "canonical:aws");
    }

    fn write_zip(source: &Path, destination: &Path) {
        let file = std::fs::File::create(destination).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        append_zip_dir(&mut archive, source, source, "aws");
        archive.finish().unwrap();
    }

    fn append_zip_dir(
        archive: &mut zip::ZipWriter<std::fs::File>,
        root: &Path,
        directory: &Path,
        prefix: &str,
    ) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                append_zip_dir(archive, root, &path, prefix);
                continue;
            }
            let relative = path.strip_prefix(root).unwrap().to_string_lossy();
            let name = format!("{prefix}/{}", relative.replace('\\', "/"));
            archive
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            let bytes = std::fs::read(path).unwrap();
            archive.write_all(&bytes).unwrap();
        }
    }
}
