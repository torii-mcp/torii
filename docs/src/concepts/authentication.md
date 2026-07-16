# Sessões de autenticação

Autenticação é um lifecycle por provider, separado do Jasper. Um target herda o lifecycle do provider indicado em seu campo `provider`.

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

## Estratégias reservadas

`session_command` e `credential_file` são desserializadas, mas retornam erro explícito em runtime. Elas não constituem suporte implementado.

## Cache e lock

`.session-cache` guarda o epoch da última validação bem-sucedida. O TTL vem de `auth.cache_ttl_seconds`, padrão 300 segundos. Um mutex assíncrono por provider de lifecycle impede janelas concorrentes; a sessão é conferida novamente dentro do lock.

## Persistência

Credenciais `environment` ficam em `auth/credentials.env` do escopo. A escrita usa arquivo temporário, flush e persistência atômica. Uma tentativa cancelada ou inválida não substitui o arquivo anterior.
