# Solução de problemas

## `no providers installed`

O registry não encontrou `provider.yaml` sob `providers/`. Execute `torii provider list`, instale um pacote, confirme `TORII_CONFIG_DIR` e use `torii config-dir` para verificar a raiz efetiva. `torii init` sozinho não instala providers.

## `rules file not found`

O provider existe, mas não possui `rules.yaml`. Crie uma política explícita; não há fallback permissivo.

## Chamada não resolvida é negada sem janela

Verifique `TORII_NO_GUI`. Qualquer valor não vazio diferente de `0` desabilita GUI. Em headless, default deny é o comportamento esperado. Variáveis do AWS Gate não alteram o Torii.

## Sessão AWS não renova

Confirme que `aws` está no `PATH`, que as três variáveis foram preenchidas e que `aws sts get-caller-identity` funciona com a mesma rede. Torii suprime stdout/stderr do comando de validação para não vazar detalhes; use o AWS CLI diretamente como humano para diagnóstico.

## Alias `aws_profile` pede autenticação humana ou informa conta divergente

O Torii não revela o profile ou os números de conta ao agente e não tenta trocar a sessão. No terminal humano, autentique o profile configurado no alias pelo mecanismo correspondente, por exemplo `aws sso login --profile empresa-producao`, e confirme `aws sts get-caller-identity --profile empresa-producao`. Depois repita a chamada MCP com o mesmo alias. Revise `torii target show aws_profile <alias>` se a conta esperada mudou.

## Alias target-aware inativo ou negado em headless

Criar um alias não cria seu lease. Confirme `torii target status <tool>` e ative explicitamente com `torii target activate <tool> <alias> --for 30`. A duração permitida vai de 1 a 1.440 minutos; sem `--for`, usa `default_target_minutes`. Em `TORII_NO_GUI=1`, uma chamada a alias inativo é negada por segurança, pois não há humano para aprovar o binding.

## Muitos aliases ativos

`target activate <tool> <alias>` substitui os ativos por padrão. Se alguém usou `--add`, o agente pode escolher qualquer alias ainda ativo em operações permitidas. Inspecione `torii target status <tool>` e use `torii target clear <tool>` ou uma nova ativação sem `--add` para reduzir o conjunto. Clear não apaga grants, credenciais, cache ou configuração e não mata processos já em execução.

## Estado de leases inválido

Um `.target-authorizations.yaml` corrompido falha fechado: nenhuma chamada target-aware prossegue. Use `torii target clear <tool>` para regravar atomicamente um conjunto vazio válido; o comando não precisa desserializar o estado anterior e não altera grants, credenciais, cache ou targets.

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
