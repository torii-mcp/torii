# Solução de problemas

## `no providers installed`

O registry não encontrou `provider.yaml` sob `providers/`. Execute `torii provider list`, instale um pacote, confirme `TORII_CONFIG_DIR` e use `torii config-dir` para verificar a raiz efetiva. `torii init` sozinho não instala providers.

## `rules file not found`

O provider existe, mas não possui `rules.yaml`. Crie uma política explícita; não há fallback permissivo.

## Chamada não resolvida é negada sem janela

Verifique `TORII_NO_GUI`. Qualquer valor não vazio diferente de `0` desabilita GUI. Em headless, default deny é o comportamento esperado. Variáveis do AWS Gate não alteram o Torii.

## Sessão AWS não renova

Confirme que `aws` está no `PATH`, que as três variáveis foram preenchidas e que `aws sts get-caller-identity` funciona com a mesma rede. Torii suprime stdout/stderr do comando de validação para não vazar detalhes; use o AWS CLI diretamente como humano para diagnóstico.

## `inherited authentication cannot be renewed`

O provider não possui coleta gerenciada. Renove a sessão pelo CLI humano ou pelo credential store correspondente e reinicie/aguarde expirar o cache conforme necessário.

## Saída truncada

`execution.truncated: true` indica que stdout e/ou stderr excederam `max_output_bytes`. Aumente o limite com cautela ou restrinja a consulta no provider.

## `dlltool.exe` ou `as.exe` ausente

No Windows GNU, coloque uma distribuição MinGW-w64 completa no `PATH` durante build/test.

## Cliente MCP não inicializa

- execute o binário diretamente e verifique erros em stderr;
- confirme que nenhum wrapper escreve banner no stdout;
- valide o path absoluto e `TORII_CONFIG_DIR` na configuração do cliente;
- use `torii provider list` fora da sessão MCP para testar o registry.
