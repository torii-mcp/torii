# CLI de controle

Esta CLI pertence ao humano e nunca é publicada como tools MCP.

Todos os níveis aceitam `--help` ou `-h`; `torii help <comando>` e a forma em camadas `torii provider help install` são equivalentes. Por exemplo:

```text
torii --help
torii reauth --help
torii provider --help
torii provider install --help
```

| Comando | Efeito |
|---|---|
| `torii` | inicia o servidor MCP stdio |
| `torii --version` ou `torii -V` | imprime a versão do binário |
| `torii init` | cria raiz e settings, sem instalar providers |
| `torii config-dir` | imprime o diretório de configuração |
| `torii self upgrade [--check]` | troca o próprio binário pela última release |
| `torii provider list` | lista providers locais, versão e origem |
| `torii provider search [query]` | pesquisa o catálogo configurado |
| `torii provider install <source>` | instala nome, diretório, archive ou URL HTTPS |
| `torii provider setup <provider> <setup>` | aplica setup read-only sobre rules vazio |
| `torii provider upgrade <provider>` | troca somente arquivos gerenciados pelo pacote |
| `torii policy show <tool> [target]` | imprime a política ativa e avisa sobre accepts ignorados |
| `torii policy edit <tool> [target] [--create]` | abre a política no editor, valida e só então substitui |
| `torii reauth <tool> [target]` | força autenticação gerenciada no escopo |
| `torii target add <tool> <name> --context <context> --provider <tool> [--scope <scope>] [--expect <identity>]` | valida o context e cria o target autenticado pelo provider de identidade indicado; `--scope` isola o balde de credencial (default: nome do target) e `--expect` fixa a identidade conferida pelo probe |
| `torii target add <tool> <name> --profile <profile> --account-id <12-dígitos> [--region <região>]` | cria um alias `aws_profile` com profile e conta esperada sob controle humano |
| `torii target list <tool>` | lista aliases e seus bindings fixos no control plane humano |
| `torii target show <tool> <name>` | imprime `target.yaml` |
| `torii target activate <tool> <name> [--for <minutes>] [--add]` | concede lease temporário ao alias; por padrão substitui todos os aliases ativos da tool |
| `torii target status <tool>` | mostra o estado dos leases e suas expirações |
| `torii target clear <tool>` | revoga todos os leases da tool, sem alterar o estado operacional |
| `torii target remove <tool> <name> --force` | revoga o lease e remove o target e seu estado isolado |
| `torii agent list` | lista adapters de agentes implementados |
| `torii agent install <agent> [--hook] [--yes]` | registra o MCP no cliente e, quando ele oferecer hook, instala o guard |
| `torii agent status <agent>` | mostra o estado MCP/hook e sua propriedade |
| `torii agent uninstall <agent> [--hook]` | remove toda a integração ou somente o hook |

Install recusa destino existente. Setup recusa rules não vazio. Upgrade requer lock de pacote e preserva rules, `.env`, grants, targets, cache e autenticação. Alterações no conjunto/configuração de providers exigem reiniciar o MCP.

## `update` e `upgrade`

O Torii usa **upgrade** para trocar a versão de algo instalado — `provider upgrade` para um pacote, `self upgrade` para o próprio binário. Não existe `update` em lugar nenhum, e isso é deliberado: na convenção que ferramentas como `apt` e `brew` firmaram, `update` sincroniza um índice local antes de decidir, e `upgrade` troca o que está instalado. O Torii não mantém índice local — `provider search` e `provider install` leem o catálogo na hora, a cada chamada — então não há o que atualizar, só o que trocar de versão. Usar os dois verbos para a mesma ação obrigaria a decorar qual vale para quê.

Quem digitar o nome antigo recebe a correção em vez de um erro genérico:

```text
there is no `torii provider update`; changing an installed version is `torii provider upgrade`
```

`self upgrade` é humano e explícito: nada no Torii se atualiza sozinho, e o MCP não expõe essa ação. Ele resolve a última release da plataforma pelo redirecionamento de `/releases/latest`, confere o SHA-256 publicado e substitui o binário em execução; providers, políticas, targets, grants e credenciais ficam intactos. `--check` apenas informa. Um cliente MCP já em execução continua com o binário anterior até ser reiniciado.

`policy edit` abre uma cópia da política no `$VISUAL` ou `$EDITOR` — `notepad` no Windows e `vi` nos demais sistemas quando nenhum está definido. Um editor que devolve o controle imediatamente precisa ser instruído a esperar, como `code --wait` ou `subl -w`. Ao fechar, o rascunho é parseado e cada regra é compilada; só depois disso ele substitui o arquivo vivo de forma atômica. YAML malformado, regex inválido ou política vazia não chegam ao arquivo ativo, e o rascunho recusado é preservado num caminho informado no erro. Em terminal interativo, o Torii oferece reabrir o editor. Sem alterações, nada é gravado. Como `rules.yaml` é relido em cada chamada, não é preciso reiniciar o MCP.

`policy edit <tool> <target>` exige um alias já configurado. Se aquele alias ainda não tiver política própria, ele herda a compartilhada, e criar uma exige `--create` — porque a política do target **substitui** a do provider naquele alias em vez de somar. `policy show` aceita os mesmos escopos e avisa quando um accept fica abaixo de `minimum_accept_tokens` e, por isso, é ignorado na avaliação.

O provider informado em `--provider` é o provider de identidade (`identity.provider`): precisa estar instalado e não pode exigir target. Cada target autentica no balde `identities/<scope>` desse provider — por padrão um balde por target, então targets distintos não compartilham sessão; use `--scope` para compartilhar de propósito. `--expect` só é aceito se o provider de identidade declarar `auth.identity`. `torii reauth <tool-alvo> <target>` delega ao lifecycle desse provider no escopo do target. Se ele usar `inherited` sem validator, não há material renovável pelo Torii; com validator (SSO/profile externo), o `reauth` aponta para o fluxo nativo.

Para `aws_profile`, use `target add` sem `--provider`: o Torii grava a própria tool no binding e isola cache e lock por alias. `target list aws_profile` mostra alias, profile e conta esperada somente no control plane humano. `torii reauth aws_profile <alias>` não troca a sessão; autentique o profile configurado pelo AWS CLI e repita a chamada.

Todo target-aware nasce inativo, embora continue anunciado no schema MCP. `target activate` é um comando exclusivamente humano: `--for` aceita de 1 a 1.440 minutos e, se omitido, usa `default_target_minutes` (15 por padrão). Sem `--add`, a ativação substitui todos os leases ativos daquela tool. `--add` preserva os demais aliases e deve ser usado somente com a compreensão de que o agente poderá escolher qualquer alias ativo em operações permitidas. `target status` não altera estado.

`target clear` grava um conjunto vazio de leases e invalida decisões de janelas antigas; ele não apaga `target.yaml`, rules, grants Jasper, `.env`, credenciais, cache ou configuração de provider. Tampouco encerra um processo já iniciado, embora bloqueie launches futuros que ainda precisem passar pela conferência de lease.

Não existem `torii aws s3 ls`, `torii kubectl get pods`, instalação via MCP ou atualização automática. Durante o servidor MCP, stdout é reservado; subcomandos humanos usam stdout para dados e stderr para progresso. `reauth` é exclusivamente humano; uma chamada MCP já autorizada abre autenticação humana automaticamente quando a sessão gerenciada não está disponível.

`<agent>` aceita `codex`, `claude`, `gemini`, `cursor`, `antigravity`, `opencode`, `copilot`, `copilot-cli` ou `pi`; `torii agent list` mostra o que cada um suporta. `opencode`, `copilot`, `copilot-cli` e `pi` não oferecem hook, então `--hook` é recusado antes de qualquer escrita e o status informa `hook not supported`. O pi não fala MCP nativamente: sua configuração só é lida por uma extensão MCP instalada pelo humano, então a instalação avisa e exige confirmação interativa ou `--yes`. `agent install` é control plane humano: escreve na configuração global do cliente, preserva entradas existentes e recusa substituir um servidor MCP `torii` conflitante. O estado necessário para remover somente conteúdo gerenciado fica em `<TORII_CONFIG_DIR>/agents/<agent>.json`.
