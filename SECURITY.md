# Política de segurança

## Versões suportadas

Correções de segurança são aplicadas sobre `main` e publicadas em uma nova tag `vX.Y.Z`.
Não há suporte a versões anteriores à última release.

## Como reportar

Use o [reporte privado de vulnerabilidades](https://github.com/torii-mcp/torii/security/advisories/new)
do GitHub. Não abra issue pública para falhas de segurança.

Inclua, quando possível:

- versão do Torii (`torii --version`) e sistema operacional;
- `provider.yaml` e `rules.yaml` mínimos que reproduzem o problema, sempre com contas,
  clusters e credenciais fictícios;
- a sequência exata de chamadas MCP e ações humanas;
- o comportamento observado e o esperado.

Nunca inclua credenciais reais, conteúdo de `auth/credentials.env`, tokens de sessão ou
saída completa de comandos.

## O que consideramos vulnerabilidade

Uma quebra dos invariantes documentados em
[Modelo de segurança](docs/src/concepts/security-model.md), por exemplo:

- uma chamada atravessar sem casar `accept`, grant ativo ou aprovação humana;
- um `deny` explícito ser vencido por accept, grant ou aprovação;
- o agente escolher target, context, profile ou conta fora dos aliases humanos;
- credencial ou sessão vazar para o processo do Torii, para a auditoria, para o log ou
  para outro provider/target;
- política ser avaliada depois da leitura de credenciais;
- argumentos serem reconstruídos como linha de shell;
- um alias target-aware ser usado sem lease humano vivo.

## O que não consideramos vulnerabilidade

- ausência de sandbox do sistema operacional: o hook de agente reduz bypass acidental e
  não substitui isolamento real;
- um agente executar algo que a política local aceita explicitamente;
- permissões concedidas pelo provedor de nuvem à credencial usada;
- `session_command` e `credential_file` retornarem erro: são reconhecidos no schema e não
  implementados.
