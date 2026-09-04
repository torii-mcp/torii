# Changelog

Todas as mudanças relevantes deste projeto são registradas aqui. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o versionamento é
[semântico](https://semver.org/lang/pt-BR/).

## [0.4.0] — 2026-09-04

### Adicionado

- `--copy-shared` em `torii policy edit <tool> <target> --create`: começa a política do alias com
  uma cópia da compartilhada em vez do template vazio. As duas formas servem casos opostos e
  ambas continuam disponíveis — vazia para um alias que precisa ser mais restrito que a raiz e
  listar explicitamente o que passa, semeada para um alias que precisa ser mais frouxo e só
  acrescentar. O arquivo gerado carrega no cabeçalho o aviso de que a cópia é um retrato: dali em
  diante um accept novo na compartilhada não chega mais naquele alias;
- **modo de target `credentials`.** Um terceiro `targeting.mode` em que o alias é o balde de
  credencial e nada é injetado no argv: serve a CLIs cuja credencial vem do ambiente, como o
  `aws` com `auth.strategy: environment`, em que cada alias coleta as próprias chaves pela
  janela e ganha lease, camada de política, grants isolados e verificação de identidade pelo
  probe do provider. O alias pode autenticar pela própria tool target-aware, arranjo antes
  permitido só no `aws_profile`. `context`, `identity.profile` e `region` são recusados, porque
  seriam no-ops silenciosos. Como o modo não injeta binding, ele também não traz baseline de
  flags bloqueadas: `targeting.locked_options` precisa travar toda opção que redirecione a
  origem da credencial (`--profile` e `--endpoint-url`, no AWS CLI), ou o agente sai do alias
  pelo argv. `torii target add <tool> <name> [--provider <tool>] [--scope <scope>] [--expect
  <identidade>]` cria o alias;
- **decisão permanente na janela de autorização.** No editor de escopo de uma chamada não
  resolvida, a caixa dos argumentos fixos ganha dois botões de manter pressionado por cinco
  segundos: um grava o prefixo escolhido no `accept` da política e executa a chamada, o outro
  grava no `deny` e a nega. A ordem é o ponto — primeiro o ajuste fino da fronteira, depois a
  permanência — e os cinco segundos existem para dar tempo de pensar antes de mexer na política.
  A regra vem do argv normalizado daquele prefixo, cada fronteira é simulada sobre a política
  resultante antes de a janela abrir (um botão que não funcionaria nasce desabilitado com o
  motivo), a escrita é feita pelo servidor com releitura do disco, preservação de comentários e
  substituição atômica, e cada gravação entra na auditoria como `policy-accept-added` ou
  `policy-deny-added`. As novas origens de decisão são `human-permanent-allow` e
  `human-permanent-deny`;

### Alterado

- **A política de um target deixa de substituir a compartilhada por inteiro.** Os dois vetores
  passam a compor de formas diferentes, porque não significam a mesma coisa: `deny` acumula — os
  denies do `rules.yaml` compartilhado valem em todos os targets e não podem ser removidos pela
  política de um alias — e `accept` continua sendo substituído, porque um target existe para ter
  outra superfície de permissão. Antes, criar `targets/<alias>/rules.yaml` apagava o piso de deny
  do provider naquele alias: o alias mais sensível era justamente o que ganhava o direito de sair
  da política da raiz. **Quebra de compatibilidade:** uma política de target que hoje depende de
  omitir um deny compartilhado para liberar algo passa a ser negada; para um veto que valha só em
  alguns aliases, a regra desce para o `deny` de cada um deles;
- `torii policy show <tool> <target>` imprime as duas camadas e o conjunto efetivo, em vez de um
  arquivo só, e `torii_policy` devolve ao agente o efetivo — `deny` com o piso compartilhado
  seguido do target, `accept` somente do target.

## [0.3.0] — 2026-08-18

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

[0.3.0]: https://github.com/torii-mcp/torii/releases/tag/v0.3.0
[0.2.0]: https://github.com/torii-mcp/torii/releases/tag/v0.2.0
[0.1.0]: https://github.com/torii-mcp/torii/releases/tag/v0.1.0
