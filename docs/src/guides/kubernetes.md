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
torii target add kubectl lab --context local-context --provider aws
torii target list kubectl
torii target show kubectl lab
```

`target add` executa `kubectl config get-contexts <context> -o name` e só grava o target quando o context existe. O provider indicado também precisa estar instalado e não pode exigir target. Reinicie o servidor MCP depois de adicionar ou remover targets, pois o registry e o enum do schema são construídos no startup.

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

Depois que o Jasper permite a chamada, o Torii executa o lifecycle de autenticação do provider referenciado pelo target. Se a sessão precisar ser coletada ou renovada, a interface e o validator são os desse provider. Após sucesso, somente o ambiente necessário é sobreposto ao processo `kubectl`.

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

Grants temporários e `.env` ficam isolados no diretório do target. O `.env` do target sobrepõe chaves do `.env` compartilhado. `.session-cache`, credenciais e lock pertencem ao provider autenticador referenciado pelo target e podem ser compartilhados por outros targets que usem o mesmo provider.

## RBAC continua obrigatório

Jasper só decide se a tentativa atravessa. A identidade selecionada no kubeconfig deve ter RBAC de menor privilégio. Uma política local permissiva não amplia RBAC; um RBAC permissivo também não substitui a política local.

Todo target exige `provider`. `torii reauth kubectl lab` delega para o lifecycle desse provider. Um provider `inherited` sem validator passa por esse lifecycle como `session-unchecked`.
