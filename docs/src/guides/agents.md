# Integrar agentes e instalar o hook

O Torii integra Codex, Claude Code, Gemini CLI, Cursor, opencode, GitHub Copilot (VS Code e CLI) e pi. A integração é control plane humano: nenhum comando desta página aparece como tool MCP.

Liste os adapters disponíveis e o que cada um suporta:

```text
torii agent list
```

Registre o servidor MCP stdio `torii` em um cliente:

```text
torii agent install <agent>
```

Nem todo cliente oferece as duas metades da integração. O hook depende de o cliente publicar um evento antes da execução de shell:

| Adapter | MCP | Hook |
|---|---|---|
| `codex`, `claude`, `gemini`, `cursor` | sim | sim |
| `opencode`, `copilot`, `copilot-cli` | sim | não existe no cliente |
| `pi` | somente através de uma extensão MCP instalada por você | não implementado pelo Torii |

Para os adapters sem hook, `--hook` é recusado antes de qualquer escrita, e `agent status` informa `hook not supported`. Isso não é uma limitação do Torii: aqueles clientes não expõem um ponto de interceptação equivalente, então o Torii não pode prometer bloquear a chamada direta ao `aws` ou ao `kubectl` neles.

A configuração fixa o caminho absoluto do executável Torii e o `TORII_CONFIG_DIR` usado durante a instalação. Reinicie o cliente para carregar a alteração.

## Descoberta e autenticação

O MCP instrui o agente a consultar `torii_policy` antes de selecionar uma operação. A tool devolve, somente para leitura, os `accept` e `deny` do provider ou target ativo; ela não executa CLIs e não lê ambiente ou credenciais.

O agente não recebe tools de `reauth`, ativação, limpeza ou edição de targets. Quando uma chamada target-aware seleciona um alias inativo, o Torii pede ao humano um lease para o binding antes de consultar grants, ambiente ou sessão. Em headless, isso termina em negação. Para trocar ou renovar uma sessão gerenciada antes da chamada, o humano usa `torii reauth <provider-tool> [target]` no control plane.

O agente não deve tratar um alias listado no schema como ambiente ativo: a lista mostra aliases configurados, não leases. Se **Adicionar** criar vários aliases ativos, a interface alerta o humano junto às ações e exige manter o botão pressionado por 1 segundo. Depois da confirmação, o agente poderá selecionar qualquer alias ativo nas operações permitidas; por isso deve escolher pelo alias semântico pedido pelo humano e não tentar alternar targets por conta própria.

Para um target `aws_profile`, a conta ou o profile não são expostos ao agente. Se o Torii informar identidade ausente ou conta divergente, o agente pede que o humano autentique o profile já configurado pelo fluxo nativo AWS e repete o mesmo alias. Ele não tenta `reauth`, troca de target nem flags `--profile`/`--region`.

> Se o comando for executado por `cargo run`, o cliente ficará apontando para o binário em `target/debug`. Prefira uma release instalada antes de configurar o agente.

## Arquivos alterados

| Adapter | MCP | Chave | Hook |
|---|---|---|---|
| Codex | `$CODEX_HOME/config.toml` | `mcp_servers` | `$CODEX_HOME/hooks.json` |
| Claude Code | `~/.claude.json` | `mcpServers` | `~/.claude/settings.json` |
| Gemini CLI | `$GEMINI_CLI_HOME/.gemini/settings.json` | `mcpServers` | o mesmo `settings.json` |
| Cursor | `~/.cursor/mcp.json` | `mcpServers` | `~/.cursor/hooks.json` |
| opencode | `$XDG_CONFIG_HOME/opencode/opencode.json` | `mcp` | — |
| Copilot no VS Code | `mcp.json` do perfil do usuário | `servers` | — |
| Copilot CLI | `$COPILOT_HOME/mcp-config.json` | `mcpServers` | — |
| pi | `~/.pi/agent/mcp.json` | `mcpServers` | — |

Sem as variáveis de override, Codex usa `~/.codex`, Gemini usa `~/.gemini`, opencode usa `~/.config/opencode`, a CLI do Copilot usa `~/.copilot` e o pi usa `~/.pi/agent`. Quando `CLAUDE_CONFIG_DIR` está definido, os arquivos do Claude passam a ser `<dir>/.claude.json` e `<dir>/settings.json`.

O perfil de usuário do VS Code depende do sistema: `%APPDATA%\Code\User` no Windows, `~/Library/Application Support/Code/User` no macOS e `$XDG_CONFIG_HOME/Code/User` no Linux. Use `TORII_COPILOT_HOME` para apontar outro perfil, por exemplo o do VS Code Insiders.

Os overrides `TORII_CURSOR_HOME`, `TORII_OPENCODE_HOME`, `TORII_COPILOT_HOME`, `TORII_COPILOT_CLI_HOME` e `TORII_PI_HOME` selecionam outro diretório, principalmente para automação e testes.

Cada cliente recebe o formato que ele mesmo entende: opencode declara `type: local` com o comando em vetor e o ambiente em `environment`; a CLI do Copilot chama stdio de `local` e exige a lista `tools`; Claude e Copilot no VS Code usam `type: stdio`. O Torii escreve apenas a entrada `torii` e preserva o resto do arquivo.

Se o opencode já mantiver a configuração em `opencode.jsonc`, o Torii recusa a instalação em vez de reescrever o arquivo: ele só edita JSON puro e a reescrita descartaria seus comentários. Adicione a entrada manualmente ou migre para `opencode.json`.

O instalador preserva outras configurações. Se já existir um servidor MCP `torii` diferente, ele para sem substituí-lo.

## pi exige uma extensão MCP

O pi não fala MCP nativamente: o suporte vem de extensões da comunidade instaladas em `~/.pi/agent/extensions/`. O Torii grava `~/.pi/agent/mcp.json` no formato compartilhado pelos outros hosts, mas quem lê esse arquivo é a extensão, não o pi.

Por isso a instalação avisa e pede confirmação:

```text
torii agent install pi
torii agent install pi --yes
```

Sem uma extensão MCP no pi, o arquivo é simplesmente ignorado e nada funciona. Se você não sabe de qual extensão se trata, não prossiga: instale primeiro a extensão MCP e só depois a integração do Torii. Em ambiente sem terminal interativo, a instalação é recusada até que `--yes` confirme a decisão.

## Hook opcional

Para instalar também o guard de execução direta, em um adapter que ofereça hook:

```text
torii agent install <codex|claude|gemini|cursor> --hook
```

Cada adapter usa o evento nativo do cliente:

| Adapter | Evento protegido | Tool de shell |
|---|---|---|
| Codex | `PreToolUse` | `Bash` |
| Claude Code | `PreToolUse` | `Bash` |
| Gemini CLI | `BeforeTool` | `run_shell_command` |
| Cursor | `beforeShellExecution` | shell do agente |

Antes da chamada, o cliente envia o comando ao próprio Torii. O guard carrega o registry atual e compara o executável tentado com o campo `command` de cada provider.

Com um provider que declara `command: kubectl`, esta tentativa é bloqueada:

```text
kubectl get pods
```

A resposta orienta o agente a chamar a tool MCP `kubectl`, selecionar um target anunciado e enviar somente os argumentos posteriores ao executável. Nome com extensão, caminho absoluto, comandos encadeados e invocações comuns por outro shell também são reconhecidos.

O hook é do Torii, não do pacote. Providers não carregam scripts ou configuração específica de agentes. Instalar, atualizar ou remover um provider muda dinamicamente o conjunto protegido sem reescrever a configuração do agente.

Se o input do hook for inválido ou o registry não puder ser carregado, a chamada de shell é negada. Com nenhum provider instalado, não existe executável para bloquear.

## Estado e remoção

Inspecione a integração:

```text
torii agent status <agent>
```

O status diferencia conteúdo gerenciado pelo Torii de uma entrada preexistente.

Remova somente o guard, preservando o MCP:

```text
torii agent uninstall <agent-com-hook> --hook
```

Ou remova toda a integração gerenciada:

```text
torii agent uninstall <agent>
```

O Torii mantém metadados em `<TORII_CONFIG_DIR>/agents/<adapter>.json` para remover somente as entradas que criou. Se uma entrada gerenciada tiver sido alterada depois, a remoção para em vez de apagar configuração do usuário.

## Limite de segurança

O hook bloqueia o caminho comum e torna a negação visível ao agente, mas não é uma sandbox nem uma fronteira completa. Um processo com acesso às mesmas credenciais ainda pode tentar outra biblioteca, outro executável ou um mecanismo não coberto pelo hook.

Use as camadas em conjunto:

1. instruções MCP orientam o agente;
2. o hook bloqueia chamadas diretas reconhecidas;
3. o sandbox do agente limita caminhos alternativos;
4. credenciais e identidades de menor privilégio limitam o impacto real.

Em opencode, Copilot e pi a camada 2 não existe. Ali o agente continua podendo chamar `aws` ou `kubectl` pelo shell dele sem passar pelo Torii, então as camadas 3 e 4 carregam sozinhas o peso: use o sandbox do cliente e credenciais de menor privilégio, e não trate a política do Torii como se fosse a única barreira.

Consulte as referências oficiais de [hooks do Codex](https://developers.openai.com/codex/hooks), [hooks do Claude Code](https://code.claude.com/docs/en/hooks), [hooks do Gemini CLI](https://geminicli.com/docs/hooks/reference/) e [hooks do Cursor](https://cursor.com/docs/hooks), além do [modelo de segurança do Torii](../concepts/security-model.md).
