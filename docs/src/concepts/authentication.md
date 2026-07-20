# Sessões de autenticação

Autenticação é um lifecycle separado do Jasper. Todo target autentica pelo provider indicado em `identity.provider`, num balde de credencial identificado por `identity.scope` (default: nome do target). Baldes distintos têm sessão, cache e lock independentes; targets só compartilham quando declaram o mesmo escopo. Quando `identity.expect` está presente, o Torii ainda confirma, via probe `auth.identity` do provider, que a sessão carrega a identidade esperada antes de executar.

## Lease de target antes da sessão

Em qualquer tool target-aware, o alias precisa primeiro de um lease humano válido. Esse lease é separado de credenciais, cache e grants de operação: ele apenas permite que o dispatcher comece a usar o binding daquele target. Sem lease, o Torii não lê `.env`, cache ou credenciais e não inicia validator, STS ou processo filho.

O lease expira independentemente da sessão. Ele é reavaliado depois da autorização Jasper, antes da sessão e novamente antes do launch. Portanto, remover o lease bloqueia trabalho pendente, mas não apaga credenciais nem interrompe um comando que já começou.

## `environment`

Implementada. O provider declara campos, templates de injeção e validação. O fluxo é:

1. carregar a sessão existente após a autorização;
2. usar cache curto se houver validação recente;
3. executar o comando de validação;
4. se inválida, abrir a janela gerada pelos campos;
5. manter a mesma janela aberta, bloquear nova submissão e mostrar progresso na barra de status enquanto uma thread de background do subprocesso da GUI executa o validator;
6. persistir atomicamente e atualizar o cache;
7. devolver as variáveis para a execução autorizada.

Campos secretos usam input mascarado. Campos multilinha têm altura limitada e rolagem interna, para que valores longos não desloquem o restante do formulário. O botão de clipboard aceita `export KEY=value`, `SET KEY=value`, `$Env:KEY=value` e `KEY=value`, mantendo somente nomes declarados. A barra de status ocupa uma altura fixa e toda a largura no limite inferior da janela, abaixo das ações alinhadas à direita, e alterna entre pronto, progresso, erro e sucesso sem deslocar o layout. Uma candidata recusada reabilita o mesmo formulário sem fechar ou recriar a janela; uma candidata aceita mostra brevemente `👍 Sessão validada.` antes do fechamento automático.

## `inherited`

Implementada. Torii não coleta material. Se `validate` existir, ele roda com `.env` e o ambiente herdado. Sem `validate`, o lifecycle registra `session-unchecked` e não cria cache de validade. `torii reauth` não se aplica a essa estratégia.

Use `.env` para apontar stores isolados do provider quando disponível, por exemplo:

```env
AZURE_CONFIG_DIR="C:/Users/voce/.config/torii/providers/az/auth/azure"
```

## Profile AWS com conta vinculada

`aws_profile` também usa `inherited`, mas não trata o ambiente inteiro do servidor como identidade. Depois de lease válido e autorização da operação, ele fixa `AWS_PROFILE` e `--profile` a partir do alias, remove variáveis AWS herdadas que poderiam ganhar precedência e verifica a conta com STS antes de cada execução. Cache e lock vivem em `targets/<alias>/auth/` e `targets/<alias>/.session-cache`.

O Torii não renova esse profile. Se a sessão expirar ou a conta estiver incorreta, um humano autentica o profile pelo mecanismo nativo do AWS CLI — por exemplo, SSO — e o agente repete a mesma chamada e o mesmo alias.

## Estratégias reservadas

`session_command` e `credential_file` são desserializadas, mas retornam erro explícito em runtime. Elas não constituem suporte implementado.

## Cache e lock

`.session-cache` guarda o epoch da última validação bem-sucedida. O TTL vem de `auth.cache_ttl_seconds`, padrão 300 segundos. Um mutex assíncrono por provider de lifecycle impede janelas concorrentes; em `aws_profile`, o mutex é por alias. A sessão é conferida novamente dentro do lock.

## Persistência

Credenciais `environment` ficam em `auth/credentials.env` do escopo. A escrita usa arquivo temporário, flush e persistência atômica. Uma tentativa cancelada ou inválida não substitui o arquivo anterior.
