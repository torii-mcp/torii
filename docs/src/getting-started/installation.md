# Instalação

## Binários oficiais

Os [releases do Torii](https://github.com/torii-mcp/torii/releases) publicam dois pacotes para cada tag:

| Plataforma | Pacote |
|---|---|
| Windows x86_64 | `torii-vX.Y.Z-windows-x86_64.zip` |
| Linux x86_64 | `torii-vX.Y.Z-linux-x86_64.tar.gz` |

Cada pacote acompanha um arquivo `.sha256`. Extraia o executável e coloque-o em um diretório do `PATH` ou use seu caminho absoluto na configuração do cliente MCP.

## Pré-requisitos

- Rust estável compatível com as dependências do projeto;
- o executável de cada provider no `PATH`, como `aws` ou `kubectl`;
- acesso a uma interface gráfica para aprovação e coleta de credenciais, ou `TORII_NO_GUI=1` para execução estritamente headless;
- mdBook 0.5 ou mais recente para construir esta documentação.

## Compilar o Torii

Na raiz do repositório:

```powershell
cargo build --release
```

O binário será criado em `target/release/torii.exe` no Windows ou `target/release/torii` em sistemas Unix.

As releases são produzidas automaticamente pelo GitHub Actions quando uma tag `vX.Y.Z` é enviada. O Windows usa o target MSVC e o Linux usa `x86_64-unknown-linux-gnu`.

## Particularidade do Windows GNU

`eframe`, usado pelas janelas locais, exige um MinGW-w64 completo durante o link. Se aparecer `error calling dlltool` ou ausência de `as.exe`, adicione ao `PATH` o diretório `bin` de uma distribuição MinGW-w64 completa antes de executar `cargo build` ou `cargo test`.

Isso é uma exigência de build, não uma configuração do Torii.

## Instalar mdBook

Binários prontos estão disponíveis nos releases do projeto mdBook. Quem já possui Cargo também pode instalar a ferramenta:

```powershell
cargo install mdbook --version 0.5.4 --locked
mdbook build docs
```

O HTML gerado fica em `docs/book/` e não deve ser versionado.

## Diretório de configuração

Por padrão, Torii usa:

```text
~/.config/torii
```

Para desenvolvimento ou testes, isole a configuração:

```powershell
$env:TORII_CONFIG_DIR = "$PWD/.torii-dev"
```

`AWSGATE_CONFIG_DIR` ainda funciona como alias temporário quando `TORII_CONFIG_DIR` não está definido. Prefira sempre o nome novo.
