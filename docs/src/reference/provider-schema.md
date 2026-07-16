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
name: lab
context: local-context
provider: aws
```

O `name` deve coincidir com o diretório e o context não pode ser vazio ou conter quebra de linha. `provider` é obrigatório e contém o nome da tool de um provider instalado e não target-aware. O target herda o lifecycle desse provider, inclusive estratégia, validação, coleta, renovação, cache e lock. Providers `inherited` sem validator também são válidos e produzem `session-unchecked`.

Se o provider indicado não estiver instalado, o Torii rejeita a criação ou o carregamento do target e informa que o provider precisa ser instalado. Não existe fallback para outro provider.

O registry valida a referência durante o startup, mas o Jasper decide antes de o Torii ler ambiente, cache ou credenciais e antes de executar o lifecycle do provider indicado. Somente em uma chamada permitida, esse lifecycle executa sua validação, coleta ou renovação. O `.env` persistente e o ambiente de sessão desse provider são aplicados ao processo filho do provider alvo; não entram no ambiente global do servidor.

Um provider instalado como dependência de autenticação continua publicado como tool MCP nesta versão. Mantenha seu `rules.yaml` em default deny quando o agente não precisar invocá-lo diretamente.

## Política

```yaml
policy:
  minimum_accept_tokens: 2
```

`minimum_accept_tokens` vale somente para `accept` em `rules.yaml`. O escopo de um grant temporário é escolhido pelo operador na janela de autorização como invocação exata ou prefixo de tokens; não é derivado automaticamente pelo provider. Em providers target-aware, `targets/<name>/rules.yaml` substitui o `rules.yaml` compartilhado quando existe.

Pacotes antigos podem conter `policy.grant_rule`. O campo é aceito apenas por compatibilidade e não influencia grants novos.

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

Sem `validate`, uma sessão `inherited` é registrada como `session-unchecked` e não recebe cache de validade.

`environment.file` deve ser relativo e permanecer dentro do diretório do provider.
