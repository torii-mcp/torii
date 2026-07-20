# Auditoria

O arquivo global `torii.log` registra eventos em formato separado por ` | `:

```text
epoch | escopo | evento | regra-curta | detalhe-opcional
```

Exemplo:

```text
1784000000 | aws | allowed-by-rules | ec2 describe-instances
1784000001 | aws | session-ok | -
1784000002 | aws | ran | ec2 describe-instances | exit=0
1784000003 | kubectl/mpce_dev | ran | get pods | exit=0
```

Providers simples usam seu nome como escopo. Providers target-aware usam `provider/target`, o que permite distinguir decisões e sessões sem registrar o context real.

## Eventos comuns

- `invoke`;
- `allowed-by-rules` e `allowed-by-grant`;
- `denied-explicit` e `denied-interface`;
- `override-once` e `override-timed`;
- `invalid-accept`;
- `session-ok`, `session-unchecked`, `session-invalid`, `session-refreshed` e `session-candidate-invalid`;
- `preflight-provider`, `preflight-ok` e `preflight-failed`;
- `identity-ok`, `identity-mismatch` e `identity-check-failed`;
- `target-access-requested`, `target-access-denied`, `target-access-replaced`, `target-access-added`, `target-access-stale`, `target-access-lost`, `target-access-revoked` e `target-access-cleared`;
- `reauth-forced`;
- `ran` com exit code.

## Sanitização

Quebras de linha e `|` são substituídos. A referência de chamada é limitada aos dois primeiros argumentos para evitar registrar a linha completa. O log não contém credenciais, clipboard, stdout ou stderr.

Eventos de preflight registram somente a tool do provider autenticador. Seus argumentos, ambiente e saída não são registrados.

Eventos `identity-*` registram somente o escopo de auditoria e o resultado da conferência. Identidade esperada, identidade observada e saída do probe não entram no log; elas viajam apenas no erro devolvido à chamada.

Eventos `target-access-*` registram somente o alias ou `-` na regra curta. O detalhe contém duração, contagem de aliases ou um rótulo estático de fase/resultado — por exemplo, `before-environment`, `after-authentication`, `at-launch`, `retry-required`, `already-active`, `target-removed` ou `all` — nunca binding, profile, conta, context, argumentos completos ou conteúdo do arquivo de leases. `target-access-stale` indica que o estado mudou enquanto a janela estava aberta: a escolha antiga não é aplicada; se o alias já estiver ativo no novo estado a chamada pode continuar, caso contrário é negada e requer nova tentativa. `target-access-lost` indica expiração ou revogação em uma das rechecagens antes de ambiente, autenticação ou launch. `target-access-revoked` registra a revogação preventiva do lease quando o control plane remove um target.

A escrita é best-effort: uma falha de auditoria não interrompe a operação. Portanto, este arquivo é observabilidade local, não ledger inviolável ou mecanismo de compliance por si só.

## Proteção

Proteja `torii.log` com as permissões do diretório de configuração. Embora ele não deva conter segredos, revela providers, ações tentadas e horários operacionais.
