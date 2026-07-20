# AGENTS.md

Este arquivo orienta agentes de código que trabalham no Torii. Ele vale para todo o repositório; arquivos `AGENTS.md` mais específicos podem complementar estas regras em subdiretórios.

## Missão do projeto

Torii é uma interface MCP local de execução controlada para agentes:

```text
agente -> MCP stdio -> Torii -> Jasper -> provider -> executável real
```

O Torii não é um CLI multicloud e não substitui `aws`, `kubectl`, `az` ou `gcloud`. Humanos usam as ferramentas humanas diretamente; agentes atravessam o Torii.

Antes de alterar comportamento, leia:

1. `awsgate-para-torii.md`, a especificação de transformação;
2. `docs/src/concepts/security-model.md`, os invariantes de segurança;
3. `docs/src/development/architecture.md`, o mapa do código;
4. os testes relacionados à área alterada.

O repositório irmão `../awsgate` é somente referência. Nunca edite, formate, mova, apague ou gere arquivos nele.

## Invariantes que não podem regredir

1. Default deny: o que não foi aceito, concedido temporariamente ou aprovado é negado.
2. Deny explícito sempre vence accept, grant e aprovação humana.
3. Matching é por prefixo de tokens, nunca por `String::starts_with` na linha reconstruída.
4. O MCP recebe `args: string[]` e, em providers target-aware, `target`; preserve `args` como vetor até `Command::args` e resolva o target somente por configuração humana.
5. Nunca use shell (`bash -c`, `sh -c`, `cmd /c`, PowerShell para encaminhar providers).
6. Política é avaliada antes de ler `.env`, credenciais ou cache de sessão.
7. Credenciais são aplicadas somente ao processo filho; nunca use `std::env::set_var` para sessão.
8. Não registre credenciais, clipboard, stdout/stderr completos ou a lista completa de argumentos.
9. Reautenticação só substitui a sessão anterior após validar a candidata.
10. Uma única renovação por provider simples ou target pode acontecer por vez.
11. Stdout do processo servidor pertence exclusivamente ao transporte MCP. Diagnósticos vão para stderr ou auditoria.
12. Uma tool por provider, nunca uma tool por operação.
13. O agente não recebe tools de `kill`, `reauth`, instalação ou edição de política.
14. Targets começam inativos; alias anunciado não concede acesso. O lease humano temporário é verificado depois do deny explícito e antes de grants, ambiente ou autenticação.
15. Lease de target é vinculado ao binding, relido do disco e atualizado com revisão/lock; uma aprovação aberta antes de `target clear` não pode restaurá-lo.
16. Não crie uma abstração genérica antes de dois providers reais demonstrarem a necessidade.

## Mapa do código

- `src/mcp/`: transporte stdio, descoberta dinâmica e dispatch de tools.
- `src/core/`: orquestra decisão, sessão, execução e resposta.
- `src/jasper/`: regras e grants; não executa processos nem lê credenciais.
- `src/providers/`: schema, registry e lifecycle de autenticação.
- `src/runtime/`: execução sem shell e limite de saída.
- `src/control/`: interface humana de aprovação e autenticação.
- `src/config/`: paths, settings e arquivos de ambiente.
- `src/audit.rs`: eventos sanitizados e best-effort.
- `examples/`: fixtures de pacotes canônicos usadas em testes e no desenvolvimento do catálogo separado.
- `docs/`: documentação oficial em mdBook.

## Fluxo de trabalho

- Preserve alterações existentes do usuário e mantenha o escopo da tarefa.
- Use `rg`/`rg --files` para busca.
- Edite arquivos manualmente com patches; use formatadores para mudanças mecânicas.
- Não edite artefatos em `target/` ou `docs/book/`.
- Mudou configuração pública, resposta MCP, CLI ou invariantes? Atualize o livro na mesma mudança.
- Mudou um pacote em `examples/providers/`? Valide instalação, setup e update local.
- Nunca coloque segredos reais em fixtures, exemplos, snapshots, logs ou mensagens de erro.

## Verificação obrigatória

Execute, nesta ordem:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
mdbook build docs
```

No Windows com toolchain GNU, testes que linkam `eframe` precisam de MinGW-w64 completo (`dlltool.exe` e `as.exe`) no `PATH`. A máquina de referência usa:

```text
%LOCALAPPDATA%\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.MSVCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin
```

Em CI/headless, defina `TORII_NO_GUI=1`. Isso deve negar chamadas não resolvidas e cancelar coleta de autenticação com segurança.

## Critério de conclusão

Uma mudança está pronta quando o comportamento pedido está implementado, os invariantes continuam cobertos por testes, a documentação pública corresponde ao código e todas as verificações relevantes passam. Não declare suporte a uma estratégia ou provider apenas porque o schema aceita seu nome.
