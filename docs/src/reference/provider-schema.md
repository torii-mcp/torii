# Schema de provider e target

`provider.yaml` usa `version: "1"`.

## Campos de topo

| Campo | Obrigatório | Descrição |
|---|---|---|
| `version` | sim | deve ser `"1"` |
| `name` | sim | identidade lógica única |
| `tool` | sim | nome MCP único |
| `description` | sim | descrição exposta ao cliente |
| `command` | sim | executável real |
| `args_prefix` | não | argumentos confiáveis antes dos argumentos MCP |
| `targeting` | não | torna a tool target-aware |
| `policy` | não | parâmetros do Jasper |
| `auth` | não | estratégia de sessão; padrão `inherited` |
| `environment` | não | arquivo persistente; padrão `.env` |

## Targeting Kubernetes

```yaml
targeting:
  mode: kubectl_context
  locked_options:
    - --custom-endpoint
```

`kubectl_context` injeta `--context <context>` para o target selecionado. Existe uma baseline não removível de flags bloqueadas:

```text
--context --kubeconfig --cluster --user --token --server
--username --password --client-key --client-certificate
--certificate-authority --insecure-skip-tls-verify --tls-server-name
--as --as-group --as-uid --as-user-extra
```

Tanto `--flag valor` quanto `--flag=valor` são recusados. `locked_options` adiciona flags específicas do provider e cada item deve começar com `--`, sem espaço ou `=`.

Cada target possui `target.yaml`:

```yaml
version: "1"
name: mpce_dev
context: eks-mpce-dev
```

O `name` deve coincidir com o diretório e o context não pode ser vazio ou conter quebra de linha.

## Política

```yaml
policy:
  minimum_accept_tokens: 2
  grant_rule:
    mode: first_tokens
    count: 2
```

`mode` aceita `first_tokens` e `exact`. Em providers target-aware, `targets/<name>/rules.yaml` substitui o `rules.yaml` compartilhado quando existe.

## Autenticação `environment`

```yaml
auth:
  strategy: environment
  fields:
    - name: TOKEN
      label: Session token
      secret: true
      required: true
  inject:
    environment:
      CLI_TOKEN: "${TOKEN}"
  validate:
    command: cli
    args: [whoami]
  cache_ttl_seconds: 300
```

Cada template deve ser exatamente `${NOME}` e referenciar um field declarado. `validate` é opcional, mas recomendado para credenciais coletadas.

## Autenticação `inherited`

```yaml
auth:
  strategy: inherited
  validate:
    command: cli
    args: [account, show]
```

Torii não coleta material nessa estratégia. `session_command` e `credential_file` são reconhecidas e ainda não implementadas.

`environment.file` deve ser relativo e permanecer dentro do diretório do provider.
