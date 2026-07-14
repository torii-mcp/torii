# Escrever políticas

Cada provider possui seu próprio `rules.yaml`. Em provider target-aware, um `rules.yaml` dentro do target substitui a política compartilhada somente naquele alias.

```yaml
version: "1.0"
deny:
  - "secretsmanager get-secret-value"
  - "ecs execute-command"
accept:
  - "s3 ls"
  - "ec2 describe-instances"
```

## Comece pelo mínimo

Adicione somente operações observadas e necessárias. A ausência de uma operação não impede aprovação humana quando a GUI está habilitada, mas em headless ela será negada.

## Use deny para escapes conhecidos

Bloqueie comandos que abrem execução arbitrária, túneis, proxies ou leitura direta de segredos. Deny vence mesmo se uma regra accept mais ampla também casar.

## Posicione flags depois da ação

Jasper avalia prefixos desde o primeiro item. Prefira:

```text
get pods -n equipe
```

Evite:

```text
-n equipe get pods
```

O segundo formato não casa com `get pods` e será não resolvido.

## Escolha grants conscientemente

`first_tokens` é útil quando serviço e ação identificam bem a capacidade. `exact` reduz o alcance do grant, mas inclui todos os argumentos no arquivo de grants e na resposta de decisão. Não passe segredos em argumentos de CLI.

## Teste fronteiras

Ao alterar matching, cubra pelo menos:

- deny e accept para a mesma ação;
- prefixos parecidos como `s3` e `s3api`;
- accept abaixo do mínimo;
- comando não listado;
- grant expirado e ativo.
