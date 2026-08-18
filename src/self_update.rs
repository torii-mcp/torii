//! Atualização do próprio binário. Como todo comando de control plane, é humano:
//! nenhuma tool MCP troca o executável que aplica a política.

use std::path::{Path, PathBuf};

use sha2::Digest;

use crate::error::{Error, Result};
use crate::providers::packages;

const REPO: &str = "torii-mcp/torii";
const LATEST_URL: &str = "https://github.com/torii-mcp/torii/releases/latest";

struct Platform {
    /// Sufixo do pacote publicado, como `linux-x86_64`.
    slug: &'static str,
    archive_extension: &'static str,
    binary: &'static str,
}

pub async fn update(check_only: bool) -> Result<i32> {
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    let latest = latest_version().await?;

    if latest == current {
        eprintln!("torii {current} is already the latest release.");
        return Ok(0);
    }
    eprintln!("A new Torii release is available: {current} -> {latest}");
    if check_only {
        eprintln!("Run `torii self update` to install it.");
        return Ok(0);
    }

    let platform = platform()?;
    let package = format!("torii-{latest}-{}", platform.slug);
    let archive = format!("{package}.{}", platform.archive_extension);
    let base = format!("https://github.com/{REPO}/releases/download/{latest}");

    eprintln!("Downloading {archive}");
    let bytes = packages::download(&format!("{base}/{archive}")).await?;
    let checksum = packages::download(&format!("{base}/{archive}.sha256")).await?;
    verify_checksum(&bytes, &checksum, &archive)?;
    eprintln!("Checksum verified");

    let staging = tempfile::tempdir().map_err(|error| {
        Error::Package(format!(
            "could not create the update staging directory: {error}"
        ))
    })?;
    packages::extract_archive(&bytes, &archive, staging.path())?;
    let extracted = staging.path().join(&package).join(platform.binary);
    if !extracted.is_file() {
        return Err(Error::Package(format!(
            "the release archive did not contain {}",
            platform.binary
        )));
    }

    let destination = std::env::current_exe()
        .map_err(|error| Error::Package(format!("could not locate the running Torii: {error}")))?;
    replace_executable(&extracted, &destination)?;

    eprintln!("Updated to {latest} at {}", destination.display());
    eprintln!(
        "Restart any agent client and MCP session still running the previous binary. Providers, policy, targets and credentials are untouched."
    );
    Ok(0)
}

/// A tag sai do redirecionamento de `/releases/latest`, que não exige credencial
/// nem consome cota da API do GitHub.
async fn latest_version() -> Result<String> {
    let response = reqwest::Client::new()
        .get(LATEST_URL)
        .send()
        .await
        .map_err(|_| Error::Package("could not reach the Torii release feed".into()))?;
    if !response.status().is_success() {
        return Err(Error::Package(format!(
            "the Torii release feed returned HTTP {}",
            response.status()
        )));
    }
    let url = response.url().clone();
    if url.scheme() != "https" {
        return Err(Error::Package(
            "the release feed redirected off HTTPS".into(),
        ));
    }
    let tag = url
        .path()
        .rsplit_once("/tag/")
        .map(|(_, tag)| tag.trim_end_matches('/'))
        .filter(|tag| tag.starts_with('v') && tag.len() > 1)
        .ok_or_else(|| {
            Error::Package("could not read the latest Torii version from the release feed".into())
        })?;
    Ok(tag.to_string())
}

fn platform() -> Result<Platform> {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        return Ok(Platform {
            slug: "windows-x86_64",
            archive_extension: "zip",
            binary: "torii.exe",
        });
    }
    if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        return Ok(Platform {
            slug: "linux-x86_64",
            archive_extension: "tar.gz",
            binary: "torii",
        });
    }
    Err(Error::Package(format!(
        "no Torii release is published for {}-{}; update by building from source",
        std::env::consts::OS,
        std::env::consts::ARCH
    )))
}

/// O arquivo publicado tem a forma `<hash>  <nome>`; a comparação usa só o hash.
/// Ele protege contra download corrompido ou truncado, não substitui assinatura.
fn verify_checksum(bytes: &[u8], checksum: &[u8], archive: &str) -> Result<()> {
    let text = String::from_utf8_lossy(checksum);
    let expected = text
        .split_whitespace()
        .next()
        .ok_or_else(|| Error::Package("the published checksum file was empty".into()))?;
    let actual = format!("{:x}", sha2::Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(Error::Package(format!(
            "checksum mismatch for {archive}; the download was not installed"
        )));
    }
    Ok(())
}

/// No Windows o executável em uso não pode ser sobrescrito, mas pode ser renomeado:
/// o processo atual continua com o arquivo antigo e o novo assume o nome.
fn replace_executable(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        Error::Package(format!(
            "the running Torii has no parent directory: {}",
            destination.display()
        ))
    })?;
    // Acrescenta o sufixo em vez de trocar a extensão: `torii.exe` vira
    // `torii.exe.old`, que ninguém confunde com outro arquivo.
    let mut retired = destination.as_os_str().to_owned();
    retired.push(".old");
    let retired = PathBuf::from(retired);
    let staged = parent.join("torii.new");

    std::fs::copy(source, &staged).map_err(|source| Error::Write {
        path: staged.clone(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(
            |source| Error::Write {
                path: staged.clone(),
                source,
            },
        )?;
    }

    let _ = std::fs::remove_file(&retired);
    if destination.exists() {
        std::fs::rename(destination, &retired).map_err(|source| Error::Write {
            path: destination.to_path_buf(),
            source,
        })?;
    }
    if let Err(source) = std::fs::rename(&staged, destination) {
        // Sem o binário no lugar, devolver o antigo é melhor que deixar o buraco.
        let _ = std::fs::rename(&retired, destination);
        let _ = std::fs::remove_file(&staged);
        return Err(Error::Write {
            path: destination.to_path_buf(),
            source,
        });
    }
    // No Windows o binário que está executando este código não pode ser apagado;
    // ele fica para trás e a próxima atualização o remove.
    if std::fs::remove_file(&retired).is_err() && retired.exists() {
        eprintln!(
            "The previous binary is still in use and was left at {}; delete it when convenient.",
            retired.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_accepts_the_published_two_field_format() {
        let bytes = b"torii";
        let digest = format!("{:x}", sha2::Sha256::digest(bytes));
        let published = format!("{digest}  torii-v9.9.9-linux-x86_64.tar.gz\n");
        assert!(verify_checksum(bytes, published.as_bytes(), "archive").is_ok());
    }

    #[test]
    fn checksum_tolerates_crlf_and_uppercase() {
        let bytes = b"torii";
        let digest = format!("{:X}", sha2::Sha256::digest(bytes));
        let published = format!("{digest}  torii.zip\r\n");
        assert!(verify_checksum(bytes, published.as_bytes(), "archive").is_ok());
    }

    #[test]
    fn a_mismatched_checksum_fails_closed() {
        let published = format!("{}  torii.zip\n", "0".repeat(64));
        assert!(verify_checksum(b"torii", published.as_bytes(), "archive").is_err());
    }

    #[test]
    fn an_empty_checksum_file_fails_closed() {
        assert!(verify_checksum(b"torii", b"   \n", "archive").is_err());
    }

    #[test]
    fn replacing_the_executable_keeps_the_new_content() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir
            .path()
            .join(if cfg!(windows) { "torii.exe" } else { "torii" });
        std::fs::write(&destination, b"old").unwrap();
        let source = dir.path().join("downloaded");
        std::fs::write(&source, b"new").unwrap();

        replace_executable(&source, &destination).unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!dir.path().join("torii.new").exists());
    }

    #[test]
    fn replacing_works_when_no_binary_is_there_yet() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("torii");
        let source = dir.path().join("downloaded");
        std::fs::write(&source, b"new").unwrap();

        replace_executable(&source, &destination).unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
    }
}
