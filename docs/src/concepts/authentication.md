# Sessões de autenticação

Autenticação é um lifecycle por escopo, separado do Jasper. O escopo é a raiz de um provider simples ou o diretório de um target.

## `environment`

Implementada. O provider declara campos, templates de injeção e validação. O fluxo é:

1. carregar a sessão existente após a autorização;
2. usar cache curto se houver validação recente;
3. executar o comando de validação;
4. se inválida, abrir a janela gerada pelos campos;
5. validar a candidata em processo filho;
6. persistir atomicamente e atualizar o cache;
7. devolver as variáveis para a execução autorizada.

Campos secretos usam input mascarado. O botão de clipboard aceita `export KEY=value`, `SET KEY=value`, `$Env:KEY=value` e `KEY=value`, mantendo somente nomes declarados.

## `inherited`

Implementada. Torii não coleta material. Se `validate` existir, ele roda com `.env` e o ambiente herdado. Sem `validate`, a sessão é considerada válida e cacheada. `torii reauth` não se aplica a essa estratégia.

Use `.env` para apontar stores isolados do provider quando disponível, por exemplo:

```env
AZURE_CONFIG_DIR="C:/Users/voce/.config/torii/providers/az/auth/azure"
```

## Estratégias reservadas

`session_command` e `credential_file` são desserializadas, mas retornam erro explícito em runtime. Elas não constituem suporte implementado.

## Cache e lock

`.session-cache` guarda o epoch da última validação bem-sucedida. O TTL vem de `auth.cache_ttl_seconds`, padrão 300 segundos. Um mutex assíncrono por escopo impede janelas concorrentes; a sessão é conferida novamente dentro do lock.

## Persistência

Credenciais `environment` ficam em `auth/credentials.env` do escopo. A escrita usa arquivo temporário, flush e persistência atômica. Uma tentativa cancelada ou inválida não substitui o arquivo anterior.
