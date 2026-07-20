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

Cada `target.yaml` associa o alias a um context e indica o provider cujo lifecycle será herdado. O registry publica todos os aliases configurados no schema MCP e exige `target` na chamada. O executor injeta `--context <configurado>` antes de `args` e rejeita flags de override enviadas pelo agente.

O alias começa inativo. Antes de grants Jasper, ambiente ou autenticação, o dispatcher exige um lease humano válido, armazenado no escopo do provider e vinculado por digest ao binding atual. A ativação normal substitui os aliases ativos da tool; adicionar outro é uma escolha humana explícita, pois deixa o agente escolher qualquer alias ativo nas operações permitidas.

O provider alvo compartilha executável e política por padrão. Cada target isola grants e pode substituir regras e variáveis persistentes localmente. Em `kubectl_context`, cache, credenciais e lock de autenticação pertencem ao provider indicado pelo campo `provider`.

`aws_profile` é o segundo modo concreto. A tool recebe um alias como `producao`; o alias fixa um profile local e uma conta AWS esperada, que não aparecem no schema MCP. O executor bloqueia opções de troca de profile, região e endpoint, aplica o binding somente ao filho e compara a conta STS antes de executar. Nesse modo, cache e lock pertencem ao próprio target, para que aliases de contas diferentes não compartilhem sessão.

Não há ainda uma linguagem genérica de “identidade remota”. O mecanismo de alias é comum, mas cada binding é implementado e validado pelo modo concreto quando há um provider real que o exige.
