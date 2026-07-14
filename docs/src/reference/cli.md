# CLI de controle

Esta CLI pertence ao humano e nunca é publicada como tools MCP.

| Comando | Efeito |
|---|---|
| `torii` | inicia o servidor MCP stdio |
| `torii init` | cria raiz e settings, sem instalar providers |
| `torii config-dir` | imprime o diretório de configuração |
| `torii provider list` | lista providers locais, versão e origem |
| `torii provider search [query]` | pesquisa o catálogo configurado |
| `torii provider install <source>` | instala nome, diretório, archive ou URL HTTPS |
| `torii provider setup <provider> <setup>` | aplica setup read-only sobre rules vazio |
| `torii provider update <provider>` | atualiza somente arquivos gerenciados pelo pacote |
| `torii reauth <tool> [target]` | força autenticação gerenciada no escopo |
| `torii target add <tool> <name> --context <context>` | valida o context e cria o target |
| `torii target list <tool>` | lista alias e context |
| `torii target show <tool> <name>` | imprime `target.yaml` |
| `torii target remove <tool> <name> --force` | remove o target e seu estado |
| `torii agent list` | lista adapters de agentes implementados |
| `torii agent install <agent> [--hook]` | registra o MCP em Codex, Claude, Gemini ou Cursor e, opcionalmente, instala o guard |
| `torii agent status <agent>` | mostra o estado MCP/hook e sua propriedade |
| `torii agent uninstall <agent> [--hook]` | remove toda a integração ou somente o hook |

Install recusa destino existente. Setup recusa rules não vazio. Update requer lock de pacote e preserva rules, `.env`, grants, targets, cache e autenticação. Alterações no conjunto/configuração de providers exigem reiniciar o MCP.

Não existem `torii aws s3 ls`, `torii kubectl get pods`, instalação via MCP ou atualização automática. Durante o servidor MCP, stdout é reservado; subcomandos humanos usam stdout para dados e stderr para progresso.

`<agent>` aceita `codex`, `claude`, `gemini` ou `cursor`. `agent install` é control plane humano: escreve na configuração global do cliente, preserva entradas existentes e recusa substituir um servidor MCP `torii` conflitante. O estado necessário para remover somente conteúdo gerenciado fica em `<TORII_CONFIG_DIR>/agents/<agent>.json`.
