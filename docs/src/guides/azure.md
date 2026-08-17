# Configurar Azure

O pacote canônico é `az`; a fixture de desenvolvimento equivalente está em
`examples/providers/az/`. A tool MCP é `az` e recebe somente `args`: este provider não é
target-aware e não exige alias nem lease.

```powershell
torii provider install az
torii provider setup az readonly
```

## Sessão herdada do `az login`

```yaml
tool: az
command: az

policy:
  minimum_accept_tokens: 2

auth:
  strategy: inherited
  cache_ttl_seconds: 300
```

A Azure CLI guarda os tokens do `az login` em `~/.azure`. O Torii **herda** essa sessão: não
coleta campos, não grava credenciais e não substitui o token. Consequências operacionais:

- quem autentica é o humano, no terminal, com `az login` (ou `az login --tenant <tenant>`);
- `torii reauth az` não tem material renovável para trocar — a sessão é do CLI, não do Torii;
- não há validator declarado, então a auditoria registra `session-unchecked`: o Torii não
  confirma a validade da sessão antes de executar. Um token expirado aparece como erro do
  próprio `az` na resposta da chamada;
- a assinatura ativa é a do perfil local. O agente não escolhe assinatura, e nada impede que
  a assinatura ativa mude por fora — confira com `az account show` antes de liberar uso.

Se você precisa de isolamento por assinatura, prefira contas separadas no sistema
operacional a tentar alternar `az account set` durante o uso do agente.

## Política: dois tokens de piso

Comandos da Azure CLI têm a forma `grupo verbo` (`vm list`, `group show`), e o pacote define
`minimum_accept_tokens: 2` para um accept não ficar amplo demais. Grupos com subcomandos
sensíveis precisam de regras mais específicas do que dois tokens: `keyvault secret list`
libera nomes, `keyvault secret show` devolve o valor — são regras diferentes e o segundo
está no deny.

## O que o setup `readonly` nega

Além de não aceitar escrita, o setup nega explicitamente:

| Categoria | Exemplos negados |
|---|---|
| emissão de credencial ou token | `account get-access-token`, `aks get-credentials`, `acr login`, `storage account keys list`, `storage account show-connection-string`, `ad sp credential reset` |
| leitura do valor de segredo | `keyvault secret show`, `keyvault secret download`, `keyvault key download`, `keyvault certificate download` |
| execução remota | `vm run-command invoke`, `ssh vm` |
| canais que escapam da curadoria | `rest`, `interactive` |

`az rest` merece atenção: ele fala com a API do Azure Resource Manager diretamente, o que
tornaria toda a curadoria de grupos e verbos irrelevante. Mantenha-o no deny.

O accept cobre descoberta e leitura de metadados — `account`, `group`, `resource`, `vm`,
`vmss`, `disk`, `network`, `storage account` (apenas metadados), `aks`, `acr`, `webapp`,
`functionapp`, `keyvault` (cofre e nomes), `role assignment`, `policy`, `monitor` e `tag`.

Revise a lista conforme a classificação de dados da sua assinatura: uma operação de leitura
ainda pode revelar informação confidencial, por exemplo variáveis de aplicação em
`webapp config show`.

## Ambiente

Use o `.env` do provider para valores persistentes não secretos:

```env
AZURE_CORE_OUTPUT="json"
AZURE_CORE_ONLY_SHOW_ERRORS="true"
```

Não coloque credenciais nesse arquivo: a sessão vem do `az login`. Como o stdin do processo
filho é nulo, prefira comandos que não abram prompt interativo — outro motivo para
`interactive` estar negado.

## RBAC continua obrigatório

A política local decide se a tentativa atravessa; o RBAC da identidade decide o que ela pode
fazer de fato. Uma política permissiva não amplia RBAC, e um RBAC amplo não é compensado por
regras locais. Use um principal de menor privilégio para o uso do agente.
