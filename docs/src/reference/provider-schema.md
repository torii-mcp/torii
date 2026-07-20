# Schema de provider e target

`provider.yaml` usa `version: "1"`.

## Campos de topo

| Campo | Obrigatório | Descrição |
|---|---|---|
| `version` | sim | deve ser `"1"` |
| `name` | sim | identidade lógica única |
| `tool` | sim | nome MCP único; `torii_policy` é reservado pelo Torii |
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

Cada target possui `target.yaml` com `version: "1"`:

```yaml
version: "1"
name: lab
context: local-context
identity:
  provider: aws
  scope: lab
  expect: "111122223333"
```

O `name` deve coincidir com o diretório e o context não pode ser vazio ou conter quebra de linha. O bloco `identity` define a autenticação:

| Campo | Obrigatório | Descrição |
|---|---|---|
| `provider` | sim | tool de um provider instalado e não target-aware que roda o lifecycle de autenticação |
| `scope` | não | balde de credencial em `identities/<scope>`; default = nome do target |
| `profile` | não | profile a injetar (via `auth.profile_env` do provider de identidade) |
| `expect` | não | identidade exigida; conferida pelo probe `auth.identity` do provider antes de cada execução |

O target usa o lifecycle do provider de identidade (estratégia, validação, coleta, renovação), mas isolado no balde `scope`: cache, credenciais e lock ficam em `identities/<scope>/`. Por padrão o escopo é o nome do target, então targets distintos não compartilham sessão; dar o mesmo `scope` a vários targets é a forma explícita de reaproveitar uma sessão. Providers `inherited` sem validator produzem `session-unchecked`.

`expect` só é aceito quando o provider de identidade declara `auth.identity`; caso contrário é erro de configuração. Se o provider indicado não estiver instalado, o Torii rejeita a criação ou o carregamento do target. Não existe fallback para outro provider.

O registry valida a referência durante o startup, mas o Jasper decide antes de o Torii ler ambiente, cache ou credenciais e antes de executar o lifecycle do provider indicado. Somente em uma chamada permitida, esse lifecycle executa sua validação, coleta ou renovação. O `.env` persistente e o ambiente de sessão desse provider são aplicados ao processo filho do provider alvo; não entram no ambiente global do servidor.

Um provider instalado como dependência de autenticação continua publicado como tool MCP nesta versão. Mantenha seu `rules.yaml` em default deny quando o agente não precisar invocá-lo diretamente.

## Targeting AWS por profile

```yaml
targeting:
  mode: aws_profile
```

Esse modo requer `auth.strategy: inherited`. O `target.yaml` fixa um profile humano e a conta esperada no bloco `identity`:

```yaml
version: "1"
name: producao
region: sa-east-1
identity:
  provider: aws_profile
  scope: empresa-producao
  profile: empresa-producao
  expect: "111122223333"
```

`identity.profile` não pode ser vazio ou conter quebra de linha; `identity.expect` deve conter exatamente 12 dígitos ASCII; `region`, quando presente, também não pode ser vazio ou conter quebra de linha. `context` não é aceito nesse modo. `identity.provider` deve ser a própria tool target-aware, como `aws_profile`; ele expressa que o binding autentica pelo próprio lifecycle. O escopo padrão criado por `torii target add --profile` é o nome do profile, de modo que targets do mesmo profile compartilham sessão e profiles distintos ficam isolados.

O Torii injeta `--profile <profile>` (e `AWS_PROFILE`, via `auth.profile_env`) e, quando configurada, `--region <region>`. O agente não pode enviar `--profile`, `--region`, `--endpoint-url`, `--no-sign-request`, `--ca-bundle` ou `--no-verify-ssl`, nas formas separada ou `--opção=valor`. Antes de executar o argumento solicitado, toda chamada permitida roda o probe `auth.identity` (`sts get-caller-identity`, campo `Account`) e exige a conta esperada. Profile, conta e região não entram no schema MCP nem nos resultados; somente o alias aparece.

Cache, lock e diretório `auth/` desse modo pertencem ao balde `identities/<scope>` do provider. `torii reauth` não altera profiles: a autenticação é feita pelo humano com o fluxo nativo do AWS CLI e a chamada é repetida depois.

## Ativação temporária de targets

`target.yaml` configura um alias, mas não o torna utilizável pelo agente. Todo target-aware começa sem lease. O lease é estado operacional por provider, fora do schema do provider e do target, e é criado somente pelo control plane humano:

```text
torii target activate <tool> <name> [--for <minutes>] [--add]
```

O estado vincula o alias a um digest do `target.yaml` e a uma expiração. Portanto, editar um binding invalida autorizações anteriores. A duração é de 1 a 1.440 minutos; sem `--for`, usa `default_target_minutes` (15). A ativação padrão substitui todos os aliases ativos da tool; `--add` conserva os existentes e permite que o agente escolha qualquer alias ativo em operações permitidas. Todos os aliases continuam no enum MCP, ativos ou não. Veja [Layout de configuração](configuration-layout.md) para o arquivo de estado e [CLI](cli.md) para revogação e status.

O lease não é uma regra `accept`, não é grant e não dispensa a autenticação. O dispatcher verifica deny explícito antes de pedir o lease e o verifica novamente antes de ambiente/autenticação e do launch.

## Política

```yaml
policy:
  minimum_accept_tokens: 2
```

`minimum_accept_tokens` vale somente para `accept` em `rules.yaml`. O escopo de um grant temporário é escolhido pelo operador na janela de autorização como invocação exata ou prefixo de tokens; não é derivado automaticamente pelo provider. Em providers target-aware, `targets/<name>/rules.yaml` substitui o `rules.yaml` compartilhado quando existe. Um grant só é consultado depois de o alias possuir lease válido; ele não ativa target algum.

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

Sem `validate`, uma sessão `inherited` é registrada como `session-unchecked` e não recebe cache de validade. Com `validate`, o Torii não consegue renovar a sessão (o login é externo, via SSO/profile): `torii reauth` aponta o humano para o fluxo nativo.

## Verificação de identidade e injeção de profile

Um provider que serve de identidade para targets pode declarar campos extras em `auth`:

```yaml
auth:
  strategy: inherited
  identity:
    command: aws
    args: [sts, get-caller-identity]
    field: Account
    cache_ttl_seconds: 300
  profile_env: AWS_PROFILE
  removed_env:
    - AWS_ACCESS_KEY_ID
    - AWS_SESSION_TOKEN
    - AWS_PROFILE
```

- `auth.identity` é o probe que responde "de quem é esta sessão?". Roda sob o comando do próprio provider (não sob o comando alvo), força saída JSON e lê o campo `field`. Um target com `identity.expect` compara o resultado antes de executar; o resultado é cacheado por escopo em `.identity-cache`. Sem esse probe, `expect` é recusado na configuração.
- `auth.profile_env` é a variável que carrega o `identity.profile` do target — por exemplo `AWS_PROFILE`, para que o plugin de credencial exec do kubectl herde o profile.
- `auth.removed_env` lista variáveis de ambiente que nunca podem vazar do processo servidor para uma invocação autenticada por este provider; as credenciais/profile injetados vencem qualquer valor ambiente.

`environment.file` deve ser relativo e permanecer dentro do diretório do provider.
