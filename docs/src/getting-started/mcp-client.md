# Conectar um cliente MCP

O cliente é responsável por iniciar e encerrar o Torii. O transporte é `stdio`; fechar stdin ou terminar o processo encerra a sessão.

Configuração genérica:

```json
{
  "mcpServers": {
    "torii": {
      "command": "C:/tools/torii.exe",
      "env": {
        "TORII_CONFIG_DIR": "C:/Users/voce/.config/torii"
      }
    }
  }
}
```

Em Unix, use o caminho correspondente:

```json
{
  "mcpServers": {
    "torii": {
      "command": "/home/voce/bin/torii"
    }
  }
}
```

## Descoberta de tools

No startup, Torii carrega os subdiretórios de `providers/`. Cada `provider.yaml` válido produz uma tool. Alterações em providers exigem reiniciar o servidor para reconstruir o registry.

O conjunto inicial costuma ser:

```text
aws
kubectl
```

Não existem tools MCP de `kill`, `reauth`, instalação ou edição de política.

## Integridade do stdout

Stdout pertence ao protocolo. Não envolva o Torii com scripts que imprimam banners, mensagens de login ou diagnósticos no mesmo stream. Mensagens humanas devem ir para stderr; a auditoria fica em `torii.log`.

## Chamada

Providers simples usam:

```json
{
  "args": ["s3", "ls"]
}
```

Providers target-aware exigem o alias anunciado no schema:

```json
{
  "target": "mpce_dev",
  "args": ["get", "pods", "-n", "agente-rm"]
}
```

Não envie uma linha inteira em um campo `command` e não junte argumentos que deveriam ser itens separados. Veja a [API MCP](../reference/mcp-api.md) para respostas e erros.
