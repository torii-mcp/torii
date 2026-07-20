# Configurar AWS por profile e aliases

Use este fluxo quando cada conta AWS já é representada por um profile local humano, por exemplo uma sessão SSO. Ele é separado do provider `aws`, que continua sendo o fluxo de credenciais temporárias coletadas pelo Torii.

O pacote cria a tool MCP `aws_profile`:

```powershell
torii provider install ./examples/providers/aws-profile
torii provider setup aws-profile readonly
```

Depois o humano cria um alias. O alias, e não o profile, é o único identificador que o agente recebe:

```powershell
torii target add aws_profile producao `
  --profile empresa-producao `
  --account-id 111122223333 `
  --region sa-east-1
```

Profile, conta esperada e região ficam no bloco `identity` do `target.yaml` local (`identity.profile`, `identity.expect`, e `region`). O escopo de credencial recebe o nome do profile, então aliases do mesmo profile compartilham sessão e profiles distintos ficam isolados. O schema MCP, os resultados e a auditoria mostram somente `producao`.

## Ativar o alias por tempo limitado

Criar o alias não o torna disponível para execução. Antes de deixar um agente usá-lo, o humano concede um lease:

```powershell
torii target activate aws_profile producao --for 30
torii target status aws_profile
```

`--for` aceita de 1 a 1.440 minutos e, se omitido, usa `default_target_minutes` (15). A ativação normal desativa todos os outros aliases ativos de `aws_profile`. Para manter os existentes, use `--add` conscientemente:

```powershell
torii target activate aws_profile homologacao --for 30 --add
```

Com mais de um alias ativo, o agente pode escolher qualquer um deles em operações permitidas. Prefira a substituição normal e políticas/IAM mais restritivos para produção. Para revogar todos os aliases ativos sem mexer em grants, sessão, cache ou configuração, use `torii target clear aws_profile`.

## Binding de conta

Depois de descartar um deny explícito, o dispatcher exige primeiro o lease humano do alias. Só então consulta grants ou autorização Jasper e, após essa decisão, fixa o profile configurado no processo filho, bloqueia `--profile`, `--region`, `--endpoint-url`, `--no-sign-request` e opções TLS equivalentes enviadas pelo agente. Credenciais AWS, região e endpoint herdados do processo servidor são removidos desse filho para não sobrepor o profile.

Antes do comando solicitado, o Torii executa internamente `sts get-caller-identity --output json` com o mesmo binding e compara a conta com os 12 dígitos configurados. A comparação ocorre em toda chamada permitida; divergência, erro de identidade ou saída inválida impede o comando solicitado. Os números de conta não são devolvidos ao agente.

O balde de credencial do alias (`identities/<profile>/`) tem cache, `.identity-cache` e lock próprios. Isso evita compartilhar estado entre aliases de contas diferentes; aliases do mesmo profile reaproveitam o balde de propósito.

## Renovar a sessão correta

Não existe tool MCP de reauth, e `torii reauth aws_profile producao` deliberadamente não tenta alterar a sessão. Um profile pode ser SSO, estático ou usar outro mecanismo externo; o Torii não consegue trocar esse estado de forma atômica e validada.

Quando o alias informar que a identidade está ausente ou pertence a outra conta, o agente deve pedir que um humano autentique o profile configurado pelo fluxo nativo apropriado e então repetir a chamada com o mesmo alias. Um alias sem lease abre a decisão humana de target; em headless, a tentativa é negada. Para um profile SSO, por exemplo:

```powershell
aws sso login --profile empresa-producao
```

Não peça ao agente para escolher outro profile, editar `target.yaml` ou tentar flags de override. Se a conta desejada mudou, o humano revisa ou cria um alias no control plane.
