//! Edição humana das políticas Jasper. Este módulo pertence ao control plane e
//! nunca é exposto como tool MCP: o agente não edita a política que o limita.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::ConfigPaths;
use crate::error::{Error, Result};
use crate::jasper::rules::Rules;
use crate::providers::{Provider, ProviderRegistry};

/// Política inicial de um target, criada apenas com `--create`.
const TARGET_TEMPLATE: &str = "version: \"1.0\"\n\n\
     # Esta política substitui a compartilhada do provider somente neste target.\n\
     # Enquanto os dois vetores estiverem vazios, nada atravessa neste alias.\n\
     deny: []\n\
     accept: []\n";

pub fn show(paths: &ConfigPaths, tool: &str, target: Option<&str>) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = resolve_provider(&registry, tool)?;
    let (path, scope) = rules_path(&provider, tool, target)?;
    if !path.exists() {
        if target.is_some() {
            eprintln!(
                "{scope} has no rules of its own; the shared provider policy applies. Run `torii policy show {tool}` to read it."
            );
            return Ok(());
        }
        return Err(Error::RulesNotFound(path));
    }

    let text = read_text(&path)?;
    let parsed: Rules = parse(&text, &path)?;
    parsed.compile()?;

    println!("# {}", path.display());
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    warn_ignored_accepts(&parsed, provider.config.policy.minimum_accept_tokens);
    Ok(())
}

pub fn edit(paths: &ConfigPaths, tool: &str, target: Option<&str>, create: bool) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = resolve_provider(&registry, tool)?;
    let (path, scope) = rules_path(&provider, tool, target)?;
    let minimum = provider.config.policy.minimum_accept_tokens;

    let original = match (path.exists(), target) {
        (true, _) => read_text(&path)?,
        // Criar rules de target é uma decisão semântica: elas substituem a política
        // compartilhada naquele alias, então exigem um pedido explícito.
        (false, Some(_)) if create => TARGET_TEMPLATE.to_string(),
        (false, Some(_)) => {
            return Err(Error::InvalidArguments(format!(
                "{scope} has no rules of its own and inherits the shared provider policy; pass --create to start a target policy that replaces it"
            )));
        }
        (false, None) => return Err(Error::RulesNotFound(path)),
    };

    let draft = write_draft(tool, target, &original)?;

    let edited = loop {
        launch_editor(&draft)?;
        let edited = read_text(&draft)?;
        if edited == original {
            eprintln!("{scope} was not changed.");
            return Ok(());
        }
        match validate(&edited, &draft) {
            Ok(rules) => break rules,
            Err(error) => {
                eprintln!("{error}");
                if !retry_confirmed() {
                    // O rascunho é preservado para não perder a edição recusada.
                    let kept = draft.keep().map_err(|error| {
                        Error::Agent(format!("could not preserve the policy draft: {error}"))
                    })?;
                    return Err(Error::InvalidArguments(format!(
                        "{scope} was left unchanged; your draft is kept at {}",
                        kept.display()
                    )));
                }
            }
        }
    };

    let text = read_text(&draft)?;
    write_atomic(&path, &text)?;
    eprintln!("{scope} updated at {}.", path.display());
    eprintln!(
        "{} deny and {} accept rule(s) are active. Rules are reread on every call, so the MCP server does not need a restart.",
        edited.deny.len(),
        edited.accept.len()
    );
    warn_ignored_accepts(&edited, minimum);
    Ok(())
}

/// Uma política só substitui a anterior depois de parsear e de compilar cada
/// regra: um regex inválido em disco faz a avaliação falhar fechada em runtime.
fn validate(text: &str, path: &Path) -> Result<Rules> {
    if text.trim().is_empty() {
        return Err(Error::InvalidArguments(
            "the policy cannot be empty; use `deny: []` and `accept: []` to keep denying everything"
                .into(),
        ));
    }
    let rules: Rules = parse(text, path)?;
    rules.compile()?;
    Ok(rules)
}

fn warn_ignored_accepts(rules: &Rules, minimum: usize) {
    for rule in rules.invalid_accepts(minimum) {
        eprintln!(
            "warning: accept rule {rule:?} has fewer than minimum_accept_tokens ({minimum}) tokens and is ignored during evaluation"
        );
    }
}

fn parse(text: &str, path: &Path) -> Result<Rules> {
    serde_yaml::from_str(text).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

fn resolve_provider(registry: &ProviderRegistry, tool: &str) -> Result<Arc<Provider>> {
    registry
        .get(tool)
        .ok_or_else(|| Error::ProviderNotFound(tool.to_string()))
}

/// Caminho da política e um rótulo humano do escopo editado.
fn rules_path(provider: &Provider, tool: &str, target: Option<&str>) -> Result<(PathBuf, String)> {
    match target {
        None => Ok((provider.paths.rules(), format!("policy for {tool:?}"))),
        Some(name) => {
            if !provider.uses_targets() {
                return Err(Error::InvalidArguments(format!(
                    "provider tool {tool:?} does not use targets, so it has a single shared policy"
                )));
            }
            if !provider.targets.contains_key(name) {
                return Err(Error::InvalidArguments(format!(
                    "target {name:?} is not configured for tool {tool:?}; run `torii target list {tool}`"
                )));
            }
            Ok((
                provider.paths.target(name).rules(),
                format!("policy for {tool:?} target {name:?}"),
            ))
        }
    }
}

/// O rascunho é fechado antes de o editor abrir: no Windows um handle nosso ainda
/// aberto faria o editor falhar com o arquivo em uso.
fn write_draft(tool: &str, target: Option<&str>, contents: &str) -> Result<tempfile::TempPath> {
    let prefix = match target {
        None => format!("torii-{tool}-rules-"),
        Some(name) => format!("torii-{tool}-{name}-rules-"),
    };
    let draft = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".yaml")
        .tempfile()
        .map_err(|error| Error::Agent(format!("could not create the policy draft: {error}")))?
        .into_temp_path();
    std::fs::write(&draft, contents).map_err(|source| Error::Write {
        path: draft.to_path_buf(),
        source,
    })?;
    Ok(draft)
}

/// `$VISUAL` e `$EDITOR` podem trazer argumentos, como `code --wait`.
fn editor_command() -> (String, Vec<String>) {
    let configured = ["VISUAL", "EDITOR"].into_iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    });
    let Some(configured) = configured else {
        let fallback = if cfg!(windows) { "notepad" } else { "vi" };
        return (fallback.into(), Vec::new());
    };
    let mut parts = configured.split_whitespace().map(str::to_string);
    let program = parts.next().unwrap_or_else(|| "vi".into());
    (program, parts.collect())
}

fn launch_editor(path: &Path) -> Result<()> {
    let (program, args) = editor_command();
    let status = std::process::Command::new(&program)
        .args(&args)
        .arg(path)
        .status()
        .map_err(|source| Error::Spawn {
            program: program.clone(),
            source,
        })?;
    if !status.success() {
        return Err(Error::InvalidArguments(format!(
            "editor {program:?} exited with {status}; the policy was not changed"
        )));
    }
    Ok(())
}

fn retry_confirmed() -> bool {
    if !std::io::stdin().is_terminal() {
        return false;
    }
    eprint!("Reopen the editor to fix it? [Y/n] ");
    if std::io::stderr().flush().is_err() {
        return false;
    }
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    )
}

fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Agent(format!("policy path has no parent: {}", path.display())))?;
    std::fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temp.write_all(contents.as_bytes())
        .and_then(|_| temp.flush())
        .map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
    temp.persist(path).map_err(|error| Error::Write {
        path: path.to_path_buf(),
        source: error.error,
    })?;
    Ok(())
}
