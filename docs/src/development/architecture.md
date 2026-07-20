# Arquitetura do código

O entrypoint `src/main.rs` delega a `app`, que separa prompt interno, control plane e servidor MCP.

```text
src/
├── app.rs                  startup e CLI de controle
├── targets.rs              control plane de targets
├── agents/codex.rs         integração Codex e guard compartilhado de shell
├── agents/portable.rs      adapters JSON de Claude, Gemini e Cursor
├── mcp/server.rs           protocolo e tools dinâmicas
├── core/invoke.rs          orquestração de uma chamada
├── target_access.rs         leases temporários de aliases target-aware
├── jasper/                 regras e grants
├── providers/
│   ├── config.rs           schema YAML
│   ├── registry.rs         descoberta de providers/targets
│   ├── packages.rs         manifest, fontes, catálogo e lifecycle de pacotes
│   └── auth/session.rs     lifecycle de sessão
├── runtime/exec.rs         processos filhos
├── control/gui.rs          janelas em subprocesso
├── config/                 paths, settings e env
├── audit.rs                log sanitizado
└── error.rs                erros públicos seguros
```

## Dependências entre camadas

`mcp` chama `core`; `core` chama Jasper, lease de target, sessão, control e runtime. Jasper permanece puro em relação a processo e autenticação. Runtime não decide política. Providers descrevem mecanismo, não operações permitidas. Depois de deny explícito e antes de grants ou sessão, `core` exige lease válido para aliases target-aware; ele o relê antes de ambiente/autenticação e antes do runner. Depois de allow, um target `kubectl_context` pode delegar autenticação ao lifecycle de outro provider não target-aware; o ambiente retornado é composto somente para o processo filho alvo. Um target `aws_profile` usa seu próprio escopo, fixa o profile e confere a conta via STS antes do runner.

## GUI em subprocesso

O servidor usa stdout para MCP, então prompts são abertos por uma nova execução do próprio binário com o subcomando interno `__prompt`. Pedido e resposta usam JSON pelos pipes privados. Durante autenticação, uma thread de background do subprocesso da GUI executa o validator sem bloquear o repaint; somente uma candidata validada retorna ao processo pai para persistência. Stderr do prompt é suprimido.

## Estado compartilhado

`ProviderRegistry` guarda providers e targets em `Arc`, com mutex assíncrono por provider e, quando necessário, por target. Settings, providers e targets são carregados no startup. O campo `provider` de cada target é validado depois que todo o registry é montado. `kubectl_context` herda cache e lock do lifecycle indicado; `aws_profile` os mantém no próprio target. Leases ficam em um arquivo por provider, com digest de binding, revisão, arquivo de lock exclusivo do sistema operacional e escrita atômica; o handle do lock é liberado pelo sistema ao término do processo, sem TTL ou limpeza por timeout de lock stale. Eles são relidos durante chamadas. Regras e grants também são lidos durante chamadas.

## Onde adicionar comportamento

- nova semântica de política: `jasper/`, com testes de token boundary;
- nova estratégia de auth comprovada: `providers/auth/`, preservando transação e lock;
- campo público de provider: `providers/config.rs`, registry, exemplos e docs;
- mudança MCP: `mcp/server.rs` e `reference/mcp-api.md`;
- novo controle humano: `app.rs`/`targets.rs`/`control/`, nunca como tool de agente.
- mudança em integração de agente: `agents/`, `guides/agents.md` e `reference/cli.md`.
