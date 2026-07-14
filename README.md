# Torii

> A [documentação oficial](docs/src/README.md) vive em `docs/` e é publicada com mdBook.

Torii é uma fronteira local de execução controlada para agentes:

```text
agente → MCP stdio → Torii → Jasper → provider → executável real
```

O humano continua usando `aws`, `kubectl`, `az` ou `gcloud` diretamente. O agente recebe uma única tool MCP por provider instalado. Providers simples usam `{ "args": string[] }`; providers target-aware exigem também um alias `target` configurado pelo humano. O Torii não usa shell, não oferece CLI operacional e não expõe tools de `kill`, `reauth`, configuração ou instalação ao agente.

## Estado desta primeira versão

- servidor MCP local por `stdio`, com lifecycle controlado pelo cliente;
- registry de providers YAML e tools MCP dinâmicas;
- gerenciador declarativo de pacotes com fontes local, archive, URL e catálogo canônico;
- targets Kubernetes que resolvem aliases para contexts sem permitir override pelo agente;
- Jasper com default deny, deny prioritário e matching por prefixo de tokens;
- grants temporários e aprovação humana em janela local;
- autenticação genérica `environment` completa e `inherited` para providers sem coleta;
- janela de autenticação gerada pelos campos do provider e clipboard com allowlist;
- validação antes da substituição atômica da sessão;
- grants, cache, autenticação e lock isolados por provider/target;
- credenciais e `.env` carregados somente depois da autorização;
- ambiente aplicado exclusivamente ao processo filho;
- captura de stdout/stderr/exit code com limite explícito;
- auditoria central sem credenciais e sem lista completa de argumentos;
- migração automática e não destrutiva de `~/.config/.awsgate` para o provider AWS quando a configuração padrão do Torii ainda não existe;
- control plane humano para instalar, pesquisar, configurar e atualizar providers sem expor essas ações ao MCP;

`session_command` e `credential_file` são reconhecidos na configuração e recusados com erro explícito; serão implementados quando um provider real provar a necessidade.

## Começando

Baixe o pacote da sua plataforma nos [releases do Torii](https://github.com/torii-mcp/torii/releases). Cada release publica:

- `torii-vX.Y.Z-windows-x86_64.zip`;
- `torii-vX.Y.Z-linux-x86_64.tar.gz`;
- um arquivo `.sha256` para cada pacote.

Também é possível compilar localmente:

```powershell
cargo build
$env:TORII_CONFIG_DIR = "$PWD/.torii-dev"
cargo run -- init
cargo run -- provider install ./examples/providers/aws
cargo run -- provider setup aws readonly
cargo run -- provider list
```

Fontes aceitas pelo install:

```powershell
cargo run -- provider install aws
cargo run -- provider install ./meu-provider
cargo run -- provider install ./meu-provider.zip
cargo run -- provider install https://example.org/meu-provider.tar.gz
```

Um nome simples é resolvido no catálogo oficial [`torii-mcp/torii-canon-providers`](https://github.com/torii-mcp/torii-canon-providers). Durante o desenvolvimento ou para catálogos privados, `TORII_PROVIDER_CATALOG` pode apontar para outro `index.yaml` local ou HTTPS.

Providers não possuem build específico por sistema operacional. São pacotes declarativos ZIP contendo manifest, configuração, regras vazias e setups opcionais. O catálogo associa cada nome à URL HTTPS do asset de uma release e ao SHA-256 esperado; o Torii baixa o asset, verifica o hash e só então valida e instala seu conteúdo.

Todo pacote instala `rules.yaml` vazio. Somente `provider setup <provider> <setup>` aplica uma política de exemplo read-only, recusando sobrescrever uma política não vazia. `provider update` nunca escreve em `rules.yaml`, `.env`, grants, targets, cache ou autenticação.

Revise os arquivos criados em `$TORII_CONFIG_DIR/providers`. Para renovar AWS:

```powershell
cargo run -- reauth aws
```

Para cadastrar outro cluster Kubernetes usando um context já existente no kubeconfig:

```powershell
cargo run -- target add kubectl cliente_hml --context eks-cliente-hml
```

Ao executar `torii` sem subcomandos, ele fala MCP em stdout. Logs e diagnósticos ficam fora do transporte; nunca escreva mensagens normais no stdout do servidor.

Configuração típica do cliente MCP:

```json
{
  "mcpServers": {
    "torii": {
      "command": "C:/caminho/para/torii.exe",
      "env": {
        "TORII_CONFIG_DIR": "C:/Users/voce/.config/torii"
      }
    }
  }
}
```

Use `TORII_NO_GUI=1` em CI/headless. Nesse modo, chamadas não resolvidas são negadas e autenticação que exija coleta é cancelada com segurança. `AWSGATE_CONFIG_DIR` e `AWSGATE_NO_GUI` são aceitas temporariamente como fallback de migração.

## Provider

Cada subdiretório em `providers/` possui `provider.yaml`, `rules.yaml` e, quando instalado como pacote, metadados imutáveis em `.torii-package/`. Um provider target-aware também possui `targets/<alias>/target.yaml`; grants, cache, ambiente e autenticação ficam fora do conteúdo atualizado do pacote. Consulte as [fixtures de pacote](examples/providers).

As regras descrevem somente o que atravessa:

```yaml
version: "1.0"
deny:
  - "ecs execute-command"
accept:
  - "ec2 describe-instances"
```

`ec2 describe-instances --region sa-east-1` casa com o accept; `ec2 describe` não casa parcialmente. Um deny compatível sempre vence qualquer accept.

## Desenvolvimento

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
mdbook build docs
```

No Windows GNU, `eframe` precisa de uma instalação MinGW-w64 completa (`dlltool` e `as.exe`) no `PATH`, como no AWS Gate de referência.

## Licença

Torii é distribuído exclusivamente sob a [GNU Affero General Public License v3.0](LICENSE), identificador SPDX `AGPL-3.0-only`.
