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

Toda resposta identifica a origem: `rules`, `grant`, `human-once`, `human-grant`, `human-deny` ou `explicit-deny`. Isso permite ao agente compreender por que uma tentativa atravessou ou parou sem expor autenticação.
