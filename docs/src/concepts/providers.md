# Providers e targets

Um provider é um diretório local carregado no startup. Ele transforma configuração em uma tool MCP sem virar plugin dinâmico, WASM, pacote OCI ou servidor separado.

## Responsabilidades

Um provider declara nome e tool, executável, argumentos prefixados, política, autenticação, ambiente e, opcionalmente, um modo de targeting.

Torii percorre `providers/` e carrega diretórios com `provider.yaml`. Nomes lógicos e tools devem ser únicos. Um provider inválido impede o startup. Nomes de tool e aliases aceitam letras ASCII, dígitos, `_`, `-` e `.`, com no máximo 128 bytes.

## Pacotes oficiais

Um pacote é um conjunto declarativo com provider, rules vazio, ambiente inicial e setups read-only opcionais. Pacotes oficiais vivem no catálogo separado [`torii-mcp/torii-canon-providers`](https://github.com/torii-mcp/torii-canon-providers); `examples/providers/` contém fixtures equivalentes para desenvolvimento.

`provider install` materializa o pacote atomicamente. `provider setup <provider> <perfil>` é uma operação posterior e explícita sobre a política vazia. O agente não acessa nenhuma dessas capacidades.

## Provider simples

AWS usa o provider diretamente. Política, grants, sessão e autenticação vivem na raiz do provider, e a chamada MCP contém apenas `args`.

`args_prefix` fixa argumentos confiáveis antes dos argumentos do agente. A política continua avaliando somente `args`.

## Provider target-aware

Kubernetes usa uma tool única `kubectl` e vários aliases:

```text
providers/kubectl/targets/
├── lab/target.yaml
└── lab_alt/target.yaml
```

Cada `target.yaml` associa o alias a um context e indica o provider cujo lifecycle será herdado. O registry publica os aliases no schema MCP e exige `target` na chamada. O executor injeta `--context <configurado>` antes de `args` e rejeita flags de override enviadas pelo agente.

O provider alvo compartilha executável e política por padrão. Cada target isola grants e pode substituir regras e variáveis persistentes localmente. Cache, credenciais e lock de autenticação pertencem ao provider indicado pelo campo `provider`.
