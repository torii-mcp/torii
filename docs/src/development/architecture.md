# Arquitetura do código

O entrypoint `src/main.rs` delega a `app`, que separa prompt interno, control plane e servidor MCP.

```text
src/
├── app.rs                  startup e CLI de controle
├── targets.rs              control plane de targets
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

`mcp` chama `core`; `core` chama Jasper, sessão, control e runtime. Jasper permanece puro em relação a processo e autenticação. Runtime não decide política. Providers descrevem mecanismo, não operações permitidas.

## GUI em subprocesso

O servidor usa stdout para MCP, então prompts são abertos por uma nova execução do próprio binário com o subcomando interno `__prompt`. Pedido e resposta usam JSON pelos pipes privados. Credenciais retornam ao pai apenas para validação e persistência; stderr do prompt é suprimido.

## Estado compartilhado

`ProviderRegistry` guarda providers e targets em `Arc`, com mutex assíncrono por escopo. Settings, providers e targets são carregados no startup. Regras e grants são lidos durante chamadas.

## Onde adicionar comportamento

- nova semântica de política: `jasper/`, com testes de token boundary;
- nova estratégia de auth comprovada: `providers/auth/`, preservando transação e lock;
- campo público de provider: `providers/config.rs`, registry, exemplos e docs;
- mudança MCP: `mcp/server.rs` e `reference/mcp-api.md`;
- novo controle humano: `app.rs`/`targets.rs`/`control/`, nunca como tool de agente.
