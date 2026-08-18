use std::process::Command;

use tempfile::TempDir;

fn torii() -> Command {
    Command::new(env!("CARGO_BIN_EXE_torii"))
}

const POLICY: &str = "version: \"1.0\"\ndeny:\n  - \"ecs execute-command\"\naccept:\n  - \"ec2 describe-instances\"\n";

fn write_provider(config: &TempDir) {
    let provider = config.path().join("providers").join("aws");
    std::fs::create_dir_all(&provider).unwrap();
    std::fs::write(
        provider.join("provider.yaml"),
        "version: \"1\"\nname: aws\ntool: aws\ndescription: AWS test provider\ncommand: aws\npolicy:\n  minimum_accept_tokens: 2\nauth:\n  strategy: inherited\nenvironment:\n  file: .env\n",
    )
    .unwrap();
    std::fs::write(provider.join("rules.yaml"), POLICY).unwrap();
}

/// Um "editor" que apenas acrescenta uma linha ao arquivo recebido, para exercitar
/// o fluxo sem depender de um editor real. `.bat` no Windows, script `sh` fora dele.
fn fake_editor(home: &TempDir, name: &str, appended: &str) -> String {
    if cfg!(windows) {
        let path = home.path().join(format!("{name}.bat"));
        std::fs::write(&path, format!("@echo off\r\necho {appended}>>%1\r\n")).unwrap();
        return path.to_str().unwrap().to_string();
    }
    let path = home.path().join(name);
    std::fs::write(
        &path,
        format!("#!/bin/sh\nprintf '%s\\n' '{appended}' >> \"$1\"\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path.to_str().unwrap().to_string()
}

fn noop_editor(home: &TempDir) -> String {
    if cfg!(windows) {
        let path = home.path().join("noop.bat");
        std::fs::write(&path, "@echo off\r\nexit /b 0\r\n").unwrap();
        return path.to_str().unwrap().to_string();
    }
    let path = home.path().join("noop.sh");
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path.to_str().unwrap().to_string()
}

fn edit(config: &TempDir, editor: &str, args: &[&str]) -> std::process::Output {
    let mut command = torii();
    command
        .env("TORII_CONFIG_DIR", config.path())
        .env("EDITOR", editor)
        .env_remove("VISUAL")
        .args(["policy", "edit"])
        .args(args);
    command.output().unwrap()
}

fn rules_of(config: &TempDir) -> String {
    std::fs::read_to_string(
        config
            .path()
            .join("providers")
            .join("aws")
            .join("rules.yaml"),
    )
    .unwrap()
}

#[test]
fn a_valid_edit_replaces_the_policy() {
    let config = TempDir::new().unwrap();
    let scripts = TempDir::new().unwrap();
    write_provider(&config);
    let editor = fake_editor(&scripts, "add", "  - \"s3 ls\"");

    let output = edit(&config, &editor, &["aws"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("updated at"), "{stderr}");
    // A contagem confirma que o Torii releu a política que acabou de gravar.
    assert!(stderr.contains("1 deny and 2 accept"), "{stderr}");

    let written = rules_of(&config);
    assert!(written.contains("s3 ls"));
    assert!(
        written.contains("ecs execute-command"),
        "o resto foi preservado"
    );
}

#[test]
fn malformed_yaml_never_reaches_the_live_policy() {
    let config = TempDir::new().unwrap();
    let scripts = TempDir::new().unwrap();
    write_provider(&config);
    let editor = fake_editor(&scripts, "broken", "deny: [ unbalanced");

    let output = edit(&config, &editor, &["aws"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid YAML"), "{stderr}");
    // Uma edição recusada não pode ser perdida.
    assert!(stderr.contains("draft is kept at"), "{stderr}");
    assert_eq!(rules_of(&config), POLICY, "a política viva ficou intacta");
}

#[test]
fn an_invalid_regex_rule_is_refused_before_writing() {
    let config = TempDir::new().unwrap();
    let scripts = TempDir::new().unwrap();
    write_provider(&config);
    let editor = fake_editor(&scripts, "regex", "  - \"/(unclosed/i\"");

    let output = edit(&config, &editor, &["aws"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid rule"), "{stderr}");
    assert_eq!(rules_of(&config), POLICY);
}

#[test]
fn an_untouched_draft_leaves_the_policy_alone() {
    let config = TempDir::new().unwrap();
    let scripts = TempDir::new().unwrap();
    write_provider(&config);
    let editor = noop_editor(&scripts);

    let output = edit(&config, &editor, &["aws"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("was not changed"), "{stderr}");
    assert_eq!(rules_of(&config), POLICY);
}

#[test]
fn an_accept_below_the_minimum_is_reported_as_ignored() {
    let config = TempDir::new().unwrap();
    let scripts = TempDir::new().unwrap();
    write_provider(&config);
    // O provider de teste exige dois tokens; "s3" sozinho é aceito no arquivo mas
    // ignorado na avaliação, e o operador precisa saber disso.
    let editor = fake_editor(&scripts, "short", "  - \"s3\"");

    let output = edit(&config, &editor, &["aws"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("is ignored during evaluation"), "{stderr}");
}

#[test]
fn show_prints_the_active_policy_and_its_path() {
    let config = TempDir::new().unwrap();
    write_provider(&config);

    let output = torii()
        .env("TORII_CONFIG_DIR", config.path())
        .args(["policy", "show", "aws"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rules.yaml"), "{stdout}");
    assert!(stdout.contains("ec2 describe-instances"), "{stdout}");
}

#[test]
fn policy_commands_reject_unknown_scopes() {
    let config = TempDir::new().unwrap();
    let scripts = TempDir::new().unwrap();
    write_provider(&config);
    let editor = noop_editor(&scripts);

    let unknown_tool = edit(&config, &editor, &["kubectl"]);
    assert!(!unknown_tool.status.success());
    assert!(String::from_utf8_lossy(&unknown_tool.stderr).contains("not installed"));

    // aws não é target-aware, então não existe política por target para editar.
    let not_targeted = edit(&config, &editor, &["aws", "dev"]);
    assert!(!not_targeted.status.success());
    let stderr = String::from_utf8_lossy(&not_targeted.stderr);
    assert!(stderr.contains("does not use targets"), "{stderr}");

    let create_without_target = edit(&config, &editor, &["aws", "--create"]);
    assert!(!create_without_target.status.success());
    let stderr = String::from_utf8_lossy(&create_without_target.stderr);
    assert!(stderr.contains("pass the target name"), "{stderr}");
}
