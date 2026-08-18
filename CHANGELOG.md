# Changelog

Todas as mudanças relevantes deste projeto são registradas aqui. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o versionamento é
[semântico](https://semver.org/lang/pt-BR/).

## Não lançado

### Adicionado

- adapters de agente para Antigravity, opencode, GitHub Copilot no VS Code, GitHub Copilot CLI
  e pi, cada um escrito no formato nativo do cliente (`mcp` com comando em vetor no opencode,
  `servers` no Copilot do VS Code, `mcpServers` com `tools` na CLI do Copilot);
- guard do Antigravity pelo evento `PreToolUse` com matcher em `run_command`, lendo a linha em
  `toolCall.args.CommandLine` e negando com `{"decision":"deny"}`; o Torii mantém um grupo de
  hooks próprio em `~/.gemini/config/hooks.json` e o remove inteiro na desinstalação;
- `--yes` em `agent install` para confirmar o adapter do pi, cujo suporte a MCP depende de uma
  extensão instalada pelo humano; sem terminal interativo a instalação é recusada;
- `torii policy show` e `torii policy edit` no control plane humano: a edição abre uma cópia
  no `$VISUAL`/`$EDITOR`, parseia e compila cada regra antes de substituir o arquivo vivo de
  forma atômica, preserva o rascunho recusado e avisa sobre accepts abaixo de
  `minimum_accept_tokens`; `--create` inicia a política de um target;
- `install.sh` e `install.ps1`: instaladores por `curl | sh` e `irm | iex` que resolvem a última
  release, conferem o SHA-256 publicado antes de extrair, instalam num diretório do usuário sem
  exigir administrador e tratam o PATH — no Linux só com `--add-to-path`, no Windows por padrão;
- `torii self upgrade [--check]`: baixa a release da plataforma, confere o checksum e substitui o
  binário em execução, preservando configuração, políticas, targets, grants e credenciais;
- guias de provider para Azure (`az`) e Snowflake (`snow`).

### Alterado

- **`provider update` passa a ser `provider upgrade`.** O Torii não mantém índice local para
  sincronizar, então trocar a versão instalada chama-se upgrade em todo o produto, e `update`
  deixa de existir. Quem digitar o nome antigo recebe uma mensagem apontando o novo;
- `agent install --hook` é recusado antes de qualquer escrita em clientes sem hook de
  pré-execução, e `agent status` informa `hook not supported` para eles;
- `agent install opencode` recusa editar uma configuração em `opencode.jsonc` em vez de
  reescrever o arquivo e descartar comentários.

### Corrigido

- larguras de traço da janela anotadas como `f32`: o literal ambíguo caía para `f64` e o Rust
  estável atual avisa sobre essa inferência, quebrando a verificação com `-D warnings`;
- checksum do pacote Windows escrito com LF, para `sha256sum -c` funcionar no destino.

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
