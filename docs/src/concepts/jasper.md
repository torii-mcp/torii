# Jasper e políticas

Jasper é o núcleo de decisão do Torii. Ele conhece argumentos e arquivos de política; não executa processos, não carrega credenciais e não implementa a gramática dos CLIs.

## Matching por prefixo de tokens

Uma regra é dividida por whitespace. Ela casa quando seus tokens são o prefixo dos itens de `args`:

| Regra | Args | Resultado |
|---|---|---|
| `s3 ls` | `s3`, `ls` | casa |
| `s3 cp` | `s3`, `cp`, `a`, `b` | casa |
| `s3 ls` | `s3api`, `list-buckets` | não casa |
| `ec2 describe` | `ec2`, `describe-instances` | não casa |

O algoritmo compara tokens inteiros. Não há prefixo textual parcial.

Uma regra delimitada por `/…/flags` é avaliada como regex sobre o argv inteiro e casa em qualquer posição, para inspecionar conteúdo (ex.: palavras destrutivas numa query inline). Antes das regras, `forbidden_args` nega argumentos que abrem canais não inspecionáveis (arquivo, stdin) e `ignore_args` remove ruído de formatação da avaliação. Veja [Escrever políticas](../guides/policies.md) e o [schema de provider](../reference/provider-schema.md).

## Camadas em provider target-aware

Um provider target-aware tem o `rules.yaml` compartilhado e, opcionalmente, um `targets/<alias>/rules.yaml`. Os dois vetores não compõem da mesma forma, porque não significam a mesma coisa:

| Vetor | Composição | Por quê |
|---|---|---|
| `deny` | compartilhado **+** do target | Um deny da raiz é o piso do provider. Criar a política de um alias não pode ser o caminho para sair dele. |
| `accept` | do target **substitui** o compartilhado | Um target existe para ter outra superfície de permissão. Um accept da raiz não deve vazar para um alias que declarou a sua. |

Sem `targets/<alias>/rules.yaml`, o alias usa a política compartilhada inteira, deny e accept. No instante em que o arquivo passa a existir, o alias **perde todos os accepts compartilhados** e precisa relistar o que usa: criar a política de um target é restritivo por padrão.

Um deny compartilhado é inescapável por desenho — não existe campo para isentá-lo num target. Para um veto que valha só em alguns aliases, a regra desce para o `deny` de cada um deles; para abrir uma exceção num alias, o accept correspondente vai na política daquele alias e a regra não entra na raiz.

```yaml
# providers/kubectl/rules.yaml          (raiz: piso de deny + accepts padrão)
deny:   ['delete']
accept: ['get pods']

# providers/kubectl/targets/prd/rules.yaml
deny:   ['exec']               # soma ao 'delete' da raiz
accept: ['get pods']           # relistado; sem isto, prd não teria accept algum

# efetivo em prd → deny: ['delete', 'exec']   accept: ['get pods']
```

`torii policy show <tool> <target>` imprime os dois arquivos e o conjunto efetivo, e `torii_policy` devolve ao agente o efetivo, não um dos arquivos.

## Largura mínima de accept

Cada provider define `minimum_accept_tokens`. AWS usa `2`, impedindo um accept amplo como `s3`. Kubernetes usa `1`, permitindo verbos como `logs`.

Accepts abaixo do mínimo são ignorados e registrados como `invalid-accept`. Denies não possuem largura mínima, pois bloquear de forma ampla é seguro.

## Grants

Uma chamada não resolvida pode receber permissão temporária. O operador escolhe o escopo na janela de autorização:

- `exact`: exige o mesmo vetor de argumentos, inclusive tamanho e ordem;
- `prefix`: exige somente os primeiros `N` argumentos escolhidos. Os argumentos posteriores podem mudar, desaparecer ou ser acrescentados.

O Torii mostra os argumentos como tokens e explica literalmente o alcance antes da confirmação. O provider não infere verbo, recurso ou operação.

Em uma tool target-aware, Jasper só chega a grants depois de o dispatcher confirmar o lease humano do alias. O lease não aparece como regra Jasper e não transforma uma operação em permitida; ele apenas libera a escolha do binding antes da política.

O arquivo `grants` usa a versão `2` e guarda somente um fingerprint tokenizado do matcher, nunca uma linha de comando reconstruída:

```yaml
version: "2"
entries:
  - expires_at: 1784000000
    matcher:
      mode: prefix
      token_count: 2
      sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

Entradas expiradas, malformadas ou de versão desconhecida não autorizam chamadas. O formato legado, que achata argumentos em texto, também é ignorado e exige nova aprovação. Grants nunca alteram `rules.yaml`.

## Decisão explicável

Toda resposta identifica a origem: `rules`, `grant`, `human-once`, `human-grant`, `human-deny`, `human-permanent-allow`, `human-permanent-deny` ou `explicit-deny`. Isso permite ao agente compreender por que uma tentativa atravessou ou parou sem expor autenticação.
