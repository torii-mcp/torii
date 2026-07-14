# API MCP

Torii usa MCP por `stdio` e anuncia somente a capability de tools. Cada provider carregado aparece exatamente uma vez.

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

A tool como `kubectl` exige também `target`. O enum é construído com os aliases carregados no startup:

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

Sem targets cadastrados, o schema torna `target` impossível de satisfazer até o humano criar um alias e reiniciar o servidor.

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

Não há tool de shutdown, reauth ou edição de targets. O cliente controla o processo; o humano usa a CLI de controle fora do MCP.
