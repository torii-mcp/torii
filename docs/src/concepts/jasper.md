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

Uma chamada não resolvida pode receber permissão temporária. O provider escolhe como derivar a regra:

- `first_tokens`: usa os primeiros `count` itens;
- `exact`: usa toda a chamada.

O arquivo `grants` contém uma entrada por linha:

```text
1784000000	ec2 describe-instances
```

O primeiro campo é o epoch Unix de expiração. Entradas expiradas ou malformadas são ignoradas e grants nunca alteram `rules.yaml`.

## Decisão explicável

Toda resposta identifica a origem: `rules`, `grant`, `human-once`, `human-grant`, `human-deny` ou `explicit-deny`. Isso permite ao agente compreender por que uma tentativa atravessou ou parou sem expor autenticação.

