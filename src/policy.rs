//! Edição humana das políticas Jasper. Este módulo pertence ao control plane e
//! nunca é exposto como tool MCP: o agente não edita a política que o limita.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::ConfigPaths;
use crate::error::{Error, Result};
use crate::jasper::rules::{self, Evaluation, Rules};
use crate::providers::{Provider, ProviderRegistry};

/// Política inicial de um target, criada apenas com `--create`.
const TARGET_TEMPLATE: &str = "version: \"1.0\"\n\n\
     # Os denies da política compartilhada do provider continuam valendo neste\n\
     # target e não podem ser removidos aqui; os denies abaixo somam a eles.\n\
     deny: []\n\n\
     # Os accepts abaixo SUBSTITUEM os accepts compartilhados neste alias: o que\n\
     # não estiver relistado aqui volta a ser negado por padrão.\n\
     accept: []\n";

pub fn show(paths: &ConfigPaths, tool: &str, target: Option<&str>) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = resolve_provider(&registry, tool)?;
    let minimum = provider.config.policy.minimum_accept_tokens;
    let (path, scope) = rules_path(&provider, tool, target)?;

    let Some(target_name) = target else {
        if !path.exists() {
            return Err(Error::RulesNotFound(path));
        }
        let (parsed, text) = read_policy(&path)?;
        print_file(&path, &text);
        warn_ignored_accepts(&parsed, minimum);
        return Ok(());
    };

    // Um target lê como duas camadas. Mostrar só uma delas esconderia metade do
    // que decide a chamada: o piso de deny vem da compartilhada mesmo quando o
    // target tem política própria.
    let shared_path = provider.paths.rules();
    if !shared_path.exists() {
        return Err(Error::RulesNotFound(shared_path));
    }
    let (shared, shared_text) = read_policy(&shared_path)?;
    print_file(&shared_path, &shared_text);

    if !path.exists() {
        eprintln!(
            "{scope} has no policy of its own, so the shared provider policy above applies whole. Run `torii policy edit {tool} {target_name} --create` to start one."
        );
        warn_ignored_accepts(&shared, minimum);
        return Ok(());
    }

    let (own, own_text) = read_policy(&path)?;
    println!();
    print_file(&path, &own_text);
    let effective = Rules::layered(&shared, &own);
    effective.compile()?;
    println!();
    print!("{}", effective_report(&scope, &effective));
    warn_ignored_accepts(&effective, minimum);
    Ok(())
}

/// Lê, parseia e compila uma política antes de exibi-la: um regex inválido em
/// disco é um erro para o humano ver, não uma linha impressa como se valesse.
fn read_policy(path: &Path) -> Result<(Rules, String)> {
    let text = read_text(path)?;
    let parsed: Rules = parse(&text, path)?;
    parsed.compile()?;
    Ok((parsed, text))
}

fn print_file(path: &Path, text: &str) {
    println!("# {}", path.display());
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
}

/// O conjunto que a avaliação realmente usa, depois de compor as duas camadas.
fn effective_report(scope: &str, effective: &Rules) -> String {
    let mut report = format!("# effective {scope}\n");
    report
        .push_str("# deny: the shared floor plus this target's own; accept: this target's only\n");
    report.push_str(&format!("version: {}\n", yaml_scalar(&effective.version)));
    push_sequence(&mut report, "deny", &effective.deny);
    push_sequence(&mut report, "accept", &effective.accept);
    report
}

fn push_sequence(report: &mut String, name: &str, rules: &[String]) {
    if rules.is_empty() {
        report.push_str(&format!("{name}: []\n"));
        return;
    }
    report.push_str(&format!("{name}:\n"));
    for rule in rules {
        report.push_str(&format!("  - {}\n", yaml_scalar(rule)));
    }
}

/// Cita o escalar como o próprio YAML citaria. Um valor que não caiba em uma
/// linha cai para a forma entre aspas do Rust, que nunca quebra a linha.
fn yaml_scalar(value: &str) -> String {
    match serde_yaml::to_string(value) {
        Ok(encoded) => {
            let trimmed = encoded.trim_end_matches('\n');
            if trimmed.contains('\n') {
                format!("{value:?}")
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => format!("{value:?}"),
    }
}

/// Qual vetor da política recebe a regra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Accept,
    Deny,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::Accept => "accept",
            Section::Deny => "deny",
        }
    }

    fn vector(self, rules: &Rules) -> &Vec<String> {
        match self {
            Section::Accept => &rules.accept,
            Section::Deny => &rules.deny,
        }
    }
}

/// A regra literal que expressa este argv normalizado, ou o motivo de ela não ser
/// representável.
///
/// A avaliação tokeniza regras literais por whitespace, então um argumento que
/// contenha espaço não tem como ser escrito como regra: ele viraria dois tokens e
/// a regra jamais casaria com o argv de um token só. Uma regra assim é pior que
/// nenhuma — um accept que não permite ou, muito pior, um deny que não protege.
pub fn literal_rule(normalized: &[String]) -> std::result::Result<String, String> {
    if normalized.is_empty() {
        return Err(
            "a normalização de argumentos do provider descarta todos os argumentos desta chamada"
                .into(),
        );
    }
    for argument in normalized {
        if argument.is_empty() {
            return Err("um dos argumentos é vazio e não pode virar token de uma regra".into());
        }
        if !argument
            .split_whitespace()
            .eq(std::iter::once(argument.as_str()))
        {
            return Err(format!(
                "o argumento {:?} contém espaço e não cabe numa regra literal; use `torii policy edit`",
                argument.chars().take(24).collect::<String>()
            ));
        }
    }
    let rule = normalized.join(" ");
    if rules::is_regex_rule(&rule) {
        return Err(
            "os argumentos formariam uma regra com a forma de regex (`/…/`) e seriam avaliados como tal"
                .into(),
        );
    }
    Ok(rule)
}

/// A trava central de uma decisão permanente: a regra só vale se a política
/// resultante, compilada e avaliada sobre este mesmo argv normalizado, produzir o
/// veredicto pretendido. Normalização, largura mínima de accept, precedência de
/// deny e qualquer interação que não previmos aparecem aqui — antes do disco.
pub fn simulate(
    current: &Rules,
    section: Section,
    rule: &str,
    normalized: &[String],
    minimum: usize,
) -> Result<()> {
    let mut candidate = current.clone();
    match section {
        Section::Accept => candidate.accept.push(rule.to_string()),
        Section::Deny => candidate.deny.push(rule.to_string()),
    }
    let evaluation = candidate.compile()?.evaluate(normalized, minimum);
    let lands = match (section, &evaluation) {
        (Section::Accept, Evaluation::Allowed { rule: matched }) => matched == rule,
        (Section::Deny, Evaluation::DeniedExplicit { rule: matched }) => matched == rule,
        _ => false,
    };
    if lands {
        return Ok(());
    }
    Err(Error::InvalidArguments(match (section, &evaluation) {
        (Section::Accept, Evaluation::Unresolved) => format!(
            "a regra de accept teria menos de {minimum} token(s) e seria ignorada na avaliação"
        ),
        (Section::Accept, Evaluation::DeniedExplicit { rule }) => {
            format!("um deny explícito ({rule:?}) prevalece sobre qualquer accept desta invocação")
        }
        _ => format!("a regra {rule:?} não produziria o efeito pretendido nesta política"),
    }))
}

/// Grava uma decisão permanente tomada por gesto humano na janela de autorização.
///
/// A política é relida do disco agora, e não reaproveitada do início da chamada,
/// para não sobrescrever uma edição concorrente. O texto reescrito é parseado,
/// compilado e simulado antes de substituir o arquivo: inserção textual nunca é
/// confiada sem reler o resultado.
pub fn add_rule(
    path: &Path,
    section: Section,
    rule: &str,
    normalized: &[String],
    minimum: usize,
) -> Result<()> {
    let text = read_text(path)?;
    let current: Rules = parse(&text, path)?;
    current.compile()?;
    if section
        .vector(&current)
        .iter()
        .any(|existing| existing == rule)
    {
        // Já está na política: nada a acrescentar e nada a duplicar.
        return Ok(());
    }
    let updated = insert(&text, section, rule)?;
    let parsed: Rules = parse(&updated, path)?;
    parsed.compile()?;
    simulate(&parsed, section, rule, normalized, minimum)?;
    write_atomic(path, &updated)
}

/// Insere a regra no fim da sequência do vetor pedido, preservando comentários,
/// indentação e o resto do arquivo.
///
/// Entende só a forma simples: mapa no topo e o vetor como sequência em bloco ou
/// `[]` vazio. Qualquer outra forma é recusada — desfigurar um arquivo que o
/// humano mantém à mão é pior que não ter o botão.
fn insert(text: &str, section: Section, rule: &str) -> Result<String> {
    let key = section.key();
    let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let scalar = yaml_scalar(rule);
    let lines = text.split_inclusive('\n').collect::<Vec<_>>();

    let mut header = None;
    for (index, line) in lines.iter().enumerate() {
        let content = line.trim_end_matches(['\n', '\r']);
        // `strip_prefix` já exige coluna zero: linha indentada não é chave de topo.
        let Some(value) = content
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(':'))
        else {
            continue;
        };
        if header.is_some() {
            return Err(Error::InvalidArguments(format!(
                "a política tem mais de uma chave {key:?} no topo; ajuste-a com `torii policy edit`"
            )));
        }
        header = Some((index, value));
    }
    let Some((header_index, value)) = header else {
        return Err(Error::InvalidArguments(format!(
            "a política não tem a chave {key:?} no topo; ajuste-a com `torii policy edit`"
        )));
    };

    let (value, comment) = match value.split_once('#') {
        Some((before, after)) => (before.trim(), format!(" #{after}")),
        None => (value.trim(), String::new()),
    };

    if value.is_empty() {
        // Sequência em bloco: a regra entra depois do último item existente.
        let mut insert_at = header_index + 1;
        let mut indent = "  ";
        for line in lines.iter().skip(header_index + 1) {
            let content = line.trim_end_matches(['\n', '\r']);
            let trimmed = content.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                insert_at += 1;
                continue;
            }
            if !content.starts_with([' ', '\t']) {
                break; // próxima chave de topo
            }
            if !trimmed.starts_with('-') {
                return Err(Error::InvalidArguments(format!(
                    "o bloco {key:?} tem uma forma que este botão não entende; ajuste-a com `torii policy edit`"
                )));
            }
            indent = &content[..content.len() - trimmed.len()];
            insert_at += 1;
        }
        // Comentário ou linha vazia depois do último item não é item: a regra
        // entra antes deles, e não no meio do rodapé do bloco.
        while insert_at > header_index + 1 {
            let previous = lines[insert_at - 1]
                .trim_end_matches(['\n', '\r'])
                .trim_start();
            if previous.is_empty() || previous.starts_with('#') {
                insert_at -= 1;
            } else {
                break;
            }
        }
        let mut updated = lines[..insert_at].concat();
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push_str(ending);
        }
        updated.push_str(&format!("{indent}- {scalar}{ending}"));
        updated.push_str(&lines[insert_at..].concat());
        return Ok(updated);
    }

    if value
        .chars()
        .filter(|c| !c.is_whitespace())
        .eq("[]".chars())
    {
        let mut updated = lines[..header_index].concat();
        updated.push_str(&format!("{key}:{comment}{ending}  - {scalar}{ending}"));
        updated.push_str(&lines[header_index + 1..].concat());
        return Ok(updated);
    }

    Err(Error::InvalidArguments(format!(
        "o vetor {key:?} está escrito em linha (`{key}: [...]`); ajuste-o com `torii policy edit`"
    )))
}

/// O que `policy edit` faz quando a política do target ainda não existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateMode {
    /// Não cria: um target sem política própria usa a compartilhada inteira.
    No,
    /// Cria vazia, para um alias que precisa ser mais restrito que a raiz e
    /// listar explicitamente o que passa.
    Empty,
    /// Cria com uma cópia da política compartilhada, para um alias que precisa
    /// ser mais frouxo: abre já com tudo dentro e você acrescenta.
    CopyShared,
}

pub fn edit(
    paths: &ConfigPaths,
    tool: &str,
    target: Option<&str>,
    create: CreateMode,
) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = resolve_provider(&registry, tool)?;
    let (path, scope) = rules_path(&provider, tool, target)?;
    let minimum = provider.config.policy.minimum_accept_tokens;

    let original = match (path.exists(), target) {
        (true, _) => read_text(&path)?,
        // Criar rules de target é uma decisão semântica: os accepts compartilhados
        // param de valer naquele alias, então exigem um pedido explícito.
        (false, Some(_)) if create == CreateMode::CopyShared => {
            seeded_from_shared(&provider, tool)?
        }
        (false, Some(_)) if create == CreateMode::Empty => TARGET_TEMPLATE.to_string(),
        (false, Some(_)) => {
            return Err(Error::InvalidArguments(format!(
                "{scope} has no rules of its own and uses the shared provider policy whole; pass --create to start a target policy, which adds denies to the shared ones and replaces the shared accepts"
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

/// A política compartilhada, verbatim, com um cabeçalho dizendo o que ela é.
///
/// A cópia é um retrato, não um vínculo: dali em diante as duas seguem separadas
/// e um accept novo na compartilhada não chega mais neste alias. Isso vai escrito
/// dentro do arquivo, onde quem for editar daqui a seis meses vai ler.
fn seeded_from_shared(provider: &Provider, tool: &str) -> Result<String> {
    let shared_path = provider.paths.rules();
    if !shared_path.exists() {
        return Err(Error::RulesNotFound(shared_path));
    }
    let (_, shared) = read_policy(&shared_path)?;
    let mut seeded = format!(
        "# Cópia da política compartilhada de {tool:?}, feita ao criar a política\n\
         # deste alias. A partir daqui as duas seguem separadas: um accept novo na\n\
         # compartilhada NÃO chega mais aqui, e precisa ser repetido neste arquivo.\n\
         #\n\
         # Os denies da compartilhada continuam valendo neste alias mesmo que você\n\
         # os apague daqui: eles são o piso do provider. Os accepts abaixo são os\n\
         # únicos que valem neste alias.\n\n"
    );
    seeded.push_str(&shared);
    if !seeded.ends_with('\n') {
        seeded.push('\n');
    }
    Ok(seeded)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(version: &str, deny: &[&str], accept: &[&str]) -> Rules {
        Rules {
            version: version.into(),
            deny: deny.iter().map(|rule| (*rule).to_string()).collect(),
            accept: accept.iter().map(|rule| (*rule).to_string()).collect(),
        }
    }

    #[test]
    fn the_effective_report_shows_both_layers_composed() {
        let effective = Rules::layered(
            &rules("1.0", &["danger"], &["get pods"]),
            &rules("1.0", &["iam"], &["get services"]),
        );
        let report = effective_report("policy for \"kubectl\" target \"dev\"", &effective);
        assert_eq!(
            report,
            concat!(
                "# effective policy for \"kubectl\" target \"dev\"\n",
                "# deny: the shared floor plus this target's own; accept: this target's only\n",
                "version: '1.0'\n",
                "deny:\n",
                "  - danger\n",
                "  - iam\n",
                "accept:\n",
                "  - get services\n",
            )
        );
    }

    #[test]
    fn an_empty_vector_reports_as_an_empty_sequence() {
        let report = effective_report("scope", &rules("1.0", &[], &[]));
        assert!(report.ends_with("deny: []\naccept: []\n"), "{report}");
    }

    #[test]
    fn a_regex_rule_survives_the_report_verbatim() {
        // Uma regra regex tem barras e escapes; o relatório precisa devolvê-la
        // exatamente como está no arquivo, ou o humano revisa outra coisa.
        let rule = r"/\btruncate\b/i";
        let report = effective_report("scope", &rules("1.0", &[rule], &[]));
        assert!(
            report.contains(&format!("  - {}\n", yaml_scalar(rule))),
            "{report}"
        );
        let quoted = yaml_scalar(rule);
        assert_eq!(serde_yaml::from_str::<String>(&quoted).unwrap(), rule);
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn a_literal_rule_is_the_normalized_argv() {
        assert_eq!(
            literal_rule(&args(&["s3", "ls", "meu-bucket"])).unwrap(),
            "s3 ls meu-bucket"
        );
    }

    #[test]
    fn an_argument_with_a_space_has_no_literal_rule() {
        // A regra "sql -q select 1" tokenizaria em quatro; o argv tem três. Ela
        // nunca casaria, e um deny que não casa é pior que nenhum deny.
        let error = literal_rule(&args(&["sql", "-q", "select 1"])).unwrap_err();
        assert!(error.contains("espaço"), "{error}");
    }

    #[test]
    fn an_argv_that_normalization_empties_has_no_literal_rule() {
        assert!(literal_rule(&[]).is_err());
        assert!(literal_rule(&args(&["ls", ""])).is_err());
    }

    #[test]
    fn an_argv_shaped_like_a_regex_has_no_literal_rule() {
        // Gravada como está, esta regra seria compilada como regex em vez de
        // literal, e casaria em qualquer posição do argv.
        assert!(literal_rule(&args(&["/etc/i"])).is_err());
    }

    #[test]
    fn an_accept_below_the_minimum_width_is_refused_before_the_disk() {
        let current = rules("1.0", &[], &[]);
        let error = simulate(&current, Section::Accept, "s3", &args(&["s3"]), 2).unwrap_err();
        assert!(error.to_string().contains("2 token"), "{error}");
        // O mesmo argv como deny passa: deny não tem largura mínima.
        simulate(&current, Section::Deny, "s3", &args(&["s3"]), 2).unwrap();
    }

    #[test]
    fn an_accept_under_an_existing_deny_is_refused() {
        let current = rules("1.0", &["s3 rb"], &[]);
        let error = simulate(
            &current,
            Section::Accept,
            "s3 rb",
            &args(&["s3", "rb", "b"]),
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("deny"), "{error}");
    }

    #[test]
    fn insertion_appends_to_a_block_sequence_and_keeps_the_comments() {
        let text = concat!(
            "version: \"1.0\"\n",
            "\n",
            "# denies primeiro\n",
            "deny:\n",
            "  - s3 rb\n",
            "\n",
            "accept:\n",
            "    - s3 ls        # indentação de quatro\n",
            "# rodapé do bloco\n",
        );
        let updated = insert(text, Section::Accept, "s3 cp").unwrap();
        assert_eq!(
            updated,
            concat!(
                "version: \"1.0\"\n",
                "\n",
                "# denies primeiro\n",
                "deny:\n",
                "  - s3 rb\n",
                "\n",
                "accept:\n",
                "    - s3 ls        # indentação de quatro\n",
                "    - s3 cp\n",
                "# rodapé do bloco\n",
            )
        );
    }

    #[test]
    fn insertion_turns_an_empty_flow_vector_into_a_block_and_keeps_its_comment() {
        let text = "version: \"1.0\"\ndeny: []  # nada bloqueado ainda\naccept: []\n";
        let updated = insert(text, Section::Deny, "iam").unwrap();
        assert_eq!(
            updated,
            "version: \"1.0\"\ndeny: # nada bloqueado ainda\n  - iam\naccept: []\n"
        );
    }

    #[test]
    fn insertion_preserves_crlf_line_endings() {
        let text = "version: \"1.0\"\r\ndeny: []\r\naccept:\r\n  - s3 ls\r\n";
        let updated = insert(text, Section::Accept, "s3 cp").unwrap();
        assert_eq!(
            updated,
            "version: \"1.0\"\r\ndeny: []\r\naccept:\r\n  - s3 ls\r\n  - s3 cp\r\n"
        );
    }

    #[test]
    fn insertion_refuses_a_shape_it_does_not_understand() {
        // Vetor em linha, chave ausente e chave repetida: recusa explícita em vez
        // de reescrever um arquivo que o humano mantém à mão.
        assert!(insert("accept: [s3 ls]\ndeny: []\n", Section::Accept, "s3 cp").is_err());
        assert!(insert("deny: []\n", Section::Accept, "s3 cp").is_err());
        assert!(insert("accept: []\naccept: []\n", Section::Accept, "s3 cp").is_err());
        assert!(insert("accept:\n  s3: ls\ndeny: []\n", Section::Accept, "s3 cp").is_err());
    }

    #[test]
    fn insertion_does_not_confuse_a_nested_key_with_the_top_level_one() {
        let text = "version: \"1.0\"\nmetadata:\n  accept: nada\naccept:\n  - s3 ls\ndeny: []\n";
        let updated = insert(text, Section::Accept, "s3 cp").unwrap();
        assert_eq!(
            updated,
            "version: \"1.0\"\nmetadata:\n  accept: nada\naccept:\n  - s3 ls\n  - s3 cp\ndeny: []\n"
        );
    }

    fn policy_file(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("rules.yaml");
        std::fs::write(&path, contents).unwrap();
        (temp, path)
    }

    #[test]
    fn adding_a_rule_writes_it_and_keeps_the_file_readable() {
        let (_temp, path) = policy_file("version: \"1.0\"\ndeny: []\naccept: []\n");
        add_rule(&path, Section::Accept, "s3 ls", &args(&["s3", "ls"]), 2).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        let parsed: Rules = serde_yaml::from_str(&written).unwrap();
        assert_eq!(parsed.accept, ["s3 ls"]);
        assert!(matches!(
            parsed
                .compile()
                .unwrap()
                .evaluate(&args(&["s3", "ls", "bucket"]), 2),
            Evaluation::Allowed { .. }
        ));
    }

    #[test]
    fn adding_a_rule_twice_does_not_duplicate_it() {
        let (_temp, path) = policy_file("version: \"1.0\"\ndeny: []\naccept: []\n");
        let normalized = args(&["s3", "ls"]);
        add_rule(&path, Section::Accept, "s3 ls", &normalized, 2).unwrap();
        add_rule(&path, Section::Accept, "s3 ls", &normalized, 2).unwrap();
        let parsed: Rules = serde_yaml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.accept, ["s3 ls"]);
    }

    #[test]
    fn a_rule_that_would_not_apply_never_reaches_the_file() {
        // Accept de um token num provider que exige dois: seria ignorado na
        // avaliação. A simulação recusa e o arquivo fica intacto.
        let original = "version: \"1.0\"\ndeny: []\naccept: []\n";
        let (_temp, path) = policy_file(original);
        let error = add_rule(&path, Section::Accept, "s3", &args(&["s3"]), 2).unwrap_err();
        assert!(error.to_string().contains("2 token"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn a_malformed_policy_is_not_overwritten() {
        let original = "deliberadamente: [inválido\n";
        let (_temp, path) = policy_file(original);
        assert!(add_rule(&path, Section::Accept, "s3 ls", &args(&["s3", "ls"]), 2).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
}
