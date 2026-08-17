# Contribuindo com o Torii

Torii é uma fronteira de segurança. Uma contribuição é avaliada primeiro pelos invariantes
que preserva e só depois pela funcionalidade que adiciona.

## Antes de começar

Leia, nesta ordem:

1. [Modelo de segurança](docs/src/concepts/security-model.md) — os invariantes que não podem regredir;
2. [Fluxo de uma chamada](docs/src/concepts/execution-flow.md) — o contrato de execução;
3. [Arquitetura do código](docs/src/development/architecture.md) — o mapa dos módulos;
4. [`AGENTS.md`](AGENTS.md) — as mesmas regras em formato operacional, válidas também para pessoas.

## Ambiente

```powershell
cargo build
$env:TORII_CONFIG_DIR = "$PWD/.torii-dev"
cargo run -- init
cargo run -- provider install ./examples/providers/aws
```

Nunca desenvolva contra `~/.config/torii`: isole a raiz com `TORII_CONFIG_DIR`.

No Windows com toolchain GNU, `eframe` exige uma instalação MinGW-w64 completa
(`dlltool.exe` e `as.exe`) no `PATH`. Em CI ou headless, use `TORII_NO_GUI=1`.

## Verificação obrigatória

Execute nesta ordem antes de abrir o PR:

```powershell
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
mdbook build docs
```

## Regras de mudança

- comportamento novo entra com teste que cobre o invariante afetado;
- mudou configuração pública, resposta MCP, CLI ou invariante? atualize o livro no mesmo commit;
- mudou o schema de provider? atualize `docs/src/reference/provider-schema.md` e ao menos um
  pacote em `examples/providers/`;
- mudou a tool MCP? atualize `docs/src/reference/mcp-api.md`;
- mudou o control plane? atualize `docs/src/reference/cli.md`;
- não crie abstração genérica antes de dois providers reais provarem a necessidade;
- não declare suporte a uma estratégia de autenticação só porque o schema aceita seu nome;
- exemplos, fixtures, logs e mensagens de erro usam contas, clusters e credenciais fictícios.

## Commits e PRs

- commits seguem [Conventional Commits](https://www.conventionalcommits.org/pt-br/) —
  `feat:`, `fix:`, `docs:`, `test:`, `chore:`, `ci:`, `style:`, com escopo opcional;
- um PR resolve um assunto e descreve o que muda no contrato público;
- mudanças que afetam usuários entram no [`CHANGELOG.md`](CHANGELOG.md) sob *não lançado*.

## Documentação

A documentação oficial é escrita em português brasileiro, com nomes de tipos, campos,
comandos e protocolos preservados em inglês. Todo capítulo navegável precisa estar em
`docs/src/SUMMARY.md`; o build usa `create-missing = false`. Não edite `docs/book/`.

## Licença

Contribuições são aceitas sob a [AGPL-3.0-only](LICENSE), a mesma licença do projeto.
