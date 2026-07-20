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

Quando instalado, o pacote de aliases AWS acrescenta `aws_profile`. Seus targets anunciados são aliases humanos; profile e conta não aparecem para o cliente MCP. Todo alias configurado continua no schema, mesmo antes de um humano conceder seu lease temporário.

Além de uma tool por provider, o Torii publica `torii_policy`, uma consulta somente de leitura das regras `accept` e `deny` ativas. Não existem tools MCP de `kill`, `reauth`, instalação ou edição de política.

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

## Descobrir a política antes de executar

O agente deve consultar `torii_policy` antes de escolher uma operação, em especial quando a allowlist não é conhecida:

```json
{
  "provider": "aws"
}
```

Para uma tool com target, inclua o alias:

```json
{
  "provider": "kubectl",
  "target": "mpce_dev"
}
```

A consulta não executa o provider nem lê sessão ou lease. Regras fora de `accept` e `deny` continuam em default deny até uma aprovação humana ou grant temporário.

Se um alias target-aware estiver inativo, o Torii pede a decisão humana sobre o binding antes de grants, ambiente e autenticação. Em headless, a chamada é negada. O agente não deve tentar ativar, limpar ou substituir aliases; aguarda a decisão humana. Se o humano mantiver mais de um alias ativo, o agente ainda pode escolher qualquer um deles nas operações permitidas.

Se uma chamada `aws_profile` autorizada informar que a identidade não corresponde ao alias, peça que o humano autentique o profile configurado pelo AWS CLI e então repita a mesma chamada. Não tente enviar `--profile`, `--region` ou escolher outro target.
