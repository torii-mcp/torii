# Configurar AWS

O pacote canônico de desenvolvimento está em `examples/providers/aws/`. Este guia cobre credenciais temporárias coletadas e validadas pelo Torii. Para profiles locais com aliases humanos e verificação de conta, veja [AWS por profile e aliases](aws-profiles.md). O lease de target não altera este provider simples: a tool `aws` continua recebendo somente `args` e não exige alias.

Instale e, opcionalmente, aplique o setup curado:

```powershell
torii provider install ./examples/providers/aws
torii provider setup aws readonly
```

Install recusa `providers/aws/` existente. Setup recusa uma política ativa não vazia.

## Provider

Os pontos essenciais são:

```yaml
tool: aws
command: aws

policy:
  minimum_accept_tokens: 2

auth:
  strategy: environment
  validate:
    command: aws
    args: [sts, get-caller-identity]
```

Os três campos temporários são declarados e injetados como:

```text
AWS_ACCESS_KEY_ID
AWS_SECRET_ACCESS_KEY
AWS_SESSION_TOKEN
```

## Região e formato de saída

Use o `.env` do provider para valores persistentes não secretos:

```env
AWS_REGION="sa-east-1"
AWS_DEFAULT_REGION="sa-east-1"
AWS_PAGER=""
```

Evitar pager é importante porque stdin do processo filho é nulo.

## Política inicial curada

O pacote instala uma política vazia. A allowlist portada do AWS Gate só entra em vigor com `provider setup aws readonly`; ela inclui operações comuns de STS, EC2, Lambda, Step Functions, CloudWatch Logs, ECS, ECR, Secrets Manager, Glue, EKS e SSM. Operações que retornam valores de segredo, credenciais, tokens ou execução remota possuem `deny` explícito.

Mesmo sem operações de escrita, há riscos que precisam de revisão humana:

- configurações Lambda podem conter variáveis sensíveis;
- parâmetros SSM podem conter segredos dependendo da convenção da organização;
- uma operação read-only ainda pode revelar dados confidenciais.

O template é um ponto de partida útil, não uma política universal pronta para produção. Remova accepts incompatíveis com sua classificação de dados e mantenha IAM de menor privilégio.

## Renovação

```powershell
torii reauth aws
```

Cole atribuições copiadas do portal ou preencha os campos. Torii testa a identidade antes de substituir a sessão. O arquivo antigo permanece se o usuário cancelar ou se `sts get-caller-identity` falhar.
