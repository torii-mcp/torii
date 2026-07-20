# API MCP

Torii usa MCP por `stdio` e anuncia a capability de tools. Cada provider carregado aparece exatamente uma vez, além da tool fixa e somente de leitura `torii_policy`.

## Consulta de política

Antes de escolher uma operação, o agente pode consultar as regras ativas sem executar provider, ler ambiente, cache ou credenciais:

```json
{
  "name": "torii_policy",
  "arguments": { "provider": "aws" }
}
```

Para uma tool target-aware, informe o alias anunciado:

```json
{
  "name": "torii_policy",
  "arguments": { "provider": "kubectl", "target": "mpce_dev" }
}
```

A resposta contém `accept`, `deny`, `minimum_accept_tokens` e `ignored_accept`. Quando existir `targets/<alias>/rules.yaml`, ela é a política devolvida, pois substitui a regra compartilhada. `unmatched` explica que comandos fora dessas listas continuam em default deny, salvo aprovação humana ou grant temporário ativo. A consulta não mostra credenciais, ambiente, grants ou leases e não modifica estado; ela funciona mesmo quando o alias está inativo.

## Provider simples

Uma tool como `aws` exige apenas o vetor de argumentos:

```json
{
  "type": "object",
  "required": ["args"],
  "properties": {
    "args": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
  },
  "additionalProperties": false
}
```

Chamada:

```json
{
  "name": "aws",
  "arguments": { "args": ["s3", "ls"] }
}
```

## Provider target-aware

Uma tool como `kubectl` ou `aws_profile` exige também `target`. O enum é construído com todos os aliases carregados no startup, ativos ou inativos:

```json
{
  "type": "object",
  "required": ["target", "args"],
  "properties": {
    "target": { "type": "string", "enum": ["mpce_dev"] },
    "args": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
  },
  "additionalProperties": false
}
```

Sem targets cadastrados, o schema torna `target` impossível de satisfazer até o humano criar um alias e reiniciar o servidor. Criar um alias não o ativa; o lease é avaliado pelo dispatcher, não pelo schema.

Chamada:

```json
{
  "name": "kubectl",
  "arguments": {
    "target": "mpce_dev",
    "args": ["get", "pods", "-n", "agente-rm"]
  }
}
```

Campos extras, `args` ausente ou vazio, target ausente/desconhecido e itens não string são recusados. Tool desconhecida também é erro de parâmetros.

Para um alias conhecido, mas sem lease válido, o dispatcher mostra ao humano o binding solicitado e os aliases ativos. O humano pode substituir os ativos pelo solicitado, adicioná-lo conscientemente ao conjunto ou negar. Em headless, a decisão é negação. O prompt acontece depois de um deny explícito ser descartado e antes de grants Jasper, ambiente, autenticação ou execução. O agente não recebe uma tool para ativar, limpar ou consultar leases.

Em `aws_profile`, o enum contém apenas aliases humanos. O profile AWS, a conta esperada e a região configurados no alias nunca são campos MCP. Se a identidade do profile estiver indisponível ou pertencer a outra conta, a resposta de erro pede intervenção humana sem revelar esses valores.

## Resposta

```json
{
  "provider": "kubectl",
  "target": "mpce_dev",
  "decision": {
    "result": "allow",
    "source": "rules",
    "rule": "get pods"
  },
  "execution": {
    "exit_code": 0,
    "stdout": "...",
    "stderr": "",
    "truncated": false
  }
}
```

`target` é omitido para providers simples. `execution` é omitido quando a decisão nega a chamada. Falhas operacionais são devolvidas como tool result estruturado com `isError`; um exit code não zero do CLI continua sendo um resultado normal de execução.

## Lifecycle

Não há tool de shutdown, reauth, edição, ativação ou limpeza de targets. O cliente controla o processo; o humano usa a CLI de controle fora do MCP. Em uma chamada autorizada cuja sessão gerenciada não está disponível, o Torii solicita a autenticação humana automaticamente. A renovação proativa continua sendo `torii reauth <provider-tool> [target]` somente para autenticação gerenciada. Para `aws_profile`, o agente pede que o humano autentique o profile já configurado pelo fluxo nativo AWS e repete o mesmo alias; ele não pode trocar o profile.
