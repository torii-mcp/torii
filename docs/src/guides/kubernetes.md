# Configurar Kubernetes

Torii publica uma única tool `kubectl`. O campo MCP `target` escolhe um alias cadastrado pelo humano; o agente nunca escolhe o context real.

## Provider único

```yaml
version: "1"
name: kubectl
tool: kubectl
command: kubectl
targeting:
  mode: kubectl_context
```

Cadastre um context já existente no kubeconfig:

```powershell
torii provider install aws
torii target add kubectl dev --context mdb-k8s-dev-ia --provider aws
torii target add kubectl hml --context mdb-k8s-hml-ia --provider aws
torii target list kubectl
torii target show kubectl dev
torii target activate kubectl dev --for 30
```

`target add` executa `kubectl config get-contexts <context> -o name` e só grava o target quando o context existe. O `--provider` indica o **provider de identidade** (o campo `identity.provider` do target): quem autentica a sessão usada por aquele context. Ele precisa estar instalado e não pode exigir target. O alias criado começa inativo: `target activate` concede um lease humano de 1 a 1.440 minutos (15 por padrão) antes de grants, ambiente ou autenticação. Reinicie o servidor MCP depois de adicionar ou remover targets, pois o registry e o enum do schema são construídos no startup; aliases inativos continuam no enum.

### Identidade e escopo de credencial

Cada target tem um bloco `identity`:

```yaml
version: "1"
name: dev
context: mdb-k8s-dev-ia
identity:
  provider: aws          # quem roda o lifecycle de autenticação
  scope: dev             # balde de credencial; default = nome do target
  expect: "009160073200" # opcional; conferido pelo probe do provider antes de executar
```

O **escopo** (`identity.scope`) é a chave do balde de credenciais. Por padrão vale o nome do target, então `dev` e `hml` autenticam de forma independente: `torii reauth kubectl dev` renova só aquele balde e os dois podem ter lease ativo ao mesmo tempo sem uma sessão derrubar a outra. Para compartilhar de propósito uma sessão entre vários contexts da mesma conta, dê o mesmo `--scope` aos targets.

O `expect` é opcional. Quando presente, o Torii roda o probe `auth.identity` do provider de identidade (para o `aws`, um `sts get-caller-identity` lendo o campo `Account`) e recusa a execução se a identidade ativa não bater — transformando um `401`/conta-errada silencioso em erro explícito antes de qualquer comando tocar o cluster. Exigir `expect` sem o provider declarar o probe é erro de configuração.

```powershell
torii target add kubectl dev --context mdb-k8s-dev-ia --provider aws --scope dev --expect 009160073200
```

Uma chamada do agente:

```json
{
  "target": "lab",
  "args": ["get", "pods", "-n", "default"]
}
```

vira conceitualmente:

```text
kubectl --context local-context get pods -n default
```

Depois de confirmar que nenhum deny explícito corresponde à chamada, o Torii exige lease humano válido para o alias antes de consultar grants Jasper ou abrir aprovação da operação. Se o lease estiver inativo, a janela privada mostra o context e os aliases ativos; o humano pode substituir os ativos, adicionar outro ou negar. Quando a adição criar múltiplos ativos, o alerta aparece junto às ações e exige manter **Adicionar** pressionado por 2 segundos. Depois que a política permite a chamada, o Torii relê o lease e executa o lifecycle de autenticação do provider referenciado pelo target. Se a sessão precisar ser coletada ou renovada, a interface e o validator são os desse provider. Após sucesso, somente o ambiente necessário é sobreposto ao processo `kubectl`.

Um deny explícito encerra antes de ler ambiente, cache ou credenciais e antes de executar o lifecycle desse provider. Uma falha ou cancelamento no preflight impede a execução do `kubectl` solicitado. Nesta versão, instalar o provider autenticador também publica sua tool MCP; deixe suas rules em default deny se o agente não precisar usá-la diretamente.

Flags capazes de trocar identidade ou endpoint, incluindo `--context`, `--kubeconfig`, `--cluster`, `--user`, `--token` e `--server`, são recusadas nos argumentos MCP. A lista completa está no contrato de [provider](../reference/provider-schema.md).

## Política read-only

O `rules.yaml` do provider é compartilhado por padrão. Um `targets/<alias>/rules.yaml` existente substitui a política compartilhada somente naquele target.

```yaml
deny:
  - "exec"
  - "attach"
  - "port-forward"
  - "proxy"
  - "config"
  - "delete namespace"
accept:
  - "get pods"
  - "get deployments"
  - "describe pod"
  - "logs"
  - "rollout status"
```

Grants temporários e `.env` ficam isolados no diretório do target. O `.env` do target sobrepõe chaves do `.env` compartilhado. Credenciais, `.session-cache`, `.identity-cache` e lock vivem no balde do provider de identidade em `providers/<provider>/identities/<scope>/`, isolados por escopo; dois targets só os compartilham quando declaram o mesmo `scope`. O conjunto de leases fica no escopo da tool, com uma entrada por target: ativar sem `--add` substitui todos os aliases ativos; adicionar outro significa que o agente poderá escolher qualquer alias ativo até a expiração ou `target clear`.

## RBAC continua obrigatório

Jasper só decide se a tentativa atravessa. A identidade selecionada no kubeconfig deve ter RBAC de menor privilégio. Uma política local permissiva não amplia RBAC; um RBAC permissivo também não substitui a política local.

Todo target exige `identity.provider`. `torii reauth kubectl dev` delega para o lifecycle desse provider no escopo do target. Um provider `inherited` sem validator passa por esse lifecycle como `session-unchecked`; um provider `inherited` com validator (login via SSO/profile externo) não pode ser renovado pelo Torii — o `reauth` aponta o humano para o fluxo nativo.
