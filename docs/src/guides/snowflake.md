# Configurar Snowflake

O pacote canônico é `snow`; a fixture equivalente está em `examples/providers/snow/`. A tool
MCP é `snow`, recebe somente `args` e não é target-aware.

```powershell
torii provider install snow
torii provider setup snow readonly
```

Este provider é o exemplo de referência para **inspeção de conteúdo**: o risco não está no
subcomando, está na query SQL que viaja como argumento.

## Sessão herdada da connection

```yaml
tool: snow
command: snow

auth:
  strategy: inherited
  cache_ttl_seconds: 300
```

A connection default vem de `~/.snowflake/config.toml`. O Torii herda essa sessão, não
coleta credencial e não a renova: `torii reauth snow` não tem material para trocar. Sem
validator declarado, a auditoria registra `session-unchecked`.

## As três camadas da política

```yaml
policy:
  minimum_accept_tokens: 1
  forbidden_args:
    - "-f"
    - "--filename"
    - "-i"
    - "--stdin"
  ignore_args:
    leading: 0
    flags: ["--format", "-o", "--output", "-x"]
```

**1. `forbidden_args` fecha os canais não inspecionáveis.** Uma query lida de arquivo ou de
stdin não está no argv, então nenhuma regra de conteúdo a enxerga. Sem esse bloco, as regras
de conteúdo abaixo seriam decoração: bastaria `snow sql -f drop.sql`. Um argumento proibido é
negado com fonte `forbidden-arg`, em qualquer posição, antes de qualquer regra ser avaliada.
O casamento aceita `--filename` e `--filename=valor`.

**2. `ignore_args` remove ruído da avaliação.** Flags de formatação são descartadas **apenas
para avaliar** a política, nunca do comando executado. Isso evita que um valor de `--format`
acione um match por engano. Uma flag nua também descarta o token de valor seguinte
(`--format json`); a forma `--format=json` descarta o token único.

**3. As regras inspecionam a query.** O setup `readonly` usa regex, que casa em qualquer
posição do argv, e libera SQL inline:

```yaml
deny:
  - "/\\btruncate\\b/i"
  - "/\\bdrop\\b/i"
  - "/\\bdelete\\b/i"
  - "/\\bupdate\\b/i"
  - "/\\binsert\\b/i"
  - "/\\bmerge\\b/i"
  - "/\\balter\\b/i"
  - "/\\bcreate\\b/i"
  - "/\\bgrant\\b/i"
  - "/\\brevoke\\b/i"
  - "/execute\\s+immediate/i"
  - "/copy\\s+into/i"
  - "/\\bput\\b/i"
accept:
  - "sql -q"
  - "sql --query"
```

Como deny vence accept, `snow sql -q "select 1; truncate t"` é barrado mesmo casando o
accept — é justamente o caso que o matching por prefixo sozinho não pegaria.

## O limite honesto do regex

Regex sobre SQL é **best-effort**, não parser. Escapam dele, entre outros:

- concatenação dinâmica e SQL montado em stored procedure;
- falsos positivos quando a palavra aparece isolada em contexto inofensivo: dentro de uma
  string literal, de um comentário SQL ou de um identificador citado como `"DROP"`
  (identificadores comuns como `updated_at` não casam, porque `_` conta como caractere de
  palavra e não há fronteira `\b` ali);
- variações que a lista não previu.

Por isso a fronteira dura é um **role read-only no próprio Snowflake**. As regras locais são
defense-in-depth, auditoria e experiência de uso: elas transformam um erro do agente em
negação explícita e barata, em vez de um erro remoto. Um regex inválido faz a avaliação
falhar fechada (erro, nunca allow silencioso), então cubra suas regras com um teste — veja
`tests/example_policies.rs`.

## Fixe a connection antes de liberar o agente

O accept `sql -q` casa por prefixo, então argumentos posteriores continuam livres. Se a sua
instalação usa mais de uma connection, conta ou role, o agente poderia acrescentar uma flag
de conexão à mesma query permitida. Recomendações, em ordem de eficácia:

1. deixe no `config.toml` apenas a connection que o agente pode usar, com um role read-only;
2. acrescente ao `provider.yaml` as flags de conexão ao `forbidden_args`, por exemplo
   `--connection`, `-c`, `--account`, `--user`, `--role`, `--warehouse`, `--database`,
   `--private-key-file` e `--temporary-connection`, para que uma troca de identidade seja
   negada antes de qualquer regra;
3. nunca dependa apenas das regras de conteúdo para decidir **onde** a query roda.

Este provider não é target-aware: ao contrário de `kubectl` e `aws_profile`, o Torii não
injeta nem bloqueia binding de conexão por conta própria aqui.

## Ambiente

```env
SNOWFLAKE_DEFAULT_CONNECTION_NAME="leitura"
```

Não coloque credenciais no `.env`: a connection e suas chaves vivem no `config.toml` do
Snowflake CLI. Como o stdin do processo filho é nulo, evite comandos que abram prompt.
