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
torii target add kubectl mpce_dev --context eks-mpce-dev
torii target list kubectl
torii target show kubectl mpce_dev
```

`target add` executa `kubectl config get-contexts <context> -o name` e só grava o target quando o context existe. Reinicie o servidor MCP depois de adicionar ou remover targets, pois o registry e o enum do schema são construídos no startup.

Uma chamada do agente:

```json
{
  "target": "mpce_dev",
  "args": ["get", "pods", "-n", "agente-rm"]
}
```

vira conceitualmente:

```text
kubectl --context eks-mpce-dev get pods -n agente-rm
```

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

Grants temporários, `.session-cache`, `.env` e `auth/` ficam isolados no diretório do target. O `.env` do target sobrepõe chaves do `.env` compartilhado.

## RBAC continua obrigatório

Jasper só decide se a tentativa atravessa. A identidade selecionada no kubeconfig deve ter RBAC de menor privilégio. Uma política local permissiva não amplia RBAC; um RBAC permissivo também não substitui a política local.

O exemplo usa autenticação `inherited`. Por isso `torii reauth kubectl mpce_dev` informa que essa estratégia não pode ser renovada pelo Torii.
