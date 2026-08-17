# Referência rápida

Uma página para consultar durante a operação. Cada item aponta para o capítulo que explica
o porquê. Exemplos em PowerShell; em shells POSIX troque `$env:VAR = "valor"` por
`export VAR=valor`.

## Modelo mental em cinco linhas

```text
agente → MCP stdio → Torii → lease do target → Jasper → sessão isolada → CLI real
```

1. o humano continua usando `aws`, `kubectl` e `az` direto no terminal;
2. o agente recebe **uma tool por provider instalado**, nunca uma tool por operação;
3. tudo começa negado: sem `accept`, grant ou aprovação humana, não passa;
4. um `deny` explícito vence accept, grant e aprovação humana;
5. alias target-aware nasce inativo: precisa de um **lease** humano temporário.

Detalhes em [Modelo mental](../concepts/mental-model.md) e
[Modelo de segurança](../concepts/security-model.md).

## Instalar e apontar a raiz

```powershell
torii --version          # confirma o binário
torii init               # cria a raiz e settings.yaml
torii config-dir         # mostra a raiz efetiva
```

| Variável | Efeito |
|---|---|
| `TORII_CONFIG_DIR` | substitui a raiz inteira (default `~/.config/torii`) |
| `TORII_NO_GUI=1` | headless: chamada não resolvida é negada, coleta é cancelada |
| `TORII_PROVIDER_CATALOG` | usa outro `index.yaml` (local ou HTTPS) no lugar do catálogo oficial |

## Conectar um cliente MCP

```powershell
torii agent list                       # adapters implementados
torii agent install claude --hook      # codex | claude | gemini | cursor
torii agent status claude
torii agent uninstall claude --hook    # remove tudo; sem --hook remove só o MCP
```

Reinicie o cliente depois de instalar ou remover. O `--hook` bloqueia chamadas diretas aos
executáveis dos providers instalados: reduz bypass acidental, não substitui sandbox do
sistema operacional. Configuração manual em [Conectar um cliente MCP](mcp-client.md) e
[Integrar agentes](../guides/agents.md).

## Configurar um provider

```powershell
torii provider search              # consulta o catálogo
torii provider search kubernetes
torii provider install aws         # nome do catálogo
torii provider install ./examples/providers/az   # diretório, .zip, .tar.gz ou URL HTTPS
torii provider setup aws readonly  # aplica política de exemplo read-only
torii provider list                # tool, nome, executável, versão e origem
torii provider update aws          # só arquivos do pacote; nunca rules, .env ou estado
```

Todo pacote instala `rules.yaml` **vazio**: nada atravessa até você escrever a política.
`setup` recusa sobrescrever uma política que já tenha accepts ou denies. Ver
[Operar providers e sessões](../guides/control-plane.md) e
[Pacotes e catálogo](../reference/provider-packages.md).

## Ajustar permissões (política)

Edite `providers/<provider>/rules.yaml`. Um `targets/<alias>/rules.yaml` substitui a
política compartilhada naquele alias.

```yaml
version: "1"
deny:
  - "ecs execute-command"          # prefixo de tokens
  - "/(?i)\\bdrop\\s+table\\b/i"   # regex sobre o argv inteiro
accept:
  - "ec2 describe-instances"
  - "s3 ls"
```

| Forma | Como casa |
|---|---|
| `"ec2 describe-instances"` | prefixo de tokens; `ec2 describe` **não** casa parcialmente |
| `"/padrão/flags"` | regex em qualquer posição do argv; flags `i`, `m`, `s`, `x` |
| `forbidden_args` | argumento negado em qualquer posição, antes de qualquer regra |
| `ignore_args` | normaliza o argv **só** para avaliar; nunca altera o comando executado |

`ec2 describe-instances --region sa-east-1` casa com o accept acima. Regex inválido falha
fechado (erro, nunca allow silencioso). `rules.yaml` é relido em cada chamada — não precisa
reiniciar o MCP. Ver [Escrever políticas](../guides/policies.md) e
[Schema de provider](../reference/provider-schema.md).

Para uma aprovação pontual, deixe a chamada cair em *unresolved*: a janela local permite
negar, permitir uma vez ou conceder um grant temporário. O agente nunca edita política.

## Targets e leases (tools target-aware)

```powershell
# Kubernetes: alias → context do kubeconfig, autenticado por outro provider
torii target add kubectl dev --context mdb-k8s-dev-ia --provider aws --expect 111122223333

# AWS por profile humano: alias → profile + conta esperada
torii target add aws_profile producao --profile empresa-producao --account-id 111122223333 --region sa-east-1

torii target list kubectl          # aliases e bindings (só no control plane humano)
torii target show kubectl dev
torii target activate kubectl dev --for 30   # concede o lease; substitui os ativos da tool
torii target status kubectl        # leases vivos e expirações
torii target clear kubectl         # revoga todos os leases da tool
torii target remove kubectl dev --force
```

| Fato | Consequência |
|---|---|
| criar alias não ativa | o alias aparece no schema MCP, mas a chamada exige lease |
| `--for` aceita 1 a 1.440 min | sem `--for`, usa `default_target_minutes` (15) |
| ativação normal substitui | todos os outros aliases ativos daquela tool são desativados |
| `--add` acumula | o agente pode escolher **qualquer** alias ativo em operação permitida |
| `clear` só revoga leases | não apaga target, rules, grants, `.env`, cache nem mata processos |
| mudou `target.yaml` | o digest do binding invalida o lease imediatamente |

Criar ou remover alias muda o enum do schema e exige reiniciar o MCP; ativar, limpar ou
expirar não. Ver [Configurar Kubernetes](../guides/kubernetes.md) e
[AWS por profile e aliases](../guides/aws-profiles.md).

## Autenticar e reautenticar

```powershell
torii reauth aws                # provider simples, autenticação gerenciada
torii reauth kubectl dev        # delega ao provider de identidade, no escopo do target
aws sso login --profile empresa-producao   # aws_profile: fluxo nativo, fora do Torii
```

| Estratégia | Reauth pelo Torii |
|---|---|
| `environment` | sim: janela coleta os campos e valida antes de substituir a sessão |
| `inherited` sem validator | não há material renovável; a sessão do ambiente é usada como está |
| `inherited` com validator (SSO/profile) | não: autentique pelo fluxo nativo e repita a chamada |
| `aws_profile` | não troca sessão: autentique o profile configurado e repita o mesmo alias |

Uma chamada já autorizada abre a janela de autenticação sozinha quando a sessão gerenciada
não está disponível. Não existe tool MCP de reauth. Ver
[Sessões de autenticação](../concepts/authentication.md).

## O que o agente vê

```json
{ "name": "aws",     "arguments": { "args": ["s3", "ls"] } }
{ "name": "kubectl", "arguments": { "target": "dev", "args": ["get", "pods"] } }
{ "name": "torii_policy", "arguments": { "provider": "kubectl", "target": "dev" } }
```

`torii_policy` é somente leitura: devolve `accept`, `deny`, `minimum_accept_tokens` e
`ignored_accept`, sem tocar credenciais, grants ou leases. O agente **não** recebe tools de
reauth, kill, instalação, edição de política ou ativação de target. Ver
[API MCP](../reference/mcp-api.md).

## Ler a decisão e a auditoria

A resposta traz `decision.result`, `decision.source` e, quando executou, `execution` com
`exit_code`, `stdout`, `stderr` e `truncated`. O log fica em `<raiz>/torii.log`:

```text
epoch | escopo | evento | regra-curta | detalhe
1784000000 | aws            | allowed-by-rules | ec2 describe-instances
1784000003 | kubectl/dev    | ran              | get pods | exit=0
```

| Evento | Leitura |
|---|---|
| `allowed-by-rules` / `allowed-by-grant` | passou por accept ou por grant vivo |
| `denied-explicit` | casou um `deny`; nada mais foi consultado |
| `override-once` / `override-timed` | humano aprovou na janela, uma vez ou por tempo |
| `target-access-*` | pedido, substituição, adição, revogação ou perda de lease |
| `identity-mismatch` | conta ativa diferente da esperada; comando não executou |
| `session-*` | estado da sessão do escopo de credencial |

Sem credenciais, clipboard ou saída completa. Escrita best-effort: é observabilidade local,
não ledger de compliance. Ver [Auditoria](../reference/audit.md).

## Quando reiniciar o servidor MCP

| Mudança | Reiniciar? |
|---|---|
| editar `rules.yaml` | não |
| ativar, limpar ou expirar lease | não |
| conceder grant temporário | não |
| instalar, atualizar ou remover provider | sim |
| criar ou remover target | sim |
| alterar `PATH` ou `TORII_CONFIG_DIR` | sim |

## Diagnóstico rápido

| Sintoma | Primeira verificação |
|---|---|
| `no providers installed` | `torii config-dir` e `torii provider list`; `init` não instala providers |
| `rules file not found` | o provider não tem `rules.yaml`; não existe fallback permissivo |
| `could not find the executable ... in PATH` | `where <cli>` no mesmo terminal e reinicie o cliente MCP |
| negado sem abrir janela | casou um `deny`, ou `TORII_NO_GUI` está setado |
| alias inativo / negado em headless | `torii target status <tool>` e `target activate` |
| muitos aliases ativos | `target status`, depois `target clear` ou ativar sem `--add` |
| conta divergente em `aws_profile` | autentique o profile e confira `torii target show` |
| `execution.truncated: true` | saída acima de `max_output_bytes` em `settings.yaml` |

Casos completos em [Solução de problemas](../operations/troubleshooting.md).

## Não faça

- não peça ao agente para trocar context, profile, conta ou região: isso é control plane humano;
- não deixe vários aliases ativos sem necessidade;
- não escreva política permissiva contando com RBAC/IAM, nem o contrário: as duas camadas somam;
- não versione nem sincronize a raiz de configuração; ela contém sessão e credenciais;
- não aceite operações que devolvem segredos, tokens ou credenciais em política read-only;
- não trate o hook de agente como isolamento real de processo.
