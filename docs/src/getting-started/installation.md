# Instalação

## Instalação rápida

Linux x86_64:

```bash
curl -fsSL https://raw.githubusercontent.com/torii-mcp/torii/main/install.sh | sh
```

Windows x86_64:

```powershell
irm https://raw.githubusercontent.com/torii-mcp/torii/main/install.ps1 | iex
```

Os dois scripts baixam a última release, **conferem o SHA-256 publicado antes de extrair** e instalam o binário num diretório do usuário — nada exige administrador ou `sudo`. Sem um verificador de hash disponível, a instalação para em vez de prosseguir sem conferir.

| | Linux | Windows |
|---|---|---|
| destino padrão | `~/.local/bin` | `%LOCALAPPDATA%\Programs\Torii` |
| mudar destino | `--dir <caminho>` ou `TORII_INSTALL_DIR` | `-InstallDir <caminho>` |
| versão fixa | `--version v0.2.0` ou `TORII_VERSION` | `-Version v0.2.0` |
| PATH | só com `--add-to-path` | automático, salvo `-NoPathUpdate` |

No Linux, o script não mexe no seu shell rc por padrão: ele imprime a linha do `export PATH` para você colar. Com `--add-to-path`, escreve um bloco marcado no rc do seu shell (`.bashrc`, `.zshrc`, `config.fish` ou `.profile`) e não duplica em execuções seguintes. No Windows, o destino entra no PATH do usuário; abra um novo terminal para enxergá-lo.

Para passar opções através do pipe, use `sh -s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/torii-mcp/torii/main/install.sh | sh -s -- --add-to-path
```

Executar um script vindo da rede é um ato de confiança. Se preferir revisar antes — o que é razoável para uma ferramenta que existe para restringir execução — baixe, leia e só então rode:

```bash
curl -fsSLO https://raw.githubusercontent.com/torii-mcp/torii/main/install.sh
less install.sh
sh install.sh
```

Reinstalar sobre uma versão anterior é o caminho normal de atualização: o script troca o binário e informa a versão anterior e a nova. Depois de instalado, `torii self update` faz o mesmo sem baixar script nenhum.

## Binários oficiais

Os [releases do Torii](https://github.com/torii-mcp/torii/releases) publicam dois pacotes para cada tag:

| Plataforma | Pacote |
|---|---|
| Windows x86_64 | `torii-vX.Y.Z-windows-x86_64.zip` |
| Linux x86_64 | `torii-vX.Y.Z-linux-x86_64.tar.gz` |

Cada pacote acompanha um arquivo `.sha256`. Extraia o executável e coloque-o em um diretório do `PATH` ou use seu caminho absoluto na configuração do cliente MCP.

Para conferir o download à mão:

```bash
sha256sum -c torii-v0.2.0-linux-x86_64.tar.gz.sha256
```

```powershell
(Get-FileHash torii-v0.2.0-windows-x86_64.zip -Algorithm SHA256).Hash
```

O checksum é publicado junto do pacote, no mesmo release. Ele detecta download corrompido ou truncado; não é assinatura criptográfica, e o [roadmap](../development/roadmap.md) registra a assinatura de artefatos como fora do escopo atual.

## Atualizar

```powershell
torii self update --check   # só informa se existe versão nova
torii self update           # baixa, confere o SHA-256 e troca o binário
```

O comando resolve a última release para a sua plataforma, confere o checksum e substitui o executável em execução. Configuração, políticas, targets, grants e credenciais não são tocados. No Windows, o binário anterior fica como `torii.exe.old` enquanto o processo atual o mantém aberto; a atualização seguinte o remove, e o comando avisa quando isso acontece.

Reinicie os clientes de agente depois de atualizar: um cliente MCP em execução continua com o binário anterior carregado.

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

O Torii não lê `AWSGATE_CONFIG_DIR` e não migra configuração do AWS Gate automaticamente.
