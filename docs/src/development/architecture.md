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

`mcp` chama `core`; `core` chama Jasper, sessão, control e runtime. Jasper permanece puro em relação a processo e autenticação. Runtime não decide política. Providers descrevem mecanismo, não operações permitidas. Depois de allow, um target pode delegar autenticação ao lifecycle de outro provider não target-aware; o ambiente retornado é composto somente para o processo filho alvo.

## GUI em subprocesso

O servidor usa stdout para MCP, então prompts são abertos por uma nova execução do próprio binário com o subcomando interno `__prompt`. Pedido e resposta usam JSON pelos pipes privados. Durante autenticação, uma thread de background do subprocesso da GUI executa o validator sem bloquear o repaint; somente uma candidata validada retorna ao processo pai para persistência. Stderr do prompt é suprimido.

## Estado compartilhado

`ProviderRegistry` guarda providers e targets em `Arc`, com mutex assíncrono por provider. Settings, providers e targets são carregados no startup. O campo `provider` de cada target é validado depois que todo o registry é montado. Cache e lock continuam pertencendo ao provider cujo lifecycle o target herda; regras e grants são lidos durante chamadas.

## Onde adicionar comportamento

- nova semântica de política: `jasper/`, com testes de token boundary;
- nova estratégia de auth comprovada: `providers/auth/`, preservando transação e lock;
- campo público de provider: `providers/config.rs`, registry, exemplos e docs;
- mudança MCP: `mcp/server.rs` e `reference/mcp-api.md`;
- novo controle humano: `app.rs`/`targets.rs`/`control/`, nunca como tool de agente.
- mudança em integração de agente: `agents/`, `guides/agents.md` e `reference/cli.md`.
