# Modelo de segurança

O Torii reduz a superfície de execução disponível ao agente. Ele não transforma um CLI ou credencial de alto privilégio em algo seguro por si só; política local e privilégios remotos precisam ser configurados em conjunto.

## Invariantes

1. **Default deny.** Ausência de regra ou grant não significa permissão.
2. **Deny prioritário.** Um deny compatível encerra a avaliação antes de qualquer aprovação.
3. **Autorização antes de autenticação.** `.env`, credenciais e cache não são lidos no caminho negado.
4. **Sem shell.** O programa recebe argumentos via `Command::args`.
5. **Credencial por processo.** Material coletado pelo Torii é sobreposto somente nos filhos de validação e execução.
6. **Substituição após validação.** Reauth falho preserva a sessão anterior.
7. **Concorrência serializada por escopo.** Chamadas simultâneas compartilham um lock por provider ou target.
8. **Auditoria sanitizada.** Logs usam uma referência curta e não armazenam clipboard, credenciais ou saída completa.
9. **Target sob controle humano.** O alias resolve para configuração local; flags de troca de context, identidade e endpoint são bloqueadas.
10. **Política pertence ao operador.** Install cria rules vazio; setup é o único writer e update nunca toca em rules ou estado operacional.

## Ordem crítica

```text
validar envelope MCP
        |
resolver provider/target e bloquear overrides
        |
carregar rules.yaml
        |
deny / accept / grant / aprovação
        |
somente se permitido: carregar env e sessão
        |
executar provider
```

Um refactor que antecipe a leitura de credenciais para antes da decisão é uma regressão de segurança, mesmo que o comando continue sendo bloqueado depois.

## Ambiente herdado

O processo filho herda o ambiente do processo Torii e recebe por cima `.env` e a sessão do provider. O Torii não chama `env_clear`. Portanto:

- não inicie o servidor com segredos globais desnecessários;
- use variáveis como `AZURE_CONFIG_DIR` e `CLOUDSDK_CONFIG` no `.env` do provider quando o CLI suportar isolamento;
- execute o Torii sob a mesma conta local confiável que controla seus arquivos de configuração.

## Limites

A versão atual não oferece sandbox de sistema operacional, timeout de processo, streaming de saída, daemon multiusuário, assinatura de providers ou distribuição remota. O runner limita o conteúdo devolvido ao agente, mas o processo pode produzir mais dados internamente antes da captura terminar.
