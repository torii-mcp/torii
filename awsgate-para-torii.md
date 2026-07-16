seguinte, o awsgate está virando o torii...

> **Decisão arquitetural posterior (julho de 2026):** o contrato Kubernetes foi consolidado em uma única tool `kubectl` com `target` obrigatório. Cada alias é criado pelo control plane humano, resolve para um context fixo e isola grants/cache/auth. Esta decisão substitui, para Kubernetes, as seções antigas deste documento que recomendam uma tool por ambiente ou deixam targets fora do escopo. A documentação oficial em `docs/` descreve o contrato vigente.

# Plano de transformação do AWS Gate em Torii

## 1. A decisão central

O AWS Gate nasceu como um wrapper de linha de comando para humanos e agentes. O Torii será outra coisa:

> **Torii é uma interface MCP de execução controlada para agentes.**

Um humano continuará usando diretamente as ferramentas normais:

```text
Humano → aws / kubectl / az / gcloud
```

Um agente não receberá essas ferramentas diretamente. Ele usará:

```text
Agente → MCP → Torii → Jasper → provider → executável real
```

O Torii não deve virar um novo AWS CLI, um novo kubectl ou um CLI multicloud. Também não deve obrigar o agente a usar uma interface desenhada para humanos apenas porque os executáveis usados por baixo são CLIs.

Os CLIs continuam existindo **como drivers maduros de execução**, escondidos atrás do Torii:

- `aws` continua entendendo AWS;
- `kubectl` continua entendendo Kubernetes;
- `az` continua entendendo Azure;
- `gcloud` continua entendendo Google Cloud.

O Torii só precisa decidir se a ação pode atravessar e, quando puder, iniciar o executável correto com os argumentos e a sessão correta.

---

## 2. O que deve continuar simples

O projeto deve preservar a principal qualidade do AWS Gate atual: ser pequeno, previsível e explicável em poucos minutos.

Princípios obrigatórios:

1. **Default deny.** O que não foi explicitamente permitido ou temporariamente autorizado é bloqueado.
2. **Deny tem prioridade.** Uma regra de bloqueio vence qualquer `accept` compatível.
3. **Matching por prefixo de tokens.** A semântica atual continua sendo o núcleo do Jasper.
4. **Sem shell.** Nunca reconstruir uma linha para `bash -c`, `cmd /c` ou equivalente.
5. **Argumentos estruturados.** O MCP envia `string[]`; o Torii usa `Command::args`.
6. **Uma tool por provider instalado, não uma tool por operação.**
7. **O provider não cadastra todas as ações existentes.** A whitelist descreve somente o que é permitido.
8. **O executável real continua validando sua própria gramática.**
9. **Credenciais só são carregadas depois da autorização.**
10. **Credenciais nunca entram no ambiente global do servidor.**
11. **Autenticação temporária é uma capacidade genérica do Torii, não uma exceção AWS.**
12. **A interface humana existe apenas para controle, aprovação e autenticação.**
13. **O agente não instala providers, não altera políticas e não controla o lifecycle do servidor.**
14. **Só criar uma nova abstração depois que pelo menos dois providers provarem que ela é necessária.**

---

## 3. Como o AWS Gate funciona hoje

O fluxo atual é aproximadamente:

```text
awsgate s3 cp origem destino
            │
            ▼
carrega rules.yaml
            │
            ▼
matching por prefixo de tokens
            │
      ┌─────┴──────────┐
     deny           allow/unresolved
      │                  │
   bloqueia       grant ou janela humana
                         │
                         ▼
                  carrega .env
                  carrega auth.env
                         │
                         ▼
              aws sts get-caller-identity
                         │
              inválida? abre janela
                         │
                         ▼
            aws s3 cp origem destino
```

O código existente já contém quase todas as peças necessárias:

- `src/config/rules.rs`: núcleo do futuro Jasper;
- `src/config/grants.rs`: grants temporários;
- `src/commands/proxy.rs`: coordenação do fluxo;
- `src/session.rs`: preflight e cache de sessão;
- `src/exec.rs`: execução do processo real;
- `src/prompt/gui.rs`: aprovação e autenticação;
- `src/prompt/clipboard.rs`: importação das credenciais copiadas do portal AWS;
- `src/audit.rs`: auditoria sem segredos.

A transformação correta é **extrair os acoplamentos com AWS e trocar a superfície externa por MCP**. Não é reescrever o produto.

---

## 4. Arquitetura mínima do Torii

```text
Cliente MCP
    │
    │ tools/call: aws
    │ args: ["s3", "ls"]
    ▼
Torii MCP Server
    │
    ├── localiza o provider
    ├── valida o envelope MCP
    ├── recebe argv estruturado
    │
    ▼
Jasper
    │
    ├── deny explícito
    ├── accept
    ├── grant temporário
    └── aprovação humana quando não resolvido
    │
    ▼
Sessão de autenticação do provider
    │
    ├── valida sessão/cache
    └── pede reautenticação humana quando necessário
    │
    ▼
Runner
    │
    ├── monta env somente para o processo filho
    ├── adiciona args fixos do provider
    └── inicia o executável sem shell
    │
    ▼
aws / kubectl / az / gcloud
    │
    ▼
stdout + stderr + exit code
    │
    ▼
resultado MCP estruturado
```

### Responsabilidades do Torii

- hospedar o servidor MCP local por `stdio`;
- carregar providers configurados;
- registrar uma tool MCP para cada provider;
- receber `argv` como array de strings;
- chamar o Jasper antes de carregar credenciais;
- coordenar aprovação humana e autenticação;
- preparar o ambiente isolado da execução;
- iniciar o processo filho;
- capturar saída, erro e exit code;
- manter auditoria;
- encerrar quando o cliente MCP encerrar o processo ou fechar o transporte.

### Responsabilidades do Jasper

- carregar `rules.yaml`;
- aplicar prioridade de `deny`;
- validar regras de `accept`;
- fazer matching por prefixo de tokens;
- consultar e gravar grants temporários;
- retornar `allow`, `deny` ou `unresolved`;
- explicar qual regra decidiu;
- nunca executar processos;
- nunca carregar material de autenticação.

### Responsabilidades do provider

- declarar o nome da tool MCP;
- declarar o executável real;
- declarar argumentos fixos opcionais;
- declarar parâmetros simples da política;
- declarar como sua sessão de autenticação é criada, aplicada e validada;
- possuir diretório isolado de regras, grants, sessão e configuração.

Todo target declara `provider` para indicar de qual provider instalado herda o lifecycle completo. O registry valida a referência no startup, mas ambiente, cache, credenciais e lifecycle só são acessados depois da autorização do Jasper. O provider indicado executa sua própria estratégia de autenticação, preflight, coleta ou renovação, e o Torii aplica o ambiente resultante somente ao processo filho do provider alvo.

---

## 5. Torii é MCP, não CLI operacional

A forma canônica de iniciar o Torii será o próprio cliente MCP:

```json
{
  "mcpServers": {
    "torii": {
      "command": "torii"
    }
  }
}
```

O processo permanece vivo durante a sessão do cliente MCP. O cliente controla seu lifecycle.

Não implementar uma tool `kill`.

Motivos:

- o agente não deve poder desligar sua própria fronteira de segurança;
- no transporte `stdio`, o cliente já sabe quando iniciar e encerrar o servidor;
- fechar stdin ou terminar o processo é suficiente;
- uma tool de shutdown adiciona capacidade sem benefício operacional real.

### O que o humano ainda faz

O humano continua responsável por:

- editar políticas;
- configurar ou instalar providers;
- aprovar uma ação não resolvida;
- fornecer ou renovar uma sessão de autenticação;
- trocar o ambiente ativo de um provider.

Isso é **control plane local**, não data plane.

O humano não usa Torii para executar:

```text
torii aws s3 ls
torii kubectl get pods
```

Ele usa diretamente:

```text
aws s3 ls
kubectl get pods
```

### Superfície humana mínima

Durante a migração, pode existir uma entrada de gerenciamento como:

```text
torii reauth aws
torii config
```

Esses comandos nunca encaminham operações para providers. Eles apenas abrem a interface local de controle.

Essa pequena superfície de gerenciamento não transforma o Torii num CLI operacional. Quando houver uma tray UI ou control window estável, ela poderá substituir os comandos de gerenciamento sem tocar no MCP.

Não bloquear a primeira migração esperando uma tray UI perfeita.

---

## 6. Uma tool MCP por provider

Não criar uma tool por operação AWS, recurso Kubernetes ou comando Azure.

A lista deve permanecer pequena:

```text
aws
kubectl_dev
az
```

A tool `aws` recebe:

```json
{
  "args": [
    "ec2",
    "describe-instances",
    "--region",
    "sa-east-1"
  ]
}
```

A tool `kubectl_dev` recebe:

```json
{
  "args": [
    "get",
    "pods",
    "-n",
    "agente-rm"
  ]
}
```

Schema MCP inicial:

```json
{
  "type": "object",
  "required": ["args"],
  "properties": {
    "args": {
      "type": "array",
      "items": { "type": "string" },
      "minItems": 1
    }
  },
  "additionalProperties": false
}
```

Não usar um campo `command` com uma string completa.

O Torii deve preservar o vetor de argumentos até o processo filho:

```rust
Command::new(program).args(args)
```

Com isso:

- nenhuma operação AWS precisa ser cadastrada;
- o Torii não precisa acompanhar novos serviços da AWS;
- o `kubectl` continua usando discovery normalmente;
- o Jasper continua pequeno;
- a whitelist contém apenas o subconjunto permitido.

---

## 7. Provider como configuração pequena

Na primeira versão, provider não é WASM, plugin dinâmico, servidor MCP separado nem pacote OCI.

É um diretório configurado, carregado pelo Torii no startup.

Exemplo AWS:

```yaml
version: "1"

name: aws
tool: aws
description: Executa AWS CLI através do Torii.

command: aws
args_prefix: []

policy:
  minimum_accept_tokens: 2

auth:
  strategy: environment

  fields:
    - name: AWS_ACCESS_KEY_ID
      label: Access key ID
      secret: false
      required: true

    - name: AWS_SECRET_ACCESS_KEY
      label: Secret access key
      secret: true
      required: true

    - name: AWS_SESSION_TOKEN
      label: Session token
      secret: true
      required: true

  clipboard:
    parser: env_assignments

  inject:
    environment:
      AWS_ACCESS_KEY_ID: "${AWS_ACCESS_KEY_ID}"
      AWS_SECRET_ACCESS_KEY: "${AWS_SECRET_ACCESS_KEY}"
      AWS_SESSION_TOKEN: "${AWS_SESSION_TOKEN}"

  validate:
    command: aws
    args:
      - sts
      - get-caller-identity

  cache_ttl_seconds: 300

environment:
  file: .env
```

Exemplo Kubernetes com contexto fixo:

```yaml
version: "1"

name: kubectl-dev
tool: kubectl_dev
description: Executa kubectl no cluster de desenvolvimento.

command: kubectl
args_prefix:
  - --context
  - eks-mpce-dev

policy:
  minimum_accept_tokens: 1

auth:
  strategy: inherited

environment:
  file: .env
```

O YAML não descreve `get pods`, `s3 cp`, `az vm start` ou qualquer outra ação. As ações aparecem somente em `rules.yaml` quando forem permitidas ou bloqueadas.

---

## 8. Autenticação temporária deve ser genérica

A autenticação temporária não será chamada de `aws_temporary` no núcleo do Torii.

O Torii deve possuir um **lifecycle genérico de sessão por provider**:

```text
sem sessão / sessão expirada
          │
          ▼
coleta humana de credenciais ou login interativo
          │
          ▼
materializa credenciais no escopo do provider
          │
          ▼
opcionalmente ativa/login no CLI
          │
          ▼
executa comando de validação
          │
      ┌───┴────┐
   inválida   válida
      │          │
   descarta    persiste sessão
                 │
                 ▼
          permite a execução
```

O provider declara os detalhes. O Torii fornece o mecanismo comum:

- formulário dinâmico baseado em `fields`;
- campos secretos e não secretos;
- textarea para credenciais longas;
- importação do clipboard;
- armazenamento isolado por provider;
- injeção por ambiente;
- materialização de arquivo, quando necessária;
- comando opcional de ativação/login;
- comando de validação;
- cache curto de sessão válida;
- lock de reautenticação por provider;
- troca manual da sessão ativa.

### Nem todo CLI usa credenciais da mesma forma

Não assumir que todos os providers consomem diretamente uma lista de variáveis de ambiente.

Existem três estratégias simples que cobrem os casos esperados:

#### 1. `environment`

O provider coleta campos e os injeta apenas no processo filho.

É o caso natural da sessão temporária AWS:

```text
AWS_ACCESS_KEY_ID
AWS_SECRET_ACCESS_KEY
AWS_SESSION_TOKEN
```

#### 2. `session_command`

O provider executa um comando de login ou autenticação e deixa o CLI manter sua sessão num diretório isolado daquele provider.

Esse modelo é adequado para CLIs que possuem seu próprio login e credential store.

O provider pode definir variáveis como:

```text
AZURE_CONFIG_DIR
CLOUDSDK_CONFIG
```

para impedir que uma instância do Torii contamine ou reutilize acidentalmente a sessão humana global.

#### 3. `credential_file`

O provider coleta um documento ou segredo, grava em arquivo privado no diretório de autenticação e:

- fornece seu caminho por variável de ambiente; ou
- passa o arquivo a um comando de ativação.

Essa estratégia atende credenciais JSON, certificados e formatos semelhantes.

### Escopo da primeira implementação

Implementar completamente `environment`, pois ele preserva o AWS Gate atual.

Desenhar a interface para `session_command` e `credential_file`, mas só implementar cada uma quando o primeiro provider real precisar dela.

Não criar uma linguagem genérica de workflow de autenticação antes disso.

---

## 9. Importação genérica do clipboard

O parser atual de AWS já reconhece formatos como:

```text
export KEY=value
SET KEY=value
$Env:KEY="value"
KEY=value
```

Ele deve deixar de ser um parser de três nomes AWS e virar um parser genérico de **atribuições de ambiente**.

O provider declara a allowlist de campos aceitos:

```yaml
auth:
  fields:
    - name: AWS_ACCESS_KEY_ID
    - name: AWS_SECRET_ACCESS_KEY
    - name: AWS_SESSION_TOKEN
```

O parser:

1. extrai pares `KEY=VALUE` do clipboard;
2. normaliza os prefixos suportados;
3. mantém apenas chaves declaradas pelo provider;
4. preenche a janela;
5. nunca grava antes da validação;
6. nunca registra valores em log.

Com isso, a mesma janela e o mesmo botão **Colar do clipboard** podem servir a qualquer provider que forneça credenciais como variáveis.

Providers com outro formato podem usar textarea ou fluxo interativo específico, sem alterar o Jasper ou o runner.

---

## 10. Troca de ambiente e `reauth`

O comportamento atual de `reauth` é importante e deve sobreviver.

Na primeira versão, cada instância de provider possui **uma sessão ativa por vez**.

Exemplo:

```text
provider aws
sessão atual: conta/ambiente A

reauth

provider aws
sessão atual: conta/ambiente B
```

A troca deve:

1. abrir a janela do provider;
2. coletar ou iniciar nova autenticação;
3. validar a nova sessão;
4. somente após sucesso substituir a sessão anterior;
5. invalidar o cache de preflight;
6. preservar a sessão antiga se a nova autenticação falhar ou for cancelada.

### Ambientes simultâneos

Não criar `target` agora.

Quando houver necessidade de manter dois ambientes simultaneamente, usar duas instâncias simples:

```text
aws_mpce_hml
aws_mpce_prd
kubectl_dev
kubectl_hml
```

Cada instância possui:

- seu `provider.yaml`;
- sua tool MCP;
- seu `rules.yaml`;
- seu material de autenticação;
- seu cache;
- seus grants.

Se a duplicação se tornar um problema real, extrair `target` depois. Não começar por ele.

### Como o humano aciona `reauth`

Ordem recomendada:

1. prompt automático quando uma chamada autorizada encontra sessão inválida;
2. comando local de gerenciamento `torii reauth <provider>` durante a migração;
3. futura control window/tray UI para troca voluntária de sessão.

Não expor `reauth` como tool MCP normal do agente.

---

## 11. Isolamento de sessão por provider

No AWS Gate atual, injetar env no processo global era tolerável porque o processo era curto.

No Torii, que permanece rodando, isso é proibido:

```rust
// não fazer
std::env::set_var(...);
```

O ambiente deve ser montado para cada processo filho:

```rust
let mut child = Command::new(&provider.command);
child.args(&provider.args_prefix);
child.args(&request.args);
child.envs(&persistent_env);
child.envs(&auth_env);
```

Cada provider deve ter um diretório de autenticação próprio:

```text
providers/aws/auth/
providers/az/auth/
providers/gcloud/auth/
```

O Torii também deve permitir que o provider altere somente para o processo filho variáveis de isolamento, por exemplo o diretório de configuração do CLI.

Benefícios:

- credenciais AWS não chegam ao kubectl;
- duas instâncias AWS podem usar sessões diferentes;
- sessões do Torii não contaminam a sessão humana global;
- chamadas concorrentes não sobrescrevem o ambiente uma da outra;
- o MCP pode ficar vivo por horas sem acumular estado global perigoso.

---

## 12. Concorrência e janela de autenticação

O MCP pode receber chamadas concorrentes. O Torii não deve abrir várias janelas para o mesmo provider.

Usar um lock em memória por instância:

1. a chamada detecta sessão inválida;
2. adquire o lock de autenticação do provider;
3. refaz a validação, pois outra chamada pode já ter renovado a sessão;
4. se ainda inválida, abre a janela;
5. valida e persiste a nova sessão;
6. libera o lock;
7. as chamadas aguardando continuam.

Um `Mutex` assíncrono ou equivalente por provider é suficiente para o servidor local.

Não criar coordenação distribuída.

---

## 13. Armazenamento seguro

O material de autenticação deve permanecer local e separado por provider.

Requisitos mínimos:

- escrever primeiro em arquivo temporário;
- fazer flush;
- renomear atomicamente;
- aplicar permissão restrita quando a plataforma permitir;
- nunca registrar credenciais;
- nunca retornar credenciais pelo MCP;
- nunca incluir segredo em mensagens de erro;
- não carregar o material no caminho bloqueado;
- substituir a sessão anterior somente depois da nova validação;
- apagar arquivos temporários, inclusive em falha.

Para `environment`, o formato atual `KEY="VALUE"` pode continuar.

Para `credential_file`, o provider deve declarar o nome lógico do arquivo, mas o Torii decide o caminho real dentro do diretório privado.

---

## 14. Jasper: extração mínima

Mover sem reescrever a semântica:

```text
src/config/rules.rs  → src/jasper/rules.rs
src/config/grants.rs → src/jasper/grants.rs
```

Entrada inicial:

```rust
pub struct PolicyRequest<'a> {
    pub provider: &'a str,
    pub args: &'a [String],
}
```

Como cada provider possui seu próprio `rules.yaml`, a avaliação pode continuar usando:

```rust
args.join(" ")
```

com matching por tokens, nunca `String::starts_with` cru.

Regras AWS:

```yaml
version: "1.0"

deny:
  - "secretsmanager get-secret-value"
  - "ecs execute-command"

accept:
  - "s3 ls"
  - "s3 cp"
  - "ec2 describe-instances"
```

Regras Kubernetes:

```yaml
version: "1.0"

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

Parâmetros de política que deixam de ser constantes globais:

- `minimum_accept_tokens`;
- modo de derivação do grant temporário.

Exemplo:

- AWS: primeiros dois tokens;
- Kubernetes: comando exato inicialmente.

---

## 15. Resultado MCP

O runner genérico retorna:

```rust
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}
```

Resposta MCP sugerida:

```json
{
  "provider": "aws",
  "decision": {
    "result": "allow",
    "source": "rules",
    "rule": "ec2 describe-instances"
  },
  "execution": {
    "exit_code": 0,
    "stdout": "...",
    "stderr": "...",
    "truncated": false
  }
}
```

Em bloqueio:

```json
{
  "provider": "aws",
  "decision": {
    "result": "deny",
    "source": "explicit-deny",
    "rule": "secretsmanager get-secret-value"
  }
}
```

Não iniciar o provider nem carregar autenticação no caso de deny.

Configurar limite de saída para evitar que um comando enorme ocupe todo o contexto do agente. Truncar explicitamente na primeira versão; streaming pode vir depois se surgir necessidade real.

---

## 16. Provider Kubernetes sem parser completo

Não criar um parser de toda a gramática do kubectl.

Para manter o matching previsível:

- colocar contexto fixo em `args_prefix`;
- recomendar verbo no primeiro token;
- bloquear operações interativas e de escape;
- começar read-only;
- adicionar normalização somente diante de um caso real.

Exemplos recomendados:

```text
get pods -n agente-rm
logs pod-x -n agente-rm
describe deployment api -n agente-rm
```

Evitar inicialmente:

```text
-n agente-rm get pods
```

O RBAC do cluster permanece como segunda barreira:

```text
Jasper decide se o agente pode tentar.
Kubernetes decide se a identidade executora pode realizar.
```

---

## 17. Estrutura de configuração

```text
~/.config/torii/
├── settings.yaml
├── torii.log
└── providers/
    ├── aws/
    │   ├── provider.yaml
    │   ├── rules.yaml
    │   ├── .env
    │   ├── grants
    │   ├── .session-cache
    │   └── auth/
    │       └── credentials.env
    │
    ├── kubectl-dev/
    │   ├── provider.yaml
    │   ├── rules.yaml
    │   ├── .env
    │   ├── grants
    │   ├── .session-cache
    │   └── auth/
    │
    └── az/
        ├── provider.yaml
        ├── rules.yaml
        ├── .env
        ├── grants
        ├── .session-cache
        └── auth/
```

Arquivos globais:

- `settings.yaml`: limites de saída, preferências da GUI e editor;
- `torii.log`: auditoria central, sem segredos.

Arquivos por provider:

- `provider.yaml`: executável, tool, política e autenticação;
- `rules.yaml`: accept/deny;
- `.env`: ambiente persistente não secreto;
- `grants`: autorizações temporárias;
- `.session-cache`: última validação bem-sucedida;
- `auth/`: sessão e artefatos privados.

---

## 18. Estrutura sugerida do código

```text
src/
├── main.rs
├── app.rs
│
├── mcp/
│   ├── mod.rs
│   ├── server.rs
│   ├── lifecycle.rs
│   └── tools.rs
│
├── core/
│   ├── mod.rs
│   └── invoke.rs
│
├── jasper/
│   ├── mod.rs
│   ├── rules.rs
│   └── grants.rs
│
├── providers/
│   ├── mod.rs
│   ├── config.rs
│   ├── registry.rs
│   └── auth/
│       ├── mod.rs
│       ├── session.rs
│       ├── environment.rs
│       ├── session_command.rs
│       └── credential_file.rs
│
├── runtime/
│   ├── mod.rs
│   └── exec.rs
│
├── control/
│   ├── mod.rs
│   ├── approval.rs
│   ├── reauth.rs
│   ├── gui.rs
│   └── clipboard.rs
│
├── config/
│   ├── mod.rs
│   ├── env_file.rs
│   └── settings.rs
│
├── audit.rs
├── editor.rs
└── error.rs
```

Essa estrutura é destino, não obrigação de um único refactor.

`session_command.rs` e `credential_file.rs` podem começar somente com tipos e erros `not implemented` até aparecer o provider que precise deles.

---

## 19. Ordem recomendada de implementação

### Fase 0 — congelar o AWS Gate atual

Antes da mudança:

- executar todos os testes;
- completar testes de:
  - `--reauth`;
  - sessão expirada;
  - cancelamento da GUI;
  - clipboard AWS;
  - deny antes de carregar credenciais;
  - deny antes de iniciar `aws`;
  - preservação do exit code;
- criar tag ou branch de referência.

### Fase 1 — separar Jasper e runner sem mudar comportamento

- mover rules e grants para `jasper`;
- substituir `run_aws` por runner genérico;
- remover `AWS_BIN` hardcoded;
- introduzir configuração de provider AWS;
- continuar oferecendo o comportamento legado somente para validar equivalência.

Aceitação: os testes antigos continuam passando com a mesma semântica.

### Fase 2 — tornar autenticação uma sessão genérica

- criar `AuthSession` e `AuthStrategy`;
- implementar `environment`;
- transformar o parser do clipboard em `env_assignments` genérico;
- gerar a janela pelos `fields` do provider;
- mover preflight para `auth.validate`;
- usar storage e cache por provider;
- injetar credenciais apenas no processo filho;
- adicionar lock de reauth por provider.

Aceitação: o fluxo AWS temporário continua idêntico para o usuário, mas não existe código central dependente dos três nomes AWS.

### Fase 3 — transformar a superfície externa em MCP

- adicionar MCP `stdio`;
- iniciar o servidor ao executar `torii` sem subcomandos;
- registrar uma tool por provider;
- usar schema `{ args: string[] }`;
- chamar o mesmo `core::invoke`;
- capturar stdout/stderr/exit code;
- encerrar quando o cliente encerrar o transporte;
- não implementar tool de kill;
- não expor config, provider install ou reauth ao agente.

Aceitação: um cliente MCP consegue chamar a tool `aws` e passar pelo mesmo Jasper, sessão e auditoria.

### Fase 4 — renomear e migrar configuração

- renomear crate, binário, textos e variáveis para Torii;
- trocar `~/.config/.awsgate` por `~/.config/torii`;
- manter fallback temporário:
  - `AWSGATE_CONFIG_DIR` → `TORII_CONFIG_DIR`;
  - `AWSGATE_NO_GUI` → `TORII_NO_GUI`;
- migrar a configuração antiga para `providers/aws`;
- atualizar títulos das janelas;
- preservar o `reauth` como ação de controle.

### Fase 5 — adicionar Kubernetes

- criar provider `kubectl-dev`;
- configurar contexto por `args_prefix`;
- começar com `auth.strategy: inherited`;
- criar política read-only;
- bloquear exec/attach/port-forward/proxy/config;
- testar allow, deny, unresolved, grants e erro do CLI.

Aceitação: o agente usa Kubernetes somente pela tool MCP instalada.

### Fase 6 — provar o segundo modelo de autenticação

Escolher Azure ou GCP e implementar somente a estratégia que o caso real exigir:

- `session_command`; ou
- `credential_file`.

Não implementar as duas por antecipação.

A partir desse caso, revisar se a configuração genérica de autenticação permaneceu simples.

### Fase 7 — instalação de providers

Somente após AWS e Kubernetes estabilizarem:

```text
torii provider install ./provider-directory
torii provider list
torii provider remove kubectl-dev
```

Esses são comandos de control plane. Nunca executam ações de nuvem.

A instalação inicial pode apenas copiar e validar um diretório. Não implementar registry remoto, atualização automática, assinatura ou OCI sem necessidade concreta.

---

## 20. Migração da configuração existente

No primeiro uso:

1. verificar se `~/.config/torii/providers/aws` existe;
2. se não existir, procurar `~/.config/.awsgate`;
3. criar `~/.config/torii/providers/aws`;
4. migrar:
   - `rules.yaml`;
   - `.env`;
   - `auth.env` para `auth/credentials.env`;
   - `grants`;
   - `.session-cache`;
5. criar o `provider.yaml` AWS padrão;
6. preservar o log antigo;
7. não sobrescrever configuração Torii existente;
8. mostrar claramente o que foi migrado;
9. validar a sessão migrada antes de considerá-la ativa.

O parser legado `aws.env` também deve continuar sendo migrado para ambiente persistente + sessão privada.

---

## 21. Mudanças concretas nos arquivos atuais

### `src/commands/proxy.rs`

Transformar em `core/invoke.rs`:

- recebe provider e `args`;
- chama Jasper;
- resolve aprovação/grant;
- solicita sessão válida;
- chama runner;
- retorna resultado estruturado.

### `src/exec.rs`

Transformar em `runtime/exec.rs`:

- remover `AWS_BIN`;
- remover `run_aws`;
- implementar `run_command`;
- remover injeção no ambiente global;
- aceitar env por processo;
- capturar stdout/stderr;
- preservar exit code.

### `src/session.rs`

Transformar no núcleo de `providers/auth/session.rs`:

- estado por provider;
- validação configurada;
- cache por provider;
- lock de renovação;
- substituição atômica de sessão.

A parte especificamente AWS deve ficar apenas no `provider.yaml` e, se ainda necessário, em testes/fixtures AWS.

### `src/config/rules.rs`

Mover para `jasper/rules.rs` e receber os parâmetros de política do provider.

### `src/config/grants.rs`

Mover para `jasper/grants.rs` e generalizar a derivação da regra.

### `src/prompt/gui.rs`

Mover para `control/gui.rs` e preservar:

- janela de aprovação;
- janela de autenticação;
- indicação do provider;
- validação antes de persistir;
- cancelamento seguro.

A janela de autenticação deve ser gerada pelos campos declarados no provider.

### `src/prompt/clipboard.rs`

Transformar em parser genérico de atribuições de ambiente:

```text
control/clipboard.rs
```

Ele não deve conhecer previamente os três nomes AWS. Deve receber a allowlist de campos do provider.

### `src/cli.rs`

Remover o modo proxy operacional.

Manter, provisoriamente, somente control plane:

```text
torii reauth <provider>
torii config ...
torii provider ...
```

Executar `torii` sem subcomando inicia o MCP por `stdio`.

Não aceitar:

```text
torii aws <args>
torii kubectl <args>
```

### `src/app.rs`

Carregar:

- `ProviderRegistry`;
- servidor MCP;
- control plane local;
- `core::invoke` compartilhado.

---

## 22. O que não implementar agora

- uma tool MCP por ação;
- catálogo completo de ações AWS;
- SDK AWS substituindo o AWS CLI;
- parser completo de kubectl;
- abstração universal de recurso/verbo/scope;
- serviço Jasper separado;
- providers como MCPs separados;
- plugin WASM;
- registry OCI;
- atualização automática;
- target registry;
- OAuth remoto;
- daemon multiusuário;
- policy language nova;
- profiles AWS obrigatórios;
- tool MCP de reauth;
- tool MCP de kill;
- CLI operacional do Torii;
- implementação antecipada de todos os modelos de autenticação.

---

## 23. Critérios de aceite

O AWS Gate pode ser considerado transformado em Torii quando:

1. `torii` inicia um servidor MCP local por `stdio`;
2. o lifecycle do processo é controlado pelo cliente MCP;
3. não existe tool de kill;
4. cada provider instalado aparece como uma única tool;
5. cada tool recebe argumentos como array de strings;
6. não existe uma tool por operação;
7. o provider AWS executa qualquer comando permitido sem cadastro prévio da operação;
8. o provider Kubernetes faz o mesmo com `kubectl`;
9. as regras continuam sendo listas simples de `accept` e `deny`;
10. deny explícito continua vencendo;
11. grants e aprovação humana continuam funcionando;
12. autenticação é modelada como sessão genérica por provider;
13. o fluxo atual de credenciais temporárias AWS continua funcionando;
14. o parser de clipboard usa os campos declarados pelo provider;
15. reauth substitui a sessão ativa somente após validação;
16. é possível manter ambientes simultâneos com instâncias separadas de provider;
17. uma sessão expirada abre apenas uma janela mesmo com concorrência;
18. credenciais são passadas somente ao processo filho autorizado;
19. comandos bloqueados não carregam sessão nem iniciam o executável;
20. sessões do Torii são isoladas das sessões globais humanas sempre que o CLI suportar diretório de configuração próprio;
21. auditoria identifica provider, regra, decisão e exit code sem segredos;
22. o humano não precisa usar Torii para executar operações de nuvem;
23. o projeto continua compreensível numa leitura curta.

---

## 24. Resumo da arquitetura

Torii será:

```text
MCP server local
+ registry pequeno de providers
+ Jasper
+ controle humano local
+ sessões de autenticação por provider
+ runner seguro sem shell
```

Um provider será:

```text
tool MCP
+ executável
+ argumentos fixos opcionais
+ rules.yaml
+ política de grants
+ estratégia de autenticação
+ sessão isolada
```

Jasper será:

```text
matching por tokens
+ deny prioritário
+ default deny
+ grants temporários
+ decisão explicável
```

A frase que deve orientar a implementação é:

> **O AWS Gate já contém o coração do Torii. O trabalho é transformar um wrapper AWS de vida curta numa fronteira MCP de longa duração, sem perder sua simplicidade.**

E a fronteira conceitual final é:

```text
Humano opera com as ferramentas humanas.
Agente opera através do Torii.
Jasper decide o que atravessa.
```

---

## 25. Notas de compatibilidade para implementação

Estas notas existem para evitar uma abstração incorreta de autenticação:

- AWS CLI aceita diretamente credenciais temporárias por `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` e `AWS_SESSION_TOKEN`.
- Azure CLI possui seu próprio fluxo de login e sessão; variáveis `AZURE_*` são comuns nos SDKs, mas não devem ser tratadas automaticamente como substituto universal de `az login`.
- O `gcloud` mantém sua própria conta/sessão ativa. `GOOGLE_APPLICATION_CREDENTIALS` pertence ao modelo ADC usado por bibliotecas e não deve ser confundido com a sessão operacional do próprio `gcloud`.

Portanto, a abstração correta não é “todo provider recebe variáveis de ambiente”. É:

> **Todo provider declara como uma sessão humana temporária é coletada, materializada, validada e aplicada à execução.**
