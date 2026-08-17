# Changelog

Todas as mudanças relevantes deste projeto são registradas aqui. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o versionamento é
[semântico](https://semver.org/lang/pt-BR/).

## [0.2.0] — 2026-08-17

### Adicionado

- integrações de agente gerenciadas: `torii agent install|status|uninstall` para Codex,
  Claude Code, Gemini CLI e Cursor, com hook opcional que bloqueia chamadas diretas aos
  executáveis dos providers instalados;
- grants temporários com escopo, aprovados na janela humana e isolados por provider/target;
- targets escopados por provider de identidade: `target add --provider`, `--scope` e
  `--expect`, com balde de credencial por alias;
- modo `aws_profile`: aliases humanos que fixam profile, conta esperada e região, com
  conferência STS da conta antes da execução;
- leases humanos temporários por alias target-aware (`target activate`, `target status`,
  `target clear`), verificados depois do deny explícito e antes de grants, ambiente ou
  autenticação;
- autorização por pressionar-e-segurar na janela, com presets de duração alinhados ao
  grant vivo mais antigo;
- regras de política por regex (`/…/flags`) avaliadas sobre o argv inteiro, `forbidden_args`
  e normalização de argv (`ignore_args`) restrita à avaliação da política;
- pacotes de exemplo para Azure CLI (`az`) e Snowflake CLI (`snow`);
- capítulo de referência rápida na documentação oficial;
- workflows de CI (`fmt`, `check`, `test`, `clippy`, build do livro) e publicação da
  documentação no GitHub Pages.

### Alterado

- `policy.grant_rule` passa a ser aceito apenas por compatibilidade e não influencia
  grants novos;
- projeto relicenciado sob AGPL-3.0-only.

### Corrigido

- `torii --version` e `torii -V` imprimem a versão do binário.

### Removido

- `awsgate-para-torii.md`, especificação interna da transformação do AWS Gate; o contrato
  vigente vive em `docs/`.

## [0.1.0] — 2026-07-14

### Adicionado

- servidor MCP local por stdio, com uma tool dinâmica por provider instalado;
- Jasper com default deny, prioridade do deny explícito e matching por prefixo de tokens;
- registry declarativo de providers, pacotes locais/archive/URL e catálogo canônico
  pesquisável, com verificação SHA-256;
- targets Kubernetes que resolvem aliases para contexts sem override pelo agente;
- autenticação `environment` e `inherited`, janela local de aprovação e coleta,
  validação antes da substituição atômica da sessão;
- execução sem shell, captura de stdout/stderr/exit code com limite explícito;
- auditoria local sanitizada;
- migração não destrutiva de `~/.config/.awsgate` para o provider AWS;
- documentação oficial em mdBook.

[0.2.0]: https://github.com/torii-mcp/torii/releases/tag/v0.2.0
[0.1.0]: https://github.com/torii-mcp/torii/releases/tag/v0.1.0
